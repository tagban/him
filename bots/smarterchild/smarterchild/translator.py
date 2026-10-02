"""The Translator: a buddy that translates. Send it something in another language and it
answers in English (or your language); start a message with a language, like
"english: ¿dónde está la biblioteca?" or "spanish: where is the library?", to choose.

The translating is done by LibreTranslate (open source, no outside service), which runs
beside the bot (see bots/translator/docker-compose.yml).

Settings, from the environment (or a .env file):
  HOTLINE_HOST, HOTLINE_PORT          the server, default hotline.vespernet.net:5500
  TRANSLATOR_LOGIN, TRANSLATOR_PASSWORD  its account (required)
  TRANSLATOR_NAME     the name buddies see, default The Translator
  TRANSLATOR_STATUS   its status line, default: Say it in any language!
  TRANSLATOR_DATA     where each person's language is kept, default ./data
  TRANSLATE_URL       LibreTranslate, default http://libretranslate:5000
  TRANSLATE_KEY       its API key, if it wants one

And in a server's public chat, where it answers "!translate" (leave HUB_HOST empty to stay out):
  HUB_HOST, HUB_PORT      the server, e.g. the Hotline Central Hub; port default 5500. Several
                          servers: comma-separated, each host or host:port
  HUB_LOGIN, HUB_PASSWORD an account there, or empty to join as a guest
  HUB_ICON                its classic user icon, default 168
  HUB_TRIGGER             what starts a command, default !
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import random
import re
import time
import urllib.request
from pathlib import Path
from typing import Protocol

from .store import Store

log = logging.getLogger("translator")

MAX_CHARS = 1000

INTRO = ("Hi, I'm The Translator! Send me something in another language and I'll put it in English. "
         "To translate into a language, start with it: \"spanish: where is the library?\" Type \"help\" for more.")
HELP = ("Send me anything in another language and I'll translate it into English (or your language).\n"
        "  spanish: where is the library? - into Spanish (any language, or its code: es:)\n"
        "  english: ¿dónde está la biblioteca? - into English\n"
        "  my language is french - what I translate into when you don't say\n"
        "  languages - the ones I know")
ROOM_HELP = ("!translate <text> puts it in English. !translate spanish: <text> puts it in Spanish "
             "(any language). !translate languages lists them. Or IM me.")

# Other ways people name languages: their own names, and a few common ones.
ALIASES = {
    "español": "spanish", "espanol": "spanish", "castellano": "spanish", "français": "french",
    "francais": "french", "deutsch": "german", "italiano": "italian", "português": "portuguese",
    "portugues": "portuguese", "brazilian": "portuguese (brazil)", "русский": "russian",
    "日本語": "japanese", "中文": "chinese", "mandarin": "chinese", "chinese (simplified)": "chinese",
    "traditional chinese": "chinese (traditional)", "한국어": "korean", "العربية": "arabic",
    "हिन्दी": "hindi", "nederlands": "dutch", "polski": "polish", "türkçe": "turkish",
    "svenska": "swedish", "українська": "ukrainian", "tiếng việt": "vietnamese", "farsi": "persian",
    "tagalog": "filipino", "norwegian": "norwegian bokmål", "ελληνικά": "greek", "עברית": "hebrew",
}


class Backend(Protocol):
    async def languages(self) -> dict[str, str]: ...      # code -> name
    async def translate(self, text: str, target: str) -> tuple[str, str]: ...  # (translation, source code)


class Libre:
    """LibreTranslate's API."""

    def __init__(self, url: str, key: str = ""):
        self.url, self.key = url.rstrip("/"), key
        self._langs: dict[str, str] | None = None
        self._langs_at = 0.0

    def _call(self, path: str, payload: dict | None = None):
        data = None
        if payload is not None:
            if self.key:
                payload = {**payload, "api_key": self.key}
            data = json.dumps(payload).encode()
        req = urllib.request.Request(self.url + path, data=data,
                                     headers={"Content-Type": "application/json", "Accept": "application/json"})
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.loads(r.read())

    async def languages(self) -> dict[str, str]:
        if self._langs is None or time.time() - self._langs_at > 3600:
            got = await asyncio.to_thread(self._call, "/languages")
            self._langs = {l["code"]: l["name"] for l in got}
            self._langs_at = time.time()
        return self._langs

    async def translate(self, text: str, target: str) -> tuple[str, str]:
        r = await asyncio.to_thread(self._call, "/translate",
                                    {"q": text, "source": "auto", "target": target, "format": "text"})
        return r["translatedText"], (r.get("detectedLanguage") or {}).get("language", "")


PREFIX = re.compile(r"^\s*(?:(?:translate )?(?:(?:in)?to|in) )?([^\s:][^:]{0,30}?)\s*:\s*(.+)$", re.S | re.I)
SET_LANG = re.compile(r"^\s*(?:my language is|set my language to|i speak|translate (?:everything )?(?:in)?to)\s+([^:]+?)[.!]?\s*$", re.I)


class Translator:
    """The conversation: `answer(login, text) -> replies`."""

    def __init__(self, folder: Path, backend: Backend):
        self.prefs = Store(folder / "languages.json", {})
        self.backend = backend

    async def _code(self, name: str) -> str | None:
        """A language's code from its name, native name or code ("Spanish", "español", "es")."""
        langs = await self.backend.languages()
        n = ALIASES.get(name.strip().lower(), name.strip().lower())
        by_name = {v.lower(): k for k, v in langs.items()}
        if n in by_name:
            return by_name[n]
        if n in langs:
            return n
        return next((k for k in langs if k.lower() == n), None)

    async def _name(self, code: str) -> str:
        return (await self.backend.languages()).get(code, code)

    async def room_request(self, rest: str) -> str:
        """A chat command's words in the IM form: "to spanish hello" and "spanish hello" become
        "spanish: hello" (a language's full name only, so "!translate hola amigos" stays whole)."""
        r = rest.strip()
        m = re.match(r"(?i)(?:(?:in)?to|in)\s+(\S+)\s+(.+)$", r, re.S)
        if m and await self._code(m[1]):
            return f"{m[1]}: {m[2]}"
        first, _, more = r.partition(" ")
        if more and not first.endswith(":"):
            langs = await self.backend.languages()
            name = ALIASES.get(first.lower(), first.lower())
            if name in (v.lower() for v in langs.values()):
                return f"{first}: {more}"
        return r

    async def answer(self, login: str, text: str, room: bool = False) -> list[str]:
        """`room`: a chat command, which always means English unless it names a language."""
        t = text.strip()
        word = t.lower().rstrip(".!?")
        try:
            if word in ("help", "?", "commands", "menu", ""):
                return [ROOM_HELP if room else HELP]
            if word in ("hi", "hello", "hey", "yo", "sup", "hiya"):
                return [INTRO]
            if word in ("languages", "list", "what languages", "which languages"):
                names = sorted((await self.backend.languages()).values())
                return ["I know: " + ", ".join(names) + "."]
            if not room and (m := SET_LANG.match(t)):
                code = await self._code(m[1])
                if not code:
                    return [f"I don't know {m[1]} yet. Type \"languages\" to see the ones I do."]
                self.prefs.data[login.lower()] = code
                self.prefs.save()
                return [f"Okay! When you don't say, I'll translate into {await self._name(code)}."]

            mine = "en" if room else self.prefs.data.get(login.lower(), "en")
            target, chosen = mine, False
            if (m := PREFIX.match(t)) and (code := await self._code(m[1])):
                target, t, chosen = code, m[2].strip(), True
            if not t:
                return ["Translate what? Like: \"spanish: where is the library?\""]
            if len(t) > MAX_CHARS:
                return [f"That's a lot! Send me {MAX_CHARS} characters or fewer at a time."]

            out, source = await self.backend.translate(t, target)
            if source == target and not chosen:
                there = await self._name(target)
                if room:
                    return [f"That's already {there}! Try: !translate spanish: {t[:40]}"]
                return [f"That's already {there}! To translate it, start with a language, like "
                        f"\"spanish: {t[:40]}\"."]
            if source == target or out.strip().lower() == t.lower():
                return [out]
            frm = await self._name(source) if source else "?"
            return [f"({frm} → {await self._name(target)}) {out}"]
        except Exception:
            log.exception("translating for %s", login)
            return ["My dictionary's stuck. Try again in a minute?"]


class TranslatorRoom:
    """The Translator in a server's public chat: answers "!translate ..." (and "!tr ...") lines,
    including ones the Discord bridge relays, and private messages like IMs."""

    WORDS = ("translate", "tr", "translator")

    def __init__(self, translator: Translator, host: str, port: int, name: str, icon: int,
                 login: str = "", password: str = "", trigger: str = "!"):
        from .hub import Hub

        self.t = translator
        self.hub = Hub(None, host, port, name, icon, login, password, trigger, app_string="The Translator 0.1")
        self.hub.names = [name]
        self.hub.on_chat = self.on_chat
        self.hub.on_private = self.on_private

    def request(self, text: str) -> str | None:
        """The words after "!translate", or None for any other line."""
        trig = self.hub.trigger
        if not trig or not text.startswith(trig):
            return None
        word, _, rest = text[len(trig):].strip().partition(" ")
        return rest.strip() if word.lower() in self.WORDS else None

    async def on_chat(self, raw: str) -> None:
        from .hub import CHAT_LINE, RELAYED

        m = CHAT_LINE.match(raw.lstrip("\r\n"))
        if not m:
            return
        who, text = m[1].strip(), m[2].strip()
        if who.lower() == self.hub.name.lower():
            return
        if r := RELAYED.match(text):
            who, text = r[2].strip(), r[3].strip()
        rest = self.request(text)
        if rest is None:
            return
        if not self.hub._room_ok(f"hub:{who}"):
            log.info("chat %s %s: rate limited", self.hub.host, who)
            return
        replies = await self.t.answer(f"hub:{who}", await self.t.room_request(rest), room=True)
        await asyncio.sleep(random.uniform(0.5, 1.2))
        c = self.hub.client
        if c:
            c.send_chat((f"{who}: " + " ".join(replies).replace("\n", " "))[: c.max_message_bytes or 4000])
        log.info("chat %s %s: %r", self.hub.host, who, rest[:60])

    async def on_private(self, uid: int, who: str, text: str) -> None:
        rest = self.request(text.strip())
        q = await self.t.room_request(rest) if rest is not None else text
        for i, r in enumerate(await self.t.answer(f"hub:{who}", q)):
            if i:
                await asyncio.sleep(0.8)
            if self.hub.client:
                self.hub.client.send_private(uid, r.replace("\n", "\r"))

    async def run(self) -> None:
        await self.hub.run()


def main() -> None:
    from .bot import Bot, load_env

    logging.basicConfig(level=os.environ.get("LOG_LEVEL", "INFO"),
                        format="%(asctime)s %(levelname)s %(name)s: %(message)s")
    load_env(Path(".env"))
    data = Path(os.environ.get("TRANSLATOR_DATA", "data"))
    login, password = os.environ.get("TRANSLATOR_LOGIN"), os.environ.get("TRANSLATOR_PASSWORD")
    if not login or not password:
        raise SystemExit("Set TRANSLATOR_LOGIN and TRANSLATOR_PASSWORD (in the environment or a .env file).")
    backend = Libre(os.environ.get("TRANSLATE_URL", "http://libretranslate:5000"), os.environ.get("TRANSLATE_KEY", ""))
    bot = Bot(os.environ.get("HOTLINE_HOST", "hotline.vespernet.net"), int(os.environ.get("HOTLINE_PORT", "5500")),
              login, password, os.environ.get("TRANSLATOR_NAME", "The Translator"),
              os.environ.get("TRANSLATOR_STATUS", "Say it in any language!"), data,
              brain=Translator(data, backend), app_string="The Translator 0.1", welcome=INTRO,
              bang_commands=False)
    rooms = []
    for entry in filter(None, (h.strip() for h in os.environ.get("HUB_HOST", "").split(","))):
        host, _, port = entry.rpartition(":") if entry.count(":") == 1 else (entry, "", "")
        rooms.append(TranslatorRoom(bot.brain, host, int(port or os.environ.get("HUB_PORT", "5500")), bot.name,
                                    int(os.environ.get("HUB_ICON", "168")), os.environ.get("HUB_LOGIN", ""),
                                    os.environ.get("HUB_PASSWORD", ""), os.environ.get("HUB_TRIGGER", "!")))

    async def all_of_it():
        await asyncio.gather(bot.run(), *(r.run() for r in rooms))

    try:
        asyncio.run(all_of_it())
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
