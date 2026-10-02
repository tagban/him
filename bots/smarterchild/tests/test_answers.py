"""SmarterChild's prepared answers (answers.py)."""

import asyncio

from smarterchild.brain import Brain


def ask(brain, text, who="tester"):
    return asyncio.run(brain.answer(who, text))[-1]


def test_favorites_and_common_questions(tmp_path):
    b = Brain(tmp_path)
    color = ask(b, "What's your favorite color?")
    assert any(w in color for w in ("White", "Hotline red", "Clear"))
    assert ask(b, "whats ur fav dinosaur")   # one it has no answer for still gets a reply
    assert "John" in ask(b, "can I talk to a human")   # not taken for an 8-ball question
    assert ask(b, "help me with my homework").startswith(("Nice try", "As if"))


def test_someone_in_danger_gets_a_serious_answer(tmp_path):
    b = Brain(tmp_path)
    for text in ("I want to die", "i'm thinking about killing myself", "I just want to end it all"):
        r = ask(b, text)
        assert "988" in r and "findahelpline.com" in r


def test_the_skills_still_answer(tmp_path):
    b = Brain(tmp_path)
    assert "Weezer" in ask(b, "remember my favorite band is Weezer")
    assert "Weezer" in ask(b, "what do you know about me")
