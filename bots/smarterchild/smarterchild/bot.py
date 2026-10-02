"""The bot: stays signed on, accepts every buddy request, and answers IMs.

Settings come from the environment (or a .env file beside the data folder):
  HOTLINE_HOST            default hotline.vespernet.net
  HOTLINE_PORT            default 5500
  SMARTERCHILD_LOGIN      the account's screen name (required)
  SMARTERCHILD_PASSWORD   its password (required)
  SMARTERCHILD_NAME       the name buddies see, default SmarterChild
  SMARTERCHILD_STATUS     the status line, default: Ask me anything! Type "help".
  SMARTERCHILD_DATA       where memory and reminders are kept, default ./data
  SMARTERCHILD_ICON       a Buddy Icon file (GIF/PNG/JPEG, 64x64 at most); default the robot in
                          assets/, empty for none

And in a server's public chat (leave HUB_HOST empty to stay out of chat):
  HUB_HOST, HUB_PORT      the server, e.g. the Hotline Central Hub; port default 5500. Several
                          servers: comma-separated, each host or host:port
  HUB_LOGIN, HUB_PASSWORD an account there (on each, with several), or empty to join as a guest
  HUB_ICON                the classic user icon, default 168 (the robot)
  HUB_TRIGGER             what starts a chat command, default ! (as in !weather Boston)
"""

from __future__ import annotations

import asyncio
import logging
import os
import random
import time
from collections import defaultdict, deque
from pathlib import Path

from . import hotline, skills
from .brain import Brain
from .hotline import Client, F, HotlineError, Tx

log = logging.getLogger("smarterchild")


def load_env(path: Path) -> None:
    """KEY=value lines, without overriding the real environment."""
    try:
        for line in path.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                k, v = line.split("=", 1)
                os.environ.setdefault(k.strip(), v.strip().strip('"').strip("'"))
    except OSError:
        pass


class Bot:
    def __init__(self, host: str, port: int, login: str, password: str, name: str, status: str, data: Path,
                 brain=None, app_string: str = "SmarterChild 0.1", welcome: str | None = None,
                 bang_commands: bool = True, icon: bytes | None = None):
        """`brain` answers messages (`answer(login, text) -> [replies]`); SmarterChild's own by
        default. BugBot brings its own, a welcome line, and no "!" commands or reminders."""
        self.host, self.port, self.login, self.password = host, port, login, password
        self.name, self.status = name, status
        self.brain = brain if brain is not None else Brain(data, name)
        self.brain.send = self.send
        self.app_string, self.welcome_text, self.bang_commands = app_string, welcome, bang_commands
        self.icon = icon  # its Buddy Icon (a small GIF, PNG or JPEG), set at sign-on
        self.client: Client | None = None
        self.recent: dict[str, deque] = defaultdict(deque)  # login -> times of recent messages
        self.warned: dict[str, float] = {}
        self.locks: dict[str, asyncio.Lock] = defaultdict(asyncio.Lock)
        self.taking: dict[bytes, tuple[str, str]] = {}  # transfer GUID -> (login, file name)

    async def send(self, to: str, text: str) -> None:
        if not self.client or self.client.closed.is_set():
            raise HotlineError("Not signed on.")
        for part in split(text, self.client.max_message_bytes - 16, self.client.enc):
            await self.client.send_im(to, part)

    async def on_event(self, kind: str, data: dict) -> None:
        if kind == "message":
            asyncio.create_task(self.on_message(data["message"]))
        elif kind == "friend_request":
            asyncio.create_task(self.welcome(data["login"]))
        elif kind == "file_offer":
            asyncio.create_task(self.on_file_offer(data["offer"]))
        elif kind == "file_ready" and data["guid"] in self.taking:
            asyncio.create_task(self.on_file_ready(data["guid"], data["relay_ref"]))

    async def on_file_offer(self, offer: hotline.FileOffer) -> None:
        """Files go to a brain that wants them (BugBot's screenshots); the rest are turned down."""
        assert self.client
        want = getattr(self.brain, "wants_file", None)
        ok, reply = want(offer.sender, offer.name, offer.size) if want else (
            False, "I can't take files, sorry! Words only.")
        try:
            if ok:
                self.taking[offer.guid] = (offer.sender, offer.name)
                await self.client.accept_file(offer.guid)
            else:
                await self.client.decline_file(offer.guid)
        except HotlineError as e:
            log.warning("file from %s: %s", offer.sender, e)
            self.taking.pop(offer.guid, None)
            return
        if reply:
            await self.send(offer.sender, reply)

    async def on_file_ready(self, guid: bytes, relay_ref: int) -> None:
        assert self.client
        login, name = self.taking.pop(guid)
        try:
            name, data = await self.client.receive_file(relay_ref, self.brain.MAX_FILE)
            replies = await self.brain.got_file(login, name, data)
        except hotline.FileTooBig:
            replies = ["That file's too big for me, sorry."]
        except Exception as e:
            log.warning("receiving %s from %s: %s", name, login, e)
            replies = ["That file didn't come through. Want to try sending it again?"]
        async with self.locks[login.lower()]:
            for r in replies:
                await self.send(login, r)

    async def welcome(self, login: str) -> None:
        assert self.client
        try:
            await self.client.accept(login)
            log.info("accepted %s", login)
            await asyncio.sleep(1.5)
            await self.send(login, self.welcome_text or (
                f"Thanks for adding me! I'm {self.name}. Ask me for the weather, a word, some math, "
                "or a game of trivia. Type \"help\" to see everything."))
        except HotlineError as e:
            log.warning("couldn't accept %s: %s", login, e)

    def flooding(self, login: str) -> bool:
        q, now = self.recent[login.lower()], time.time()
        q.append(now)
        while q and now - q[0] > 60:
            q.popleft()
        return len(q) > 20

    async def on_message(self, m: hotline.Message) -> None:
        assert self.client
        c = self.client
        c.ack(m.guid, m.sender, read=True)
        if self.flooding(m.sender):
            if time.time() - self.warned.get(m.sender.lower(), 0) > 60:
                self.warned[m.sender.lower()] = time.time()
                await self.send(m.sender, "Whoa, slow down! Give me a minute to catch up.")
            return
        async with self.locks[m.sender.lower()]:  # one conversation at a time per person
            late = m.timestamp and time.time() - m.timestamp > 600
            c.typing(m.sender, True)
            started = time.time()
            try:
                text = m.body
                if self.bang_commands:
                    from .hub import command  # "!weather Boston" works in IM too, out of habit
                    text = command(m.body.strip()) or m.body
                replies = await self.brain.answer(m.sender, text)
            except Exception:
                log.exception("answering %s", m.sender)
                replies = ["Oops, something went wrong in my circuits. Try that again?"]
            if late:
                replies.insert(0, "Sorry, I was offline when you sent that!")
            # Look like we're typing for a second or two: SmarterChild never answered instantly.
            await asyncio.sleep(max(0.0, typing_time(replies[0]) - (time.time() - started)))
            c.typing(m.sender, False)
            for i, r in enumerate(replies):
                if i:  # and again before each follow-up
                    c.typing(m.sender, True)
                    await asyncio.sleep(typing_time(r) * 0.7)
                    c.typing(m.sender, False)
                try:
                    await self.send(m.sender, r)
                except HotlineError as e:
                    log.warning("reply to %s failed: %s", m.sender, e)
                    break
            log.info("%s: %r -> %d repl%s", m.sender, m.body[:60], len(replies), "y" if len(replies) == 1 else "ies")

    async def session(self) -> None:
        c = Client(self.host, self.port, self.login, self.password, nickname=self.name,
                   app_string=self.app_string, on_event=self.on_event)
        await c.connect()
        self.client = c
        log.info("signed on to %s as %s (%s)", c.server_name or self.host, self.login, c.transport)
        await c.set_presence(hotline.ONLINE, self.status, discoverable=True)
        await self.publish_name()
        await self.publish_icon()
        for b in await c.get_roster():
            if b.state == hotline.PENDING_IN:
                asyncio.create_task(self.welcome(b.login))
        while not c.closed.is_set():
            if isinstance(self.brain, Brain):
                await skills.run_reminders(self.brain)
            try:
                await asyncio.wait_for(c.closed.wait(), 5)
            except asyncio.TimeoutError:
                pass
        log.warning("signed off: %s", c.close_reason)

    async def publish_name(self) -> None:
        """The name buddies see is the profile's nickname (Set User Info replaces it all)."""
        assert self.client
        try:
            await self.client.request(826, [(0x0614, self.client.enc(self.name))])
        except HotlineError as e:
            log.info("couldn't set the display name: %s", e)

    async def publish_icon(self) -> None:
        """Sets the Buddy Icon, unless the server already has this one."""
        c = self.client
        if not self.icon or not c or c.max_icon_bytes is None:
            return
        try:
            if await c.own_icon_hash() == c.icon_hash(self.icon):
                return
            await c.set_buddy_icon(self.icon)
            log.info("set the Buddy Icon (%d bytes)", len(self.icon))
        except HotlineError as e:
            log.info("couldn't set the Buddy Icon: %s", e)

    async def run(self) -> None:
        wait = 5
        while True:
            start = time.time()
            try:
                await self.session()
            except HotlineError as e:
                log.error("sign-on failed: %s", e)
                if "Incorrect" in str(e):
                    wait = max(wait, 300)  # don't hammer the server with a bad password
            except (OSError, asyncio.TimeoutError) as e:
                log.error("connection problem: %s", e)
            except Exception:
                log.exception("unexpected error")
            if time.time() - start > 600:
                wait = 5  # it had been up a while: reconnect quickly
            log.info("reconnecting in %ds", wait)
            await asyncio.sleep(wait + random.random() * 2)
            wait = min(wait * 2, 300)


def load_icon(path: str | None, bundled: str) -> bytes | None:
    """A Buddy Icon: the file at `path` if given (empty: none), else the one in assets/."""
    if path == "":
        return None
    try:
        return Path(path).read_bytes() if path else (Path(__file__).parent / "assets" / bundled).read_bytes()
    except OSError as e:
        log.warning("no Buddy Icon: %s", e)
        return None


def typing_time(reply: str) -> float:
    """How long to look busy before a reply: a second or two, longer for longer replies."""
    return min(1.0 + len(reply) / 250, 2.5) + random.uniform(0, 0.4)


def split(text: str, limit: int, enc) -> list[str]:
    """Pieces that each fit in one message, broken at lines or spaces."""
    out, cur = [], ""
    for line in text.split("\n"):
        cand = f"{cur}\n{line}" if cur else line
        if len(enc(cand)) <= limit:
            cur = cand
            continue
        if cur:
            out.append(cur)
        cur = ""
        while len(enc(line)) > limit:
            cut = line[:limit].rsplit(" ", 1)[0] or line[:limit]
            out.append(cut)
            line = line[len(cut):].lstrip()
        cur = line
    if cur:
        out.append(cur)
    return out or [""]


def main() -> None:
    logging.basicConfig(level=os.environ.get("LOG_LEVEL", "INFO"),
                        format="%(asctime)s %(levelname)s %(name)s: %(message)s")
    data = Path(os.environ.get("SMARTERCHILD_DATA", "data"))
    load_env(Path(".env"))
    login, password = os.environ.get("SMARTERCHILD_LOGIN"), os.environ.get("SMARTERCHILD_PASSWORD")
    if not login or not password:
        raise SystemExit("Set SMARTERCHILD_LOGIN and SMARTERCHILD_PASSWORD (in the environment or a .env file).")
    bot = Bot(os.environ.get("HOTLINE_HOST", "hotline.vespernet.net"), int(os.environ.get("HOTLINE_PORT", "5500")),
              login, password, os.environ.get("SMARTERCHILD_NAME", "SmarterChild"),
              os.environ.get("SMARTERCHILD_STATUS", 'Ask me anything! Type "help".'), data,
              icon=load_icon(os.environ.get("SMARTERCHILD_ICON"), "smarterchild.png"))
    hubs = []
    for entry in filter(None, (h.strip() for h in os.environ.get("HUB_HOST", "").split(","))):
        from .hub import Hub
        host, _, port = entry.rpartition(":") if entry.count(":") == 1 else (entry, "", "")
        hubs.append(Hub(bot.brain, host, int(port or os.environ.get("HUB_PORT", "5500")), bot.name,
                        int(os.environ.get("HUB_ICON", "168")), os.environ.get("HUB_LOGIN", ""),
                        os.environ.get("HUB_PASSWORD", ""), os.environ.get("HUB_TRIGGER", "!")))

    async def both():
        await asyncio.gather(bot.run(), *(h.run() for h in hubs))

    try:
        asyncio.run(both())
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
