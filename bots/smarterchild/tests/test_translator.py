"""The Translator, with a pretend LibreTranslate."""

import asyncio

from smarterchild.translator import Translator

PHRASES = {  # (text, target) -> (translation, detected source)
    ("¿dónde está la biblioteca?", "en"): ("Where is the library?", "es"),
    ("where is the library?", "es"): ("¿Dónde está la biblioteca?", "en"),
    ("where is the library?", "en"): ("where is the library?", "en"),
    ("bonjour tout le monde", "en"): ("hello everyone", "fr"),
    ("bonjour tout le monde", "de"): ("Hallo zusammen", "fr"),
    ("Note: the cat sleeps", "en"): ("Note: the cat sleeps", "en"),
}


class Fake:
    async def languages(self):
        return {"en": "English", "es": "Spanish", "fr": "French", "de": "German", "zh-Hans": "Chinese",
                "zh-Hant": "Chinese (traditional)"}

    async def translate(self, text, target):
        return PHRASES[(text, target)]


def ask(t, text, who="kim"):
    return asyncio.run(t.answer(who, text))[0]


def test_into_english_by_default(tmp_path):
    t = Translator(tmp_path, Fake())
    assert ask(t, "¿dónde está la biblioteca?") == "(Spanish → English) Where is the library?"
    assert ask(t, "english: ¿dónde está la biblioteca?") == "(Spanish → English) Where is the library?"


def test_a_language_up_front_picks_the_target(tmp_path):
    t = Translator(tmp_path, Fake())
    for prefix in ("spanish:", "Spanish :", "es:", "español:", "to spanish:", "translate to Spanish:"):
        assert ask(t, f"{prefix} where is the library?") == "(English → Spanish) ¿Dónde está la biblioteca?"


def test_already_english_gets_a_hint(tmp_path):
    t = Translator(tmp_path, Fake())
    assert "already English" in ask(t, "where is the library?")
    # a colon that isn't a language is just part of the message
    assert "already English" in ask(t, "Note: the cat sleeps")


def test_my_language_is_remembered(tmp_path):
    t = Translator(tmp_path, Fake())
    assert "German" in ask(t, "my language is Deutsch")
    assert ask(t, "bonjour tout le monde") == "(French → German) Hallo zusammen"
    assert ask(Translator(tmp_path, Fake()), "bonjour tout le monde") == "(French → German) Hallo zusammen"
    assert ask(t, "bonjour tout le monde", who="lee") == "(French → English) hello everyone"
    assert "don't know Klingon" in ask(t, "my language is Klingon")
    assert "Chinese" in ask(t, "languages")


def test_trouble_is_said_plainly(tmp_path):
    class Down(Fake):
        async def translate(self, text, target):
            raise OSError("connection refused")

    assert "stuck" in ask(Translator(tmp_path, Down()), "hola amigo")


def test_chat_commands(tmp_path):
    from smarterchild.translator import TranslatorRoom

    t = Translator(tmp_path, Fake())
    room = TranslatorRoom(t, "127.0.0.1", 1, "The Translator", 168)
    assert room.request("!translate ¿dónde está la biblioteca?") == "¿dónde está la biblioteca?"
    assert room.request("!tr hi") == "hi" and room.request("!weather Boston") is None
    assert room.request("translate this") is None
    rq = lambda s: asyncio.run(t.room_request(s))
    assert rq("to spanish where is the library?") == "spanish: where is the library?"
    assert rq("Spanish where is the library?") == "Spanish: where is the library?"
    assert rq("es: where is the library?") == "es: where is the library?"
    assert rq("hola amigos") == "hola amigos"
    room_ask = lambda s: asyncio.run(t.answer("hub:pat", asyncio.run(t.room_request(s)), room=True))[0]
    assert room_ask("¿dónde está la biblioteca?") == "(Spanish → English) Where is the library?"
    assert room_ask("spanish where is the library?") == "(English → Spanish) ¿Dónde está la biblioteca?"
    assert "!translate spanish:" in room_ask("where is the library?")
    assert room_ask("").startswith("!translate <text>")
    # someone's own language (set over IM) doesn't change what the room gets
    ask(t, "my language is german", who="hub:pat")
    assert room_ask("bonjour tout le monde") == "(French → English) hello everyone"


def test_in_a_rooms_chat(mock_server, tmp_path):
    from smarterchild.hotline import Client
    from smarterchild.translator import TranslatorRoom

    async def run():
        room = TranslatorRoom(Translator(tmp_path, Fake()), "127.0.0.1", mock_server, "The Translator", 168)
        task = asyncio.create_task(room.run())
        heard: asyncio.Queue = asyncio.Queue()

        async def h(kind, data):
            if kind == "chat":
                await heard.put(data["text"])
            elif kind == "private":
                await heard.put("PM " + data["text"])

        pat = Client("127.0.0.1", mock_server, "", "", nickname="Pat", classic=True, on_event=h)
        await pat.connect()
        for _ in range(100):
            if room.hub.client and not room.hub.client.closed.is_set():
                break
            await asyncio.sleep(0.05)
        await asyncio.sleep(0.3)
        pat.send_chat("bonjour tout le monde")   # not a command: left alone
        pat.send_chat("!translate bonjour tout le monde")
        while "hello everyone" not in (line := await asyncio.wait_for(heard.get(), 6)):
            assert "The Translator:" not in line
        assert "Pat: (French → English) hello everyone" in line
        pat.send_chat("Discord | Sam: !tr to spanish where is the library?")
        while "biblioteca" not in (line := await asyncio.wait_for(heard.get(), 6)):
            pass
        assert "Sam: (English → Spanish)" in line
        users = await pat.get_users()
        tid = next(uid for uid, n in users.items() if n == "The Translator")
        pat.send_private(tid, "¿dónde está la biblioteca?")
        while not (line := await asyncio.wait_for(heard.get(), 6)).startswith("PM "):
            pass
        assert line == "PM (Spanish → English) Where is the library?"
        await pat.close()
        await room.hub.client.close()
        task.cancel()

    asyncio.run(run())
