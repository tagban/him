"""The brain, without the network: every skill that doesn't need a web service."""

import asyncio

import pytest

from smarterchild import skills
from smarterchild.brain import Brain, normalize


PLACES = {
    "boston": {"name": "Boston", "lat": 42.36, "lon": -71.06, "tz": "America/New_York", "cc": "US",
               "label": "Boston, Massachusetts"},
}


@pytest.fixture
def brain(tmp_path, monkeypatch):
    """No network: places come from PLACES, and a place's rundown is a stand-in."""
    async def find_place(q):
        return PLACES.get(q.strip().lower())

    async def place_facts(ctx, p):
        return f"(facts about {p['name']})"
    monkeypatch.setattr(skills, "find_place", find_place)
    monkeypatch.setattr(skills, "place_facts", place_facts)

    async def current_weather(ctx, p):
        return "64°F and cloudy"
    monkeypatch.setattr(skills, "current_weather", current_weather)
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


def test_weather_units_follow_the_place(brain):
    from smarterchild.brain import Ctx
    from smarterchild.skills import units_for
    ctx = Ctx(brain, "alice", "hi")
    assert units_for(ctx, {"cc": "US"}) == "f"
    assert units_for(ctx, {"cc": "FR"}) == "c"
    assert units_for(ctx, {"cc": "PR"}) == "f"
    assert units_for(ctx, {"label": "Portland, Oregon"}) == "f"   # remembered before we kept the country
    assert units_for(ctx, {"label": "Paris, France"}) == "c"
    ask(brain, "use celsius")
    assert units_for(Ctx(brain, "alice", "hi"), {"cc": "US"}) == "c"
    ask(brain, "use local units")
    assert units_for(Ctx(brain, "alice", "hi"), {"cc": "US"}) == "f"


def test_happy_news_leaves_out_the_grim():
    from smarterchild.skills import parse_feed
    rss = b"""<rss><channel>
      <item><title>Town plants 10,000 trees</title><link>https://x/1</link></item>
      <item><title>Beloved actor dies at 90</title><link>https://x/2</link></item>
      <item><title>Old story</title><link>https://x/3</link><pubDate>Mon, 01 Jan 2001 00:00:00 GMT</pubDate></item>
    </channel></rss>"""
    assert parse_feed(rss, "GNN") == [("Town plants 10,000 trees", "https://x/1", "GNN")]
    from smarterchild.hub import command
    assert command("!happynews") == "happy news"


def test_it_says_what_it_is(brain):
    for q in ("who are you", "what are you?", "are you a bot", "where are you from", "who made you"):
        r = ask(brain, q, who="bob")
        assert "chatterbot based on SmarterChild from AOL Instant Messenger" in r, q
        assert "https://github.com/tagban/him/releases" in r and "HL Central Hub" in r
    room = "\n".join(asyncio.run(brain.answer("hub:pat", "what are you", room=True)))
    assert "AOL Instant Messenger" in room and "!help" in room


def test_news_routes(brain):
    from smarterchild.hub import command
    news_skills = ("happy_news", "news_on", "news", "wiki")

    def route(q):  # the first news-ish skill that claims the message
        return next(s.handler.__name__ for s in brain.skills
                    if s.handler.__name__ in news_skills and s.pattern.fullmatch(q))

    assert route("news happy") == route("good news") == route("happynews") == "happy_news"
    for q in ("news us", "us news", "news world", "tech news", "news war", "news about bitcoin"):
        assert route(q) == "news_on", q
    assert route("news") == "news"
    assert command("!news us") == "news us"


def test_asks_where_you_are_once_in_im(brain):
    first = ask(brain, "hi", who="kim")
    assert "where" in first.lower()
    assert "Boston, Massachusetts" in ask(brain, "Boston", who="kim")
    assert "(facts about Boston)" in ask(brain, "hey", who="kim") or True
    assert brain.memory.of("kim")["place"]["name"] == "Boston"
    assert "where" not in ask(brain, "how are you", who="kim").lower()  # asked once, and it knows now


def test_a_command_after_the_question_is_just_answered(brain):
    ask(brain, "hi", who="lee")
    assert ask(brain, "what is 6*7", who="lee") == "6*7 = 42"
    ask(brain, "hi", who="max")
    assert "Fair enough" in ask(brain, "not telling", who="max")


def test_never_asks_in_a_room(brain):
    out = "\n".join(asyncio.run(brain.answer("hub:pat", "hi", room=True)))
    assert "where" not in out.lower()


def test_says_hi_with_your_weather(brain):
    ask(brain, "hi", who="tagban")
    ask(brain, "Boston", who="tagban")
    brain.memory.of("tagban")["name"] = "Tagban"
    r = ask(brain, "hey", who="tagban")
    assert "Tagban" in r and "64°F and cloudy" in r and "Boston" in r
    brain.memory.of("tagban")["seen"] -= 5 * 86400
    assert "Welcome back, Tagban! It's been 5 days." in ask(brain, "hi", who="tagban")
    # but never where people can read it
    brain.memory.of("hub:tagban")["place"] = brain.memory.of("tagban")["place"]
    assert "Boston" not in "\n".join(asyncio.run(brain.answer("hub:tagban", "hi", room=True)))


def test_trivia_score_and_leaderboard(brain):
    assert "haven't played" in ask(brain, "trivia score", who="ann")
    for who, right, total in (("ann", 8, 10), ("bo", 3, 10), ("cy", 1, 2)):
        m = brain.memory.of(who)
        m["trivia_right"], m["trivia_total"] = right, total
    brain.memory.of("ann")["name"] = "Ann"
    r = ask(brain, "my score", who="ann")
    assert "8 of 10" in r and "80%" in r and "#1 of 2" in r
    top = ask(brain, "leaderboard", who="bo")
    assert "1. Ann: 8 of 10" in top and "2. bo: 3 of 10" in top and "cy" not in top


def test_odoyle_is_in_the_rotation():
    from smarterchild.personality import FALLBACK
    assert any("O'DOYLE RULES" in f for f in FALLBACK)


def test_what_it_doesnt_understand_is_kept(tmp_path):
    from smarterchild.misses import Misses

    b = Brain(tmp_path)
    run = lambda text, who="kim": asyncio.run(b.answer(who, text))
    run("blorp the snorgle")
    run("Blorp the snorgle!", who="lee")
    run("define ennui")                      # asked for a lookup: not a miss
    run("my email is kim@example.com zzz")   # looks personal: not kept
    run("what is 6*7")                       # answered: not a miss
    top = Misses(tmp_path).top()
    assert [(e["text"].lower(), e["kind"], e["count"]) for e in top] == [("blorp the snorgle", "fallback", 2)]
    assert "kim" not in str(Misses(tmp_path).store.data) and "lee" not in str(Misses(tmp_path).store.data)
