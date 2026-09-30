"""SmarterChild in a Hotline server's public chat (the Hotline Central Hub, say).

It speaks only when spoken to: a chat line that starts or ends with its name
("SmarterChild, weather in Boston" / "what's 6*7, smarterchild?"). Private messages
sent to it on that server are answered like IMs. Everyone else's chat is ignored.
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


def addressed(text: str, names: list[str]) -> str | None:
    """What was asked, when a line starts or ends with one of our names; else None."""
    alt = "|".join(re.escape(n) for n in names)
    m = re.fullmatch(rf"@?(?:{alt})\b[\s,:;!.-]*(.*)", text, re.I | re.S) or \
        re.fullmatch(rf"(.*?)[\s,;:-]*@?(?:{alt})[\s?!.]*", text, re.I | re.S)
    return m[1].strip() if m else None


class Hub:
    def __init__(self, brain: Brain, host: str, port: int, name: str, icon: int,
                 login: str = "", password: str = ""):
        self.brain, self.host, self.port = brain, host, port
        self.name, self.icon, self.login, self.password = name, icon, login, password
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
        if kind == "chat" and data.get("chat_id") is None:
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
        q = addressed(text, self.names)
        if q is None or not self._room_ok(who):
            return
        assert self.client
        await asyncio.sleep(random.uniform(0.7, 1.6))
        replies = await self.brain.answer(f"hub:{who}", q, room=True)
        lines = "\n".join(replies).split("\n")
        if len(lines) > 5:
            lines = lines[:4] + ["(There's more: IM me for the rest.)"]
        text = f"{who}: " + "\r".join(lines)
        self.client.send_chat(text[: self.client.max_message_bytes or 4000])
        log.info("chat %s: %r", who, q[:60])

    async def on_private(self, uid: int, who: str, text: str) -> None:
        assert self.client
        replies = await self.brain.answer(f"hub:{who}", text)
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
