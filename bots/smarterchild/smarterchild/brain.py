"""SmarterChild's brain: plain rules, no AI model. Each message goes to the game or
menu the person is in the middle of, else to the first skill whose pattern matches,
else to small talk, else to a shrug that points at "help"."""

from __future__ import annotations

import random
import re
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Awaitable, Callable

from .store import Memory, Store

Reply = "str | list[str] | None"
Handler = Callable[["Ctx", re.Match], Awaitable[Reply]]
ModeHandler = Callable[["Ctx"], Awaitable[Reply]]

QUIT = {"quit", "stop", "exit", "cancel", "nevermind", "never mind", "done", "q", "menu", "help"}

# Politeness and address that don't change what's being asked.
_LEAD = re.compile(
    r"^(?:(?:hey|hi|yo|ok|okay|so|um+|uh+)\s+)?(?:smarter ?child|sc|bot)?[,:]?\s*"
    r"(?:(?:please|pls|plz)\s+)?(?:(?:can|could|would|will) you\s+)?(?:(?:please|pls|plz)\s+)?",
    re.I)


def normalize(text: str) -> str:
    t = " ".join(text.strip().split())
    t = _LEAD.sub("", t, count=1) or t
    t = re.sub(r"[\s?!.]+$", "", t)
    t = re.sub(r"\s+(?:please|pls|plz|thx|thanks)$", "", t, flags=re.I)
    return t.strip()


@dataclass
class Skill:
    pattern: re.Pattern
    handler: Handler


@dataclass
class Mode:
    handler: ModeHandler
    state: dict
    started: float = field(default_factory=time.time)


class Ctx:
    """One incoming message, with what the brain knows about its sender."""

    def __init__(self, brain: "Brain", login: str, text: str, room: bool = False):
        self.brain, self.login, self.raw, self.room = brain, login, text, room
        self.text = normalize(text)
        self.low = self.text.lower()
        self.last_seen = 0

    @property
    def mem(self) -> dict:
        return self.brain.memory.of(self.login)

    @property
    def name(self) -> str:
        return self.mem.get("name") or self.login

    def remember(self, key: str, value) -> None:
        self.brain.memory.set(self.login, key, value)

    def start(self, handler: ModeHandler, **state) -> None:
        self.brain.modes[self.login.lower()] = Mode(handler, state)

    @property
    def state(self) -> dict:
        m = self.brain.modes.get(self.login.lower())
        return m.state if m else {}

    def end(self) -> None:
        self.brain.modes.pop(self.login.lower(), None)


class Brain:
    def __init__(self, data: Path, bot_name: str = "SmarterChild"):
        self.bot_name = bot_name
        self.data = data
        # Where to find the bot, for people who meet it outside Hotline IM.
        self.pitch = ("Add me as a buddy: get HIM, the Hotline Instant Messenger, at "
                      "https://github.com/tagban/him/releases and add \"smarterchild\" on VesperNet.")
        self.memory = Memory(data)
        self.reminders = Store(data / "reminders.json", [])
        self.skills: list[Skill] = []
        self.modes: dict[str, Mode] = {}
        # Sends a message later (reminders); the bot sets it once it's signed on.
        self.send: Callable[[str, str], Awaitable[None]] | None = None
        from . import skills  # registers everything
        skills.register(self)

    def on(self, *patterns: str):
        """Decorator: the handler answers messages that fully match any pattern."""
        def wrap(fn: Handler) -> Handler:
            for p in patterns:
                self.skills.append(Skill(re.compile(p, re.I), fn))
            return fn
        return wrap

    async def answer(self, login: str, text: str, room: bool = False) -> list[str]:
        """The replies to one message. `room`: said in a public chat, where there's no
        introduction, no games (they'd take over the room) and nothing long."""
        ctx = Ctx(self, login, text, room)
        if room:
            from .personality import room_reply
            r = room_reply(ctx)
            if r:
                return [r]
        mem = ctx.mem
        first = not mem.get("seen")
        ctx.last_seen = mem.get("seen", 0)  # before this message, for "welcome back"
        mem["seen"] = int(time.time())
        mem["count"] = mem.get("count", 0) + 1
        self.memory.store.save()

        reply = None
        mode = self.modes.get(login.lower())
        if mode and time.time() - mode.started > 30 * 60:
            ctx.end()  # an abandoned game
            mode = None
        if mode:
            if ctx.low in QUIT:
                ctx.end()
                if ctx.low not in ("menu", "help"):
                    return [random.choice(["OK, we can stop there.", "Done. What next?", "Game over, then!"])]
            else:
                reply = await mode.handler(ctx)
        if reply is None:
            for s in self.skills:
                m = s.pattern.fullmatch(ctx.text)
                if m:
                    reply = await s.handler(ctx, m)
                    if reply is not None:
                        break
        if reply is None:
            from .personality import fallback
            reply = fallback(ctx)
        out = [reply] if isinstance(reply, str) else list(reply)
        if room:
            mode = self.modes.get(login.lower())
            if mode and getattr(mode.handler, "game", False):
                ctx.end()
                return [f"Let's play that one over IM, so we don't flood the room! {self.pitch}"]
            ctx.end()  # follow-ups ("more", a menu number) belong in IM
            return out
        if first:
            out.insert(0, f"Hi {ctx.name}! I'm {self.bot_name}. Type \"help\" anytime to see what I can do.")
        return out
