"""SmarterChild in a Hotline server's public chat (the Hotline Central Hub, say).

It speaks only when spoken to:
- a `!` command: "!weather Boston", "!news", "!define ennui", "!joke", "!help";
- a line that starts or ends with its name ("SmarterChild, what's 6*7?");
- the weather for a named place ("weather in Boston"), which is rarely just chatter.
Lines a relay posts for people on Discord ("Discord | Name: !weather Boston") count as theirs.
Private messages sent to it on that server are answered like IMs. Everything else
said in the room is left alone.
"""

from __future__ import annotations

import asyncio
import logging
import random
import re
import time
from collections import deque

from .brain import Brain
from .hotline import Client, HotlineError

log = logging.getLogger("hub")

# "\r%13s:  message" for chat; " *** name action" for an emote.
CHAT_LINE = re.compile(r"^\s*(.{1,31}?):\s+(.*)$", re.S)


# "!w Boston" and friends: short command names for what the brain already understands.
COMMANDS = {
    "w": "weather", "wx": "weather", "forecast": "weather", "d": "define", "def": "define", "dict": "define",
    "wiki": "tell me about", "wp": "tell me about", "whois": "who is", "t": "time in", "time": "time in",
    "calc": "", "math": "", "c": "", "8ball": "8 ball", "8": "8 ball", "otd": "on this day",
    "history": "on this day", "rooms": "chat rooms", "servers": "chat rooms", "headlines": "news",
    "happynews": "happy news", "goodnews": "happy news", "happy": "happy news", "good": "happy news",
    "sc": "", "smarterchild": "", "help": "help", "commands": "help", "about": "who are you",
    "whoami": "who are you", "info": "who are you",
    "city": "city", "place": "city", "town": "city", "where": "city",
    "score": "trivia score", "top": "leaderboard", "leaderboard": "leaderboard", "trivia": "trivia",
}
# A relay speaking for someone elsewhere, like the Discord bridge: "Discord | Name: message".
RELAYED = re.compile(r"^(Discord|Web)\s*\|\s*(.{1,40}?):\s+(.*)$", re.S)
NATURAL = re.compile(r"(?:what'?s |how'?s )?(?:the )?(?:weather|forecast)(?: like)? (?:in|for|at) .{2,60}", re.I)


def command(text: str, trigger: str = "!") -> str | None:
    """The question in a "!command args" line, with short names spelled out; else None."""
    if not trigger or not text.startswith(trigger) or len(text) <= len(trigger):
        return None
    word, _, rest = text[len(trigger):].strip().partition(" ")
    if not word:
        return None
    w = word.lower()
    if w in COMMANDS:
        head = COMMANDS[w]
        if w in ("time", "t") and not rest:
            return "what time is it"
        return f"{head} {rest}".strip() or "help"
    return f"{word} {rest}".strip()


def addressed(text: str, names: list[str]) -> str | None:
    """What was asked, when a line starts or ends with one of our names; else None."""
    alt = "|".join(re.escape(n) for n in names)
    m = re.fullmatch(rf"@?(?:{alt})\b[\s,:;!.-]*(.*)", text, re.I | re.S) or \
        re.fullmatch(rf"(.*?)[\s,;:-]*@?(?:{alt})[\s?!.]*", text, re.I | re.S)
    return m[1].strip() if m else None


class Hub:
    def __init__(self, brain: Brain, host: str, port: int, name: str, icon: int,
                 login: str = "", password: str = "", trigger: str = "!"):
        self.brain, self.host, self.port = brain, host, port
        self.name, self.icon, self.login, self.password = name, icon, login, password
        self.trigger = trigger
        self.names = sorted({name, name.replace(" ", ""), "smarterchild", "smarter child"}, key=len, reverse=True)
        self.client: Client | None = None
        self.sent: deque = deque()  # times of our recent chat lines
        self.per_user: dict[str, deque] = {}

    def _room_ok(self, who: str) -> bool:
        """At most 8 chat replies a minute in all, and 4 for any one person."""
        now = time.time()
        for q, cap in ((self.sent, 8), (self.per_user.setdefault(who.lower(), deque()), 4)):
            while q and now - q[0] > 60:
                q.popleft()
            if len(q) >= cap:
                return False
        self.sent.append(now)
        self.per_user[who.lower()].append(now)
        return True

    async def on_event(self, kind: str, data: dict) -> None:
        # The public chat, whether or not the server puts a Chat ID on it (we join no private chats).
        if kind == "chat":
            asyncio.create_task(self.on_chat(data["text"]))
        elif kind == "private":
            asyncio.create_task(self.on_private(data["id"], data["name"], data["text"]))

    async def on_chat(self, raw: str) -> None:
        line = raw.lstrip("\r\n")
        m = CHAT_LINE.match(line)
        if not m:
            return  # an emote or a server line
        who, text = m[1].strip(), m[2].strip()
        if who.lower() in (n.lower() for n in self.names):
            return  # ourselves
        key = f"hub:{who}"
        r = RELAYED.match(text)
        if r:  # answer the person on Discord, not the relay
            who, text = r[2].strip(), r[3].strip()
            key = f"{r[1].lower()}:{who}"
        q = command(text, self.trigger)
        if q is None:
            q = addressed(text, self.names)
        if q is None and NATURAL.fullmatch(text.rstrip("?!. ")):
            q = text
        if q is None:
            return
        if not self._room_ok(key):
            log.info("chat %s: rate limited", who)
            return
        assert self.client
        await asyncio.sleep(random.uniform(0.7, 1.6))
        replies = await self.brain.answer(key, q, room=True)
        lines = "\n".join(replies).split("\n")
        if len(lines) > 5:
            lines = lines[:4] + ["(There's more: IM me for the rest.)"]
        text = f"{who}: " + "\r".join(lines)
        self.client.send_chat(text[: self.client.max_message_bytes or 4000])
        log.info("chat %s: %r", who, q[:60])

    async def on_private(self, uid: int, who: str, text: str) -> None:
        assert self.client
        replies = await self.brain.answer(f"hub:{who}", command(text.strip(), self.trigger) or text)
        for i, r in enumerate(replies):
            if i:
                await asyncio.sleep(0.8)
            self.client.send_private(uid, r.replace("\n", "\r"))
        log.info("private %s: %r", who, text[:60])

    async def session(self) -> None:
        c = Client(self.host, self.port, self.login, self.password, nickname=self.name, icon=self.icon,
                   classic=True, app_string="SmarterChild 0.1", on_event=self.on_event)
        await c.connect()
        self.client = c
        log.info("in %s's chat as %s (icon %d, %s)", c.server_name or self.host, self.name, self.icon, c.transport)
        try:
            await c.get_users()
        except HotlineError:
            pass
        await c.closed.wait()
        log.warning("left %s: %s", self.host, c.close_reason)

    async def run(self) -> None:
        wait = 15
        while True:
            start = time.time()
            try:
                await self.session()
            except (HotlineError, OSError, asyncio.TimeoutError) as e:
                log.error("hub: %s", e)
            except Exception:
                log.exception("hub: unexpected error")
            if time.time() - start > 600:
                wait = 15
            await asyncio.sleep(wait + random.random() * 5)
            wait = min(wait * 2, 600)  # be gentle: servers ban clients that hammer them
