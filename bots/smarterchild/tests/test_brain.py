"""The brain, without the network: every skill that doesn't need a web service."""

import asyncio

import pytest

from smarterchild import skills
from smarterchild.brain import Brain, normalize


@pytest.fixture
def brain(tmp_path):
    return Brain(tmp_path)


def ask(brain, text, who="alice"):
    return "\n".join(asyncio.run(brain.answer(who, text)))


def test_first_contact_says_hello(brain):
    assert "I'm SmarterChild" in ask(brain, "hi")
    assert "I'm SmarterChild" not in ask(brain, "hi")


def test_normalize_drops_politeness():
    assert normalize("Hey SmarterChild, can you please tell me a joke??") == "tell me a joke"


def test_menu_and_numbers(brain):
    ask(brain, "hi")
    assert "1. Weather" in ask(brain, "help")
    assert ask(brain, "3").startswith("Dictionary")
    assert ask(brain, "4").startswith("Math")


def test_math(brain):
    ask(brain, "hi")
    assert ask(brain, "what is 12*(3+4)") == "12*(3+4) = 84"
    assert ask(brain, "2^10") == "2^10 = 1,024"
    assert ask(brain, "what's 15% of 80") == "15% of 80 is 12."
    assert "Nice try" in ask(brain, "5/0") or "zero" in ask(brain, "5/0").lower()
    assert "= " not in ask(brain, "__import__('os')")


def test_conversions(brain):
    ask(brain, "hi")
    assert ask(brain, "10 km in miles") == "10 km is 6.21371 miles."
    assert ask(brain, "convert 212 f to c") == "212 f is 100 c."
    assert "16" in ask(brain, "how many ounces in a pound")


def test_memory(brain):
    ask(brain, "hi")
    assert "Sam" in ask(brain, "my name is Sam")
    assert "Sam" in ask(brain, "what's my name")
    ask(brain, "remember my favorite band is Weezer")
    assert "Weezer" in ask(brain, "what's my favorite band")
    assert "Weezer" in ask(brain, "what do you know about me")
    assert "yes/no" in ask(brain, "forget me")
    ask(brain, "yes")
    assert "Sam" not in ask(brain, "what's my name")


def test_games(brain):
    ask(brain, "hi")
    board = ask(brain, "hangman")
    assert "Guess a letter" in board
    word = brain.modes["alice"].state["word"]
    for ch in dict.fromkeys(word):
        out = ask(brain, ch)
    assert "You got it" in out or "win" in out
    assert "1 to 100" in ask(brain, "guess a number")
    n = brain.modes["alice"].state["n"]
    assert ask(brain, str(n)).startswith(str(n))
    assert ask(brain, "roll 2d6").startswith("🎲")
    assert ask(brain, "flip a coin") in ("Heads!", "Tails!", "It landed on its edge. Seriously.")


def test_reminders(brain):
    ask(brain, "hi")
    assert "in 10 minutes" in ask(brain, "remind me in 10 minutes to stretch")
    assert "stretch" in ask(brain, "my reminders")
    sent = []

    async def send(to, text):
        sent.append((to, text))

    brain.send = send
    brain.reminders.data[0]["due"] = 0
    asyncio.run(skills.run_reminders(brain))
    assert sent == [("alice", "Reminder: stretch")]
    assert "don't have any" in ask(brain, "my reminders")


def test_personality_and_fallback(brain):
    ask(brain, "hi")
    assert ask(brain, "you suck")
    assert ask(brain, "asl")
    for _ in range(10):
        r = ask(brain, "flarbgloop zibbity").lower()
        assert "help" in r or "trivia" in r


def test_in_a_room_it_points_to_him_and_keeps_games_for_im(brain):
    r = ask(brain, "where are you from?", who="hub:pat")
    assert "github.com/tagban/him" in "\n".join(asyncio.run(brain.answer("hub:pat", "where are you from?", room=True)))
    assert "over IM" in "\n".join(asyncio.run(brain.answer("hub:pat", "hangman", room=True)))
    assert "hub:pat" not in brain.modes
    out = asyncio.run(brain.answer("hub:sam", "what is 6*7", room=True))
    assert out == ["6*7 = 42"]  # no introduction in a room


def test_hub_commands_and_triggers(tmp_path):
    from smarterchild.hub import Hub, command

    assert command("!weather Boston") == "weather Boston"
    assert command("!w Boston") == "weather Boston"
    assert command("!news") == "news"
    assert command("!time") == "what time is it"
    assert command("!time Tokyo") == "time in Tokyo"
    assert command("!calc 12*7") == "12*7"
    assert command("!who is Ada Lovelace") == "who is Ada Lovelace"
    assert command("!") is None and command("hello") is None

    class FakeClient:
        max_message_bytes = 4096
        def __init__(self):
            self.said = []
        def send_chat(self, text, emote=False):
            self.said.append(text)

    hub = Hub(Brain(tmp_path), "x", 5500, "SmarterChild", 168)
    hub.client = FakeClient()

    async def hear(line):
        # the Hub sends a Chat ID with public chat, as the protocol allows
        await hub.on_event("chat", {"text": "\r" + line, "chat_id": 0})
        await asyncio.sleep(0)
        for t in list(asyncio.all_tasks()):
            if t is not asyncio.current_task():
                await t

    async def run():
        await hear("          Pat:  !calc 6*7")
        await hear("          Pat:  nice weather we're having")
        await hear("          Sam:  smarterchild, what's 2+2")
        await hear("  SmarterChild:  Pat: talking to myself")
        await hear("          Pat:  !help")

    asyncio.run(run())
    said = hub.client.said
    assert said[0] == "Pat: 6*7 = 42"
    assert said[1] == "Sam: 2+2 = 4"
    assert "!weather" in said[2]
    assert len(said) == 3
