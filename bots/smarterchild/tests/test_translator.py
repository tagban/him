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
