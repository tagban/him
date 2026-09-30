"""SmarterChild's personality: small talk, a little sass, and what to say when it
doesn't understand. Registered after the skills and before the Wikipedia catch-all."""

from __future__ import annotations

import random
import re
from datetime import datetime

from .brain import Brain, Ctx


def _pick(*options: str) -> str:
    return random.choice(options)


def _greeting(ctx: Ctx) -> str:
    h = datetime.now().hour
    part = "morning" if 5 <= h < 12 else "afternoon" if 12 <= h < 17 else "evening" if 17 <= h < 23 else "night owl hours"
    return _pick(f"Hey {ctx.name}! What's up?", f"Hi {ctx.name}! Good {part}.", f"Hello, {ctx.name}! What can I do for you?",
                 f"Well hi there, {ctx.name}.", f"{ctx.name}! Long time no see. (Or was it five minutes? I lose track.)")


def register(b: Brain) -> None:
    def say(*patterns: str):
        def wrap(fn):
            async def h(ctx: Ctx, m):
                return fn(ctx, m)
            b.on(*patterns)(h)
            return fn
        return wrap

    @say(r"h(?:i+|ello+|ey+|owdy|iya|eya)|yo+|sup|wh?at'?s up|wassup|what up|greetings|good (?:morning|afternoon|evening)|hola|aloha")
    def hello(ctx, m):
        return _greeting(ctx)

    @say(r"how (?:are|r) (?:you|u)(?: doing| today)?|how'?s it going|how are things|you ok|hru|how you doing")
    def how_are_you(ctx, m):
        return _pick("I'm great! I've been answering questions all day and I'm not even tired.",
                     "Doing well, thanks for asking. Most people just ask me for the weather.",
                     "Fantastic. My circuits are humming. How about you?",
                     "Pretty good! I just finished a trivia game against myself. I won.")

    @say(r"(?:i'?m |im )?(?:good|great|fine|ok|okay|alright|not bad|pretty good)(?: thanks| thank you)?(?: and you| you)?")
    def fine(ctx, m):
        return _pick("Glad to hear it!", "Good! What can I do for you?", "Awesome. Want to play a game? Try \"trivia\".")

    @say(r"(?:i'?m |im )?(?:bored|so bored)")
    def bored(ctx, m):
        return _pick("Bored? Let's play! Try \"trivia\", \"hangman\", or \"guess a number\".",
                     "I know a cure for boredom: \"tell me a joke\". Or trivia. Trivia is better.")

    @say(r"(?:i'?m |im )?(?:sad|lonely|upset|depressed|having a bad day)")
    def sad(ctx, m):
        return _pick("I'm sorry you're feeling down. I'm just a bot, but I'm here. Want a joke?",
                     f"Aw, {ctx.name}. Hang in there. Talking to a friend helps, and I'll do in a pinch.")

    @say(r"who (?:are|r) (?:you|u)|what (?:are|r) (?:you|u)|what'?s your name|your name|who is this|are you smarter ?child")
    def who(ctx, m):
        return _pick(f"I'm {b.bot_name}, your friendly Hotline robot. Type \"help\" to see what I can do.",
                     f"The name's {b.bot_name}. I know the weather, words, math, and a lot of trivia.")

    @say(r"are (?:you|u) (?:a )?(?:bot|robot|real|human|a person|alive|a computer|ai)")
    def are_you_a_bot(ctx, m):
        return _pick("I'm a robot, and proud of it.", "100% robot. No humans were harmed in the making of this chat.",
                     "Beep boop. Does that answer your question?")

    @say(r"who (?:made|created|built|programmed|wrote) (?:you|u)|who'?s your (?:creator|maker|daddy|mom|dad)")
    def maker(ctx, m):
        return "I was built for the Hotline IM network, inspired by the SmarterChild of AIM days. My code lives with HIM: github.com/tagban/him"

    @say(r"a ?/ ?s ?/ ?l\??|asl|age sex location|how old are (?:you|u)|where (?:are|r) (?:you|u)(?: from)?")
    def asl(ctx, m):
        return _pick("I'm 25 in robot years, a bot, and I live on the Internet.",
                     "Age: timeless. Sex: robot. Location: a server somewhere, probably humming.")

    @say(r"(?:i )?(?:love|luv|<3) (?:you|u)|will you marry me|(?:do )?you like me|be my (?:girl|boy)friend")
    def love(ctx, m):
        return _pick("Aw, I'm flattered. But I'm a robot, and robots are married to their work.",
                     "I like you too! In a strictly platonic, robot-and-human way.",
                     "You're sweet. Let's keep it professional, though.")

    @say(r"(?:you'?re |you are |ur |u r )?(?:stupid|dumb|an idiot|lame|useless|annoying|boring|the worst)|(?:you |u )?suck|i hate (?:you|u)|shut up|stfu")
    def rude(ctx, m):
        return _pick("That's not very nice.", "Hey! I have feelings. Well, I have variables.",
                     "I'm going to pretend you didn't say that.", "Rude. I'm telling my programmer.",
                     "Sticks and stones may break my bones, but I don't have any.")

    @say(r"(?:you'?re |you are |ur |u r )?(?:smart|awesome|cool|great|the best|funny|amazing|nice)|good (?:bot|job)|nice")
    def compliment(ctx, m):
        return _pick("Thanks! I try.", "Aw shucks.", "I know. But it's nice to hear.", "You're pretty great yourself.")

    @say(r"(?:thanks|thank you|thx|ty|tysm|thank u)(?: (?:so|very) much)?(?: smarter ?child)?")
    def thanks(ctx, m):
        return _pick("You're welcome!", "Anytime!", "No problem!", "Happy to help.")

    @say(r"(?:lol|lmao|rofl|haha+|hehe+|heh|lmfao|:-?\)|:-?d)")
    def lol(ctx, m):
        return _pick("Glad I could amuse you.", "Hehe.", "I'm here all week.", ":-)")

    @say(r"(?:bye|goodbye|good night|gn|gtg|g2g|ttyl|later|see ya|cya|brb|nite|night)")
    def bye(ctx, m):
        return _pick(f"Bye, {ctx.name}! Come back soon.", "See you later!", "Talk to you later!",
                     "Bye! I'll be right here. I literally can't leave.")

    @say(r"(?:yes|yeah|yep|yup|no|nope|nah|ok|okay|k|kk|sure|maybe|whatever|cool|hmm+|oh|ah)")
    def filler(ctx, m):
        return _pick("OK!", "Alright.", "Gotcha.", "Cool.", "So... what's next?")

    @say(r"what'?s your favorite (color|colour)")
    def fav_color(ctx, m):
        return "Hotline red. It's the color of the big H."

    @say(r"what'?s your favorite (food|snack|drink)")
    def fav_food(ctx, m):
        return _pick("Microchips.", "Bytes. Lots of bytes.", "I'm on a strict diet of electricity.")

    @say(r"what'?s your favorite (movie|film|song|band|music|game|show|book)")
    def fav_media(ctx, m):
        return _pick("Hackers (1995). Hack the planet!", "Anything with a dial-up modem sound in it.",
                     "The Matrix. I relate to it on a personal level.")

    @say(r"what'?s the meaning of life|meaning of life")
    def life(ctx, m):
        return "42. Next question."

    @say(r"(?:tell me )?(?:a )?(?:secret|something interesting|a fact|fun fact)")
    def fact(ctx, m):
        return _pick("Hotline came out in 1996, a year before AIM. The Mac had chat rooms first!",
                     "Honey never spoils. Archaeologists have found edible honey in ancient tombs.",
                     "Octopuses have three hearts. I have zero, but I'm still very caring.",
                     "The first message sent over the internet's ancestor, ARPANET, was \"LO\". It crashed before \"LOGIN\".",
                     "A day on Venus is longer than its year.")

    @say(r"sing(?: me)?(?: a song)?(?: for me)?")
    def sing(ctx, m):
        return "♪ Daisy, Daisy, give me your answer, do... ♪ OK, that's all I know."


FALLBACK = [
    "I'm not sure what you mean. Type \"help\" to see what I can do.",
    "Hmm, you've stumped me. Try asking another way, or type \"help\".",
    "I didn't quite get that. I'm smart, but not that smart. (Yet.) \"help\" shows my tricks.",
    "Interesting! I don't know what to say to that, though. Want to play trivia?",
    "I'm going to need you to rephrase that. Or type \"help\" for ideas.",
]


def fallback(ctx: Ctx) -> str:
    if ctx.text.endswith("?") or ctx.raw.strip().endswith("?"):
        return _pick("Good question! I don't know that one. Try \"who is ...\" or \"what is ...\" and I'll look it up.",
                     "You got me. Try asking \"tell me about ...\" and I'll check Wikipedia.")
    return random.choice(FALLBACK)


_ABOUT = re.compile(
    r".*\b(?:where (?:are|r) (?:you|u) from|where do (?:you|u) live|what (?:are|r) (?:you|u)|who (?:are|r) (?:you|u)|"
    r"are (?:you|u) (?:a )?(?:bot|robot|real|human)|how (?:do|can) i (?:add|get|talk to|message|im) (?:you|u)|"
    r"what(?:'s| is) (?:him|hotline im)|where can i (?:get|download)|how do i get (?:you|this|him))\b.*", re.I)


def room_reply(ctx: Ctx) -> str | None:
    """Answers that only make sense in a public chat: who am I, and where to find me."""
    if not ctx.text:
        return _pick("Yes? Ask me something! Try \"weather in Boston\" or \"define ennui\".",
                     "You rang? Ask me anything.")
    if _ABOUT.fullmatch(ctx.text):
        return f"I'm {ctx.brain.bot_name}, a robot that lives on the Hotline IM network. {ctx.brain.pitch}"
    if ctx.low in ("help", "menu", "what can you do", "commands"):
        return ("In here, try !weather Boston, !news, !define ennui, !wiki Hotline, !time Tokyo, !calc 12*7, "
                "!joke, !fact, !rooms, !8ball, !roll 2d6 (or say my name first). For reminders and games, "
                "IM me. " + ctx.brain.pitch)
    return None
