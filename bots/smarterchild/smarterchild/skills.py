"""Everything SmarterChild can do. Each skill is a function with the patterns that
reach it; the first pattern that fully matches a (normalized) message wins, so
specific skills are registered before general ones."""

from __future__ import annotations

import ast
import asyncio
import html
import math
import operator
import random
import re
import struct
import time
from datetime import datetime, timedelta
from zoneinfo import ZoneInfo

from . import web
from .brain import Brain, Ctx

# ---------- the menu ----------

MENU = [
    ("Weather", 'weather in Boston, forecast, "I live in Chicago"'),
    ("Look it up", 'who is Ada Lovelace, tell me about Hotline, "wiki MacOS 9"'),
    ("Dictionary", 'define serendipity, what does "ennui" mean'),
    ("Math and conversions", "what is 15% of 80, 12*(3+4), 10 km in miles, 72 f in c"),
    ("Time around the world", "what time is it in Tokyo, time in London"),
    ("Reminders", "remind me in 20 minutes to check the oven, remind me at 5pm to call Mom, my reminders"),
    ("Games", "trivia, hangman, guess a number, rock paper scissors, 8 ball will I win, roll 2d6, flip a coin"),
    ("Hotline chat rooms", "chat rooms, who's on Hotline"),
    ("On this day", "on this day, today in history"),
    ("Jokes and fortunes", "tell me a joke, fortune"),
    ("About you", 'my name is Sam, what\'s my name, remember my favorite band is Weezer, what do you know about me'),
]


def menu_text() -> str:
    lines = ["Here's what I can do. Type a number for examples, or just ask me:"]
    lines += [f"{i}. {name}" for i, (name, _) in enumerate(MENU, 1)]
    return "\n".join(lines)


def menu_item(n: int) -> str | None:
    if 1 <= n <= len(MENU):
        name, ex = MENU[n - 1]
        return f"{name}. Try: {ex}."
    return None


async def _menu_mode(ctx: Ctx):
    if ctx.low.isdigit():
        return menu_item(int(ctx.low)) or f"Pick a number from 1 to {len(MENU)}."
    ctx.end()
    return None  # not a number: treat it as a new question


def register(b: Brain) -> None:
    @b.on(r"help|menu|commands|options|what (?:can|do) you do|what can i (?:ask|say|do)|\?+|h")
    async def help_(ctx: Ctx, m):
        ctx.start(_menu_mode)
        return menu_text()

    @b.on(r"(\d{1,2})")
    async def number(ctx: Ctx, m):
        return menu_item(int(m[1]))

    _memory(b)
    _reminders(b)
    _weather(b)
    _time(b)
    _convert(b)
    _calc(b)
    _define(b)
    _games(b)
    _jokes(b)
    _rooms(b)
    _on_this_day(b)
    from . import personality
    personality.register(b)
    _wiki(b)  # last: "what is X" is its catch-all


# ---------- places (weather, time, reminders) ----------

STATES = dict(
    AL="Alabama", AK="Alaska", AZ="Arizona", AR="Arkansas", CA="California", CO="Colorado", CT="Connecticut",
    DE="Delaware", FL="Florida", GA="Georgia", HI="Hawaii", ID="Idaho", IL="Illinois", IN="Indiana", IA="Iowa",
    KS="Kansas", KY="Kentucky", LA="Louisiana", ME="Maine", MD="Maryland", MA="Massachusetts", MI="Michigan",
    MN="Minnesota", MS="Mississippi", MO="Missouri", MT="Montana", NE="Nebraska", NV="Nevada",
    NH="New Hampshire", NJ="New Jersey", NM="New Mexico", NY="New York", NC="North Carolina",
    ND="North Dakota", OH="Ohio", OK="Oklahoma", OR="Oregon", PA="Pennsylvania", RI="Rhode Island",
    SC="South Carolina", SD="South Dakota", TN="Tennessee", TX="Texas", UT="Utah", VT="Vermont",
    VA="Virginia", WA="Washington", WV="West Virginia", WI="Wisconsin", WY="Wyoming", DC="District of Columbia",
)


async def find_place(q: str) -> dict | None:
    """{name, lat, lon, tz, label} for a city ("Boston", "Portland, OR", "Paris, France")."""
    q = q.strip().strip(",")
    city, _, region = (x.strip() for x in q.partition(","))
    region_full = STATES.get(region.upper(), region)
    try:
        data = await web.get_json("https://geocoding-api.open-meteo.com/v1/search",
                                  {"name": city, "count": 10, "language": "en", "format": "json"}, ttl=86400)
    except Exception:
        return None
    results = data.get("results") or []
    if region_full:
        rl = region_full.lower()
        results = [r for r in results if rl in (r.get("admin1", "") + " " + r.get("country", "")).lower()] or results
    if not results:
        return None
    r = results[0]
    where = r.get("admin1") if r.get("country_code") == "US" else r.get("country")
    return {"name": r["name"], "lat": r["latitude"], "lon": r["longitude"], "tz": r.get("timezone", "UTC"),
            "label": f"{r['name']}, {where}" if where else r["name"]}


# ---------- about you ----------

def _memory(b: Brain) -> None:
    @b.on(r"(?:my name is|call me|i'?m called|you can call me)\s+([a-z][\w' .-]{0,30})")
    async def set_name(ctx: Ctx, m):
        name = m[1].strip().title() if m[1].islower() else m[1].strip()
        ctx.remember("name", name)
        return random.choice([f"Nice to meet you, {name}!", f"OK, I'll call you {name}.", f"{name}. Got it. Cool name."])

    @b.on(r"what'?s my name|what is my name|who am i|do you know my name")
    async def get_name(ctx: Ctx, m):
        if ctx.mem.get("name"):
            return f"You're {ctx.mem['name']}! How could I forget?"
        return f"Your screen name is {ctx.login}. Tell me what to call you: \"my name is ...\""

    @b.on(r"(?:i live in|i'?m in|i'?m from|my (?:location|city|town|zip(?: code)?) is|set (?:my )?location(?: to)?)\s+(.+)")
    async def set_place(ctx: Ctx, m):
        p = await find_place(m[1])
        if not p:
            return f"Hmm, I couldn't find \"{m[1]}\" on the map. Try a city, like \"Portland, OR\"."
        ctx.remember("place", p)
        return f"Got it, {p['label']}. Now \"weather\" and \"time\" will use that."

    @b.on(r"where do i live|what'?s my (?:location|city)")
    async def get_place(ctx: Ctx, m):
        p = ctx.mem.get("place")
        return f"You told me {p['label']}." if p else "You haven't told me. Say \"I live in (city)\"."

    @b.on(r"use (celsius|centigrade|metric|fahrenheit|imperial)")
    async def units(ctx: Ctx, m):
        c = m[1].lower() in ("celsius", "centigrade", "metric")
        ctx.remember("units", "c" if c else "f")
        return f"OK, {'Celsius' if c else 'Fahrenheit'} it is."

    @b.on(r"remember(?: that)? my ([\w ]{1,40}?) (?:is|are) (.{1,200})")
    async def remember_fact(ctx: Ctx, m):
        facts = ctx.mem["facts"]
        facts[m[1].lower()] = m[2]
        ctx.brain.memory.store.save()
        return random.choice([f"I'll remember that your {m[1]} is {m[2]}.", f"Filed away: {m[1]} = {m[2]}."])

    @b.on(r"what(?:'s| is| are) my ([\w ]{1,40})")
    async def recall_fact(ctx: Ctx, m):
        v = ctx.mem["facts"].get(m[1].lower())
        if v:
            return f"Your {m[1]} is {v}."
        return None  # maybe it's a question for another skill

    @b.on(r"what do you (?:know|remember) about me|what have i told you")
    async def about_me(ctx: Ctx, m):
        mem = ctx.mem
        bits = []
        if mem.get("name"):
            bits.append(f"your name is {mem['name']}")
        if mem.get("place"):
            bits.append(f"you're in {mem['place']['label']}")
        bits += [f"your {k} is {v}" for k, v in mem["facts"].items()]
        if mem.get("trivia_total"):
            bits.append(f"you've gotten {mem.get('trivia_right', 0)} of {mem['trivia_total']} trivia questions right")
        if not bits:
            return "Not much yet! Tell me things, like \"my name is ...\" or \"remember my favorite color is blue\"."
        return "I know that " + ", ".join(bits[:-1]) + (" and " if len(bits) > 1 else "") + bits[-1] + "."

    async def forget_mode(ctx: Ctx):
        ctx.end()
        if ctx.low in ("yes", "y", "yeah", "yep", "sure", "do it"):
            ctx.brain.memory.forget(ctx.login)
            return "Done. Who are you again? ;)"
        return "OK, I'll keep remembering."

    @b.on(r"forget (?:about )?(?:me|everything)(?: about me)?")
    async def forget(ctx: Ctx, m):
        ctx.start(forget_mode)
        return "Forget everything you've told me? (yes/no)"


# ---------- reminders ----------

UNITS_S = {"s": 1, "sec": 1, "secs": 1, "second": 1, "seconds": 1, "m": 60, "min": 60, "mins": 60, "minute": 60,
           "minutes": 60, "h": 3600, "hr": 3600, "hrs": 3600, "hour": 3600, "hours": 3600, "d": 86400,
           "day": 86400, "days": 86400, "week": 604800, "weeks": 604800}


def _user_tz(ctx: Ctx):
    p = ctx.mem.get("place")
    try:
        return ZoneInfo(p["tz"]) if p else None
    except Exception:
        return None


def _reminders(b: Brain) -> None:
    def add(ctx: Ctx, due: float, what: str) -> None:
        b.reminders.data.append({"login": ctx.login, "due": due, "text": what, "made": time.time()})
        b.reminders.save()

    def when(due: float, tz) -> str:
        d = datetime.fromtimestamp(due, tz)
        return d.strftime("%-I:%M %p") + (" tomorrow" if d.date() > datetime.now(tz).date() else "") + \
            ("" if tz else " (server time; tell me where you live for yours)")

    @b.on(r"remind me (?:in|after) (\d+|an?|one) ?([a-z]+) (?:to |that |about )?(.+)",
          r"remind me (?:to |that |about )?(.+?) in (\d+|an?|one) ?([a-z]+)")
    async def in_(ctx: Ctx, m):
        g = m.groups()
        n, unit, what = (g[0], g[1], g[2]) if m.re.pattern.startswith("remind me (?:in") else (g[1], g[2], g[0])
        unit = unit.lower()
        if unit not in UNITS_S:
            return None
        count = 1 if n.lower() in ("a", "an", "one") else int(n)
        secs = count * UNITS_S[unit]
        if not 5 <= secs <= 366 * 86400:
            return "I can remind you anywhere from a few seconds to a year from now."
        add(ctx, time.time() + secs, what)
        return f"OK! I'll remind you to {what} in {count} {unit}. (Even if you're offline, it'll be waiting for you.)"

    @b.on(r"remind me (?:at|by) (\d{1,2})(?::(\d\d))? ?(am|pm)?(?: (tomorrow|today))? (?:to |that |about )?(.+)")
    async def at(ctx: Ctx, m):
        tz = _user_tz(ctx)
        now = datetime.now(tz)
        hour, minute = int(m[1]), int(m[2] or 0)
        if m[3]:
            hour = hour % 12 + (12 if m[3].lower() == "pm" else 0)
        if hour > 23 or minute > 59:
            return "That's not a time I know."
        due = now.replace(hour=hour, minute=minute, second=0, microsecond=0)
        if m[4] == "tomorrow" or due <= now:
            due += timedelta(days=1)
        add(ctx, due.timestamp(), m[5])
        return f"You got it. I'll remind you to {m[5]} at {when(due.timestamp(), tz)}."

    @b.on(r"(?:my |list |show |what are my )?reminders|what did i ask you to remind me")
    async def list_(ctx: Ctx, m):
        mine = [r for r in b.reminders.data if r["login"].lower() == ctx.login.lower()]
        if not mine:
            return "You don't have any reminders. Try \"remind me in 10 minutes to stretch\"."
        tz = _user_tz(ctx)
        return "Your reminders:\n" + "\n".join(f"{i}. {r['text']} ({when(r['due'], tz)})"
                                               for i, r in enumerate(sorted(mine, key=lambda r: r["due"]), 1))

    @b.on(r"(?:cancel|clear|delete|remove) (?:all )?(?:my )?reminders")
    async def clear(ctx: Ctx, m):
        before = len(b.reminders.data)
        b.reminders.data = [r for r in b.reminders.data if r["login"].lower() != ctx.login.lower()]
        b.reminders.save()
        n = before - len(b.reminders.data)
        return f"Cleared {n} reminder{'s' * (n != 1)}." if n else "You didn't have any."


async def run_reminders(b: Brain) -> None:
    """Sends reminders as they come due (the bot calls this every few seconds)."""
    now = time.time()
    due = [r for r in b.reminders.data if r["due"] <= now]
    if not due or not b.send:
        return
    for r in due:
        try:
            await b.send(r["login"], f"Reminder: {r['text']}")
            b.reminders.data.remove(r)
        except Exception:
            pass  # not signed on: try again next time
    b.reminders.save()


# ---------- weather and time ----------

WMO = {0: "clear", 1: "mostly clear", 2: "partly cloudy", 3: "cloudy", 45: "foggy", 48: "foggy",
       51: "drizzly", 53: "drizzly", 55: "drizzly", 56: "freezing drizzle", 57: "freezing drizzle",
       61: "light rain", 63: "rain", 65: "heavy rain", 66: "freezing rain", 67: "freezing rain",
       71: "light snow", 73: "snow", 75: "heavy snow", 77: "snow grains", 80: "showers", 81: "showers",
       82: "heavy showers", 85: "snow showers", 86: "snow showers", 95: "thunderstorms",
       96: "thunderstorms with hail", 99: "thunderstorms with hail"}


def _weather(b: Brain) -> None:
    @b.on(r"(?:what'?s |how'?s )?(?:the )?(weather|forecast|temperature|temp)(?: like)?(?: (?:in|for|at) (.+?))?(?: (?:today|now|tomorrow|this week))?",
          r"(?:is it|will it) (?:going to )?(?:rain|snow|be (?:hot|cold|nice))(?: today| tomorrow)?(?: in (.+))?()",
          r"weather (.+)()")
    async def weather(ctx: Ctx, m):
        where = next((g for g in reversed(m.groups()) if g and g.lower() not in
                      ("weather", "forecast", "temperature", "temp")), None)
        place = await find_place(where) if where else ctx.mem.get("place")
        if where and not place:
            return f"I couldn't find \"{where}\". Try a city, like \"weather in Austin, TX\"."
        if not place:
            ctx.start(_ask_place)
            return "Where are you? Tell me a city (like \"Denver\" or \"Paris, France\") and I'll remember it."
        return await forecast(place, ctx.mem.get("units", "f"))

    async def _ask_place(ctx: Ctx):
        ctx.end()
        p = await find_place(ctx.text)
        if not p:
            return f"I couldn't find \"{ctx.text}\". Ask me again with a city name."
        ctx.remember("place", p)
        return await forecast(p, ctx.mem.get("units", "f"))


async def forecast(p: dict, units: str) -> str:
    try:
        d = await web.get_json("https://api.open-meteo.com/v1/forecast", {
            "latitude": p["lat"], "longitude": p["lon"],
            "current": "temperature_2m,apparent_temperature,weather_code,wind_speed_10m,relative_humidity_2m",
            "daily": "temperature_2m_max,temperature_2m_min,weather_code,precipitation_probability_max",
            "temperature_unit": "celsius" if units == "c" else "fahrenheit",
            "wind_speed_unit": "kmh" if units == "c" else "mph", "timezone": "auto", "forecast_days": 3}, ttl=600)
    except Exception:
        return "The weather service isn't answering right now. Try again in a bit?"
    c, dl = d["current"], d["daily"]
    deg = "°C" if units == "c" else "°F"
    wind = "km/h" if units == "c" else "mph"
    lines = [f"Weather for {p['label']}: {round(c['temperature_2m'])}{deg} and {WMO.get(c['weather_code'], 'weird out')}"
             f" (feels like {round(c['apparent_temperature'])}{deg}), humidity {c['relative_humidity_2m']}%, "
             f"wind {round(c['wind_speed_10m'])} {wind}."]
    for i, label in enumerate(["Today", "Tomorrow", datetime.fromisoformat(dl["time"][2]).strftime("%A")]):
        rain = dl["precipitation_probability_max"][i]
        lines.append(f"{label}: {WMO.get(dl['weather_code'][i], '?')}, high {round(dl['temperature_2m_max'][i])}"
                     f", low {round(dl['temperature_2m_min'][i])}" + (f", {rain}% chance of rain" if rain and rain >= 20 else ""))
    return "\n".join(lines)


def _time(b: Brain) -> None:
    @b.on(r"what time is it(?: in (.+))?", r"(?:the )?time(?: in (.+))?", r"what'?s the time(?: in (.+))?",
          r"what(?:'s| is) (?:the )?date(?: today)?()", r"what day is it(?: today)?()")
    async def time_in(ctx: Ctx, m):
        where = m[1] if m.groups() else None
        if where:
            p = await find_place(where)
            if not p:
                return f"I couldn't find \"{where}\"."
        else:
            p = ctx.mem.get("place")
        tz = ZoneInfo(p["tz"]) if p else None
        now = datetime.now(tz)
        s = now.strftime("%-I:%M %p on %A, %B %-d, %Y")
        return f"It's {s} in {p['label']}." if p else f"It's {s} here (tell me where you live for your time)."


# ---------- math and conversions ----------

_OPS = {ast.Add: operator.add, ast.Sub: operator.sub, ast.Mult: operator.mul, ast.Div: operator.truediv,
        ast.FloorDiv: operator.floordiv, ast.Mod: operator.mod, ast.Pow: operator.pow,
        ast.USub: operator.neg, ast.UAdd: operator.pos}
_FUNCS = {"sqrt": math.sqrt, "sin": lambda x: math.sin(math.radians(x)), "cos": lambda x: math.cos(math.radians(x)),
          "tan": lambda x: math.tan(math.radians(x)), "log": math.log10, "ln": math.log, "abs": abs,
          "round": round, "floor": math.floor, "ceil": math.ceil}
_CONST = {"pi": math.pi, "e": math.e}


def safe_eval(expr: str) -> float:
    """Arithmetic only: numbers, + - * / // % ** (^), parentheses, a few functions."""
    def ev(n):
        if isinstance(n, ast.Expression):
            return ev(n.body)
        if isinstance(n, ast.Constant) and isinstance(n.value, (int, float)) and not isinstance(n.value, bool):
            return n.value
        if isinstance(n, ast.Name) and n.id in _CONST:
            return _CONST[n.id]
        if isinstance(n, ast.UnaryOp) and type(n.op) in _OPS:
            return _OPS[type(n.op)](ev(n.operand))
        if isinstance(n, ast.BinOp) and type(n.op) in _OPS:
            a, c = ev(n.left), ev(n.right)
            if isinstance(n.op, ast.Pow) and (abs(c) > 1000 or abs(a) > 1e6):
                raise ValueError("too big")
            r = _OPS[type(n.op)](a, c)
            if isinstance(r, (int, float)) and abs(r) > 1e300:
                raise ValueError("too big")
            return r
        if isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id in _FUNCS and len(n.args) == 1:
            return _FUNCS[n.func.id](ev(n.args[0]))
        raise ValueError("not arithmetic")

    if len(expr) > 200:
        raise ValueError("too long")
    return ev(ast.parse(expr, mode="eval"))


def fmt_num(x: float) -> str:
    if isinstance(x, float) and x.is_integer() and abs(x) < 1e15:
        x = int(x)
    if isinstance(x, int):
        return f"{x:,}"
    return f"{x:,.6g}" if abs(x) >= 1e-4 else f"{x:.4g}"


LENGTH = {"mm": .001, "cm": .01, "m": 1, "km": 1000, "in": .0254, "ft": .3048, "yd": .9144, "mi": 1609.344,
          "nmi": 1852}
MASS = {"mg": 1e-6, "g": .001, "kg": 1, "lb": .45359237, "oz": .028349523125, "st": 6.35029318,
        "ton": 907.18474, "tonne": 1000}
VOLUME = {"ml": .001, "l": 1, "gal": 3.785411784, "qt": .946352946, "pt": .473176473, "cup": .2365882365,
          "floz": .0295735295625, "tbsp": .01478676478125, "tsp": .00492892159375}
SPEED = {"mph": .44704, "kph": 1 / 3.6, "mps": 1, "knot": .514444}
TIME = {"sec": 1, "min": 60, "hr": 3600, "day": 86400, "week": 604800, "year": 31557600}
DATA = {"byte": 1, "kb": 1e3, "mb": 1e6, "gb": 1e9, "tb": 1e12, "kib": 1024, "mib": 1024 ** 2, "gib": 1024 ** 3}
TABLES = [LENGTH, MASS, VOLUME, SPEED, TIME, DATA]
ALIASES = {
    "millimeter": "mm", "centimeter": "cm", "meter": "m", "metre": "m", "kilometer": "km", "kilometre": "km",
    "inch": "in", "inches": "in", '"': "in", "foot": "ft", "feet": "ft", "'": "ft", "yard": "yd", "mile": "mi",
    "nautical mile": "nmi", "milligram": "mg", "gram": "g", "kilogram": "kg", "kilo": "kg", "pound": "lb",
    "lbs": "lb", "ounce": "oz", "stone": "st", "tons": "ton", "metric ton": "tonne", "milliliter": "ml",
    "liter": "l", "litre": "l", "gallon": "gal", "quart": "qt", "pint": "pt", "fluid ounce": "floz",
    "fl oz": "floz", "tablespoon": "tbsp", "teaspoon": "tsp", "km/h": "kph", "kmh": "kph", "m/s": "mps",
    "knots": "knot", "second": "sec", "s": "sec", "minute": "min", "hour": "hr", "h": "hr", "years": "year",
    "bytes": "byte", "kilobyte": "kb", "megabyte": "mb", "gigabyte": "gb", "terabyte": "tb",
    "celsius": "c", "centigrade": "c", "°c": "c", "fahrenheit": "f", "°f": "f", "kelvin": "k", "degrees c": "c",
    "degrees f": "f", "degrees": "f",
}


def unit(u: str) -> str:
    u = u.lower().strip().rstrip(".")
    for cand in (u, u[:-1] if u.endswith("s") and len(u) > 2 else u, u[:-2] if u.endswith("es") else u):
        if cand in ALIASES:
            return ALIASES[cand]
        if any(cand in t for t in TABLES) or cand in ("c", "f", "k"):
            return cand
    return u


def convert(n: float, a: str, b: str) -> float | None:
    a, b = unit(a), unit(b)
    temps = {"c": (1, 0), "f": (5 / 9, -32), "k": (1, -273.15)}
    if a in temps and b in temps:
        c = (n - 32) * 5 / 9 if a == "f" else n - 273.15 if a == "k" else n
        return c * 9 / 5 + 32 if b == "f" else c + 273.15 if b == "k" else c
    for t in TABLES:
        if a in t and b in t:
            return n * t[a] / t[b]
    return None


def _convert(b: Brain) -> None:
    num = r"(-?\d+(?:\.\d+)?)"
    @b.on(rf"(?:convert |what(?:'s| is) )?{num} ?([a-z°\"'/ .]+?) (?:to|in|into|as|=) ([a-z°\"'/ .]+)",
          r"how many ([a-z°/ .]+?) (?:are )?in (?:an? |one )?(\d+(?:\.\d+)?)? ?([a-z°/ .]+)")
    async def conv(ctx: Ctx, m):
        if m.re.pattern.startswith("how many"):
            n, a, target = float(m[2] or 1), m[3], m[1]
        else:
            n, a, target = float(m[1]), m[2], m[3]
        r = convert(n, a, target)
        if r is None:
            return None
        return f"{fmt_num(n)} {a.strip()} is {fmt_num(r)} {target.strip()}."

    @b.on(rf"(?:what(?:'s| is) )?{num} ?% of {num}")
    async def percent(ctx: Ctx, m):
        return f"{m[1]}% of {m[2]} is {fmt_num(float(m[1]) * float(m[2]) / 100)}."

    @b.on(rf"(?:what(?:'s| is) (?:a |the )?)?(\d+(?:\.\d+)?) ?% tip on \$?{num}", rf"tip(?: on)? \$?{num}()")
    async def tip(ctx: Ctx, m):
        if m[2]:
            pct, bill = float(m[1]), float(m[2])
            return f"A {fmt_num(pct)}% tip on ${bill:,.2f} is ${bill * pct / 100:,.2f} (${bill * (1 + pct / 100):,.2f} total)."
        bill = float(m[1])
        return "\n".join(f"{p}%: ${bill * p / 100:,.2f} tip, ${bill * (1 + p / 100):,.2f} total" for p in (15, 18, 20, 25))


def _calc(b: Brain) -> None:
    @b.on(r"(?:what(?:'s| is)|calc(?:ulate)?|compute|solve|how much is)?\s*([-+*/^%().\d\s x×÷,a-z]+?)\s*=?")
    async def calc(ctx: Ctx, m):
        expr = m[1].strip()
        if not re.search(r"\d", expr) or not re.search(r"[-+*/^%x×÷(]|sqrt|sin|cos|tan|log|ln", expr):
            return None
        words = set(re.findall(r"[a-z]+", expr)) - set(_FUNCS) - set(_CONST) - {"x"}
        if words:
            return None
        e = re.sub(r"(?<=[\d)])\s*[x×]\s*(?=[\d(])", "*", expr).replace("÷", "/").replace("^", "**").replace(",", "")
        try:
            r = safe_eval(e)
        except ZeroDivisionError:
            return random.choice(["Dividing by zero? Nice try.", "Nope. Zero stays out of the bottom of fractions."])
        except Exception:
            return None
        return f"{expr} = {fmt_num(r)}"


# ---------- words ----------

def _define(b: Brain) -> None:
    @b.on(r"(?:define|definition of|meaning of|dictionary|look up the word) \"?([a-z' -]{1,40})\"?",
          r"what does \"?([a-z' -]{1,40})\"? mean", r"what(?:'s| is) the (?:meaning|definition) of \"?([a-z' -]{1,40})\"?")
    async def define(ctx: Ctx, m):
        word = m[1].strip().strip("'\"").lower()
        try:
            data = await web.get_json(f"https://en.wiktionary.org/api/rest_v1/page/definition/{web.quote(word)}",
                                      ttl=86400)
        except Exception:
            return f"I don't know the word \"{word}\". Did you spell it right?"
        lines, n = [word], 0
        for entry in data.get("en", []):
            for d in entry.get("definitions", []):
                text = _plain(d.get("definition", ""))
                if not text:
                    continue
                n += 1
                lines.append(f"{n}. ({entry.get('partOfSpeech', '?').lower()}) {text}")
                if n >= 4:
                    break
            if n >= 4:
                break
        return "\n".join(lines) if n else f"I don't know the word \"{word}\". Did you spell it right?"


def _plain(fragment: str) -> str:
    """Wiktionary's HTML to one line of text."""
    t = re.sub(r"<[^>]+>", "", fragment)
    return " ".join(html.unescape(t).split())


def _wiki(b: Brain) -> None:
    @b.on(r"(?:who|what) (?:is|was|are|were) (?:a |an |the )?(.{2,80})", r"tell me about (.{2,80})",
          r"(?:wiki|wikipedia|look up|search for|search) (.{2,80})", r"who invented (.{2,60})")
    async def wiki(ctx: Ctx, m):
        q = m[1].strip()
        try:
            s = await web.get_json("https://en.wikipedia.org/w/api.php", {
                "action": "opensearch", "search": q, "limit": 1, "namespace": 0, "format": "json"}, ttl=3600)
            if not s[1]:
                return f"I looked, but I couldn't find anything about \"{q}\"."
            page = await web.get_json(f"https://en.wikipedia.org/api/rest_v1/page/summary/{web.quote(s[1][0])}",
                                      ttl=3600)
        except Exception:
            return "Wikipedia isn't answering right now. Ask me again in a bit?"
        text = page.get("extract") or ""
        if not text:
            return f"I found a page called \"{s[1][0]}\" but it's empty. Weird."
        parts = _chunks(text, 420)
        if len(parts) > 1:
            ctx.start(_more_mode, parts=parts[1:], url=page.get("content_urls", {}).get("desktop", {}).get("page"))
            return parts[0] + "\n(Say \"more\" for more.)"
        url = page.get("content_urls", {}).get("desktop", {}).get("page")
        return text + (f"\n{url}" if url else "")


async def _more_mode(ctx: Ctx):
    if ctx.low not in ("more", "yes", "y", "go on", "continue", "keep going", "and"):
        ctx.end()
        return None
    parts = ctx.state["parts"]
    nxt = parts.pop(0)
    if parts:
        return nxt + "\n(\"more\"?)"
    ctx.end()
    return nxt + (f"\nThe rest: {ctx.state.get('url')}" if ctx.state.get("url") else "")


def _chunks(text: str, size: int) -> list[str]:
    sentences = re.split(r"(?<=[.!?])\s+", text)
    out, cur = [], ""
    for s in sentences:
        if cur and len(cur) + len(s) > size:
            out.append(cur)
            cur = s
        else:
            cur = f"{cur} {s}".strip()
    if cur:
        out.append(cur)
    return out


def _on_this_day(b: Brain) -> None:
    @b.on(r"(?:what happened )?on this day|today in history|this day in history|history")
    async def otd(ctx: Ctx, m):
        now = datetime.now()
        try:
            d = await web.get_json(
                f"https://api.wikimedia.org/feed/v1/wikipedia/en/onthisday/selected/{now:%m}/{now:%d}", ttl=3600)
        except Exception:
            return "My history books are closed right now. Try later?"
        events = random.sample(d.get("selected", []), min(3, len(d.get("selected", []))))
        if not events:
            return "Nothing happened today, apparently. Ever."
        return f"On this day ({now:%B} {now.day}):\n" + "\n".join(
            f"{e['year']}: {e['text']}" for e in sorted(events, key=lambda e: e["year"]))


# ---------- Hotline chat rooms ----------

TRACKERS = ["hltracker.com", "tracker.preterhuman.net", "hotline.kicks-ass.net", "saddle.dyndns.org"]
_rooms_cache: tuple[float, list] = (0, [])


async def _tracker(host: str) -> list[tuple[str, int, str, str]]:
    r, w = await asyncio.wait_for(asyncio.open_connection(host, 5498), 6)
    try:
        w.write(b"HTRK\x00\x01")
        await w.drain()
        if (await asyncio.wait_for(r.readexactly(6), 6))[:4] != b"HTRK":
            return []
        out, total = [], None
        for _ in range(200):
            if total is not None and len(out) >= total:
                break
            h = await asyncio.wait_for(r.readexactly(8), 6)
            t, n = struct.unpack(">HH", h[4:8])
            total = t if total is None else total
            for _ in range(n):
                fixed = await r.readexactly(10)
                name = await r.readexactly((await r.readexactly(1))[0])
                desc = await r.readexactly((await r.readexactly(1))[0])
                ip = ".".join(str(x) for x in fixed[:4])
                users = struct.unpack(">H", fixed[6:8])[0]
                out.append((f"{ip}:{struct.unpack('>H', fixed[4:6])[0]}", users,
                            name.decode("mac_roman").strip(), desc.decode("mac_roman").strip()))
            if n == 0:
                break
        return out
    finally:
        w.close()


async def hotline_rooms() -> list:
    global _rooms_cache
    if _rooms_cache[0] > time.time():
        return _rooms_cache[1]
    results = await asyncio.gather(*(_tracker(t) for t in TRACKERS), return_exceptions=True)
    merged: dict[str, tuple] = {}
    for res in results:
        if isinstance(res, list):
            for addr, users, name, desc in res:
                if not re.search(r"[a-z0-9]", name, re.I) or "----" in name or "MAJOR MAC BACKUP" in name \
                        or "welcome to hotline" in name.lower():
                    continue
                if addr not in merged or users > merged[addr][1]:
                    merged[addr] = (addr, users, name, desc)
    rooms = sorted(merged.values(), key=lambda r: (-r[1], r[2].lower()))
    _rooms_cache = (time.time() + 300, rooms)
    return rooms


def _rooms(b: Brain) -> None:
    @b.on(r"(?:hotline )?(?:chat ?rooms?|servers|rooms)|who'?s (?:on|online)(?: on hotline)?|what'?s (?:busy|popular|happening)(?: on hotline)?")
    async def rooms(ctx: Ctx, m):
        try:
            rs = await hotline_rooms()
        except Exception:
            rs = []
        if not rs:
            return "I couldn't reach the Hotline trackers just now."
        people = sum(r[1] for r in rs)
        lines = [f"{len(rs)} Hotline servers are up, with {people} people on. The busiest:"]
        for addr, users, name, desc in rs[:8]:
            lines.append(f"{name} ({users}): {desc[:60]}" if desc else f"{name} ({users})")
        lines.append("Join one from HIM: People > Join a Chat Room.")
        return "\n".join(lines)


# ---------- games ----------

TRIVIA_BACKUP = [
    ("What year did AOL Instant Messenger launch?", "1997", ["1995", "1999", "2001"]),
    ("What was the name of the Mac OS released in 1999 with Sherlock 2?", "Mac OS 9", ["Mac OS 8", "System 7", "Rhapsody"]),
    ("Which company made the Hotline software?", "Hotline Communications", ["Netscape", "Apple", "Be Inc."]),
    ("How many bits are in a byte?", "8", ["4", "16", "10"]),
    ("What is the largest planet in our solar system?", "Jupiter", ["Saturn", "Neptune", "Earth"]),
    ("What is H2O better known as?", "Water", ["Hydrogen peroxide", "Salt", "Helium"]),
    ("Who painted the Mona Lisa?", "Leonardo da Vinci", ["Michelangelo", "Raphael", "Donatello"]),
    ("How many sides does a hexagon have?", "6", ["5", "7", "8"]),
    ("Which planet is called the Red Planet?", "Mars", ["Venus", "Mercury", "Jupiter"]),
    ("What is the capital of Australia?", "Canberra", ["Sydney", "Melbourne", "Perth"]),
]


async def _trivia_question() -> tuple[str, str, list[str], str]:
    try:
        d = await web.get_json("https://opentdb.com/api.php", {"amount": 1, "type": "multiple"})
        q = d["results"][0]
        return (html.unescape(q["question"]), html.unescape(q["correct_answer"]),
                [html.unescape(x) for x in q["incorrect_answers"]], html.unescape(q["category"]))
    except Exception:
        q, a, wrong = random.choice(TRIVIA_BACKUP)
        return q, a, list(wrong), "General"


async def _trivia_ask(ctx: Ctx) -> str:
    q, right, wrong, cat = await _trivia_question()
    choices = wrong + [right]
    random.shuffle(choices)
    ctx.start(_trivia_mode, choices=choices, right=right, asked=True)
    letters = "ABCD"
    return f"{cat}: {q}\n" + "\n".join(f"{letters[i]}. {c}" for i, c in enumerate(choices)) + \
        "\n(Answer with a letter. \"quit\" to stop.)"


async def _trivia_mode(ctx: Ctx):
    st = ctx.state
    if not st.get("asked"):  # waiting on "another?"
        if ctx.low in ("yes", "y", "yeah", "yep", "sure", "another", "again", "next", "ok", "k"):
            return await _trivia_ask(ctx)
        ctx.end()
        return None
    choices, right = st["choices"], st["right"]
    pick = None
    if len(ctx.low) == 1 and ctx.low in "abcd":
        pick = choices["abcd".index(ctx.low)]
    else:
        pick = next((c for c in choices if c.lower() == ctx.low), None)
    if pick is None:
        return "Pick A, B, C or D. (Or \"quit\".)"
    mem = ctx.mem
    mem["trivia_total"] = mem.get("trivia_total", 0) + 1
    ok = pick == right
    if ok:
        mem["trivia_right"] = mem.get("trivia_right", 0) + 1
    ctx.brain.memory.store.save()
    st["asked"] = False
    score = f"You're {mem.get('trivia_right', 0)} for {mem['trivia_total']}."
    lead = random.choice(["Correct!", "Yep!", "You got it!", "Nailed it."]) if ok else \
        random.choice([f"Nope, it was {right}.", f"Sorry! The answer is {right}.", f"Close, but it's {right}."])
    return f"{lead} {score} Another one?"


HANGMAN_WORDS = """modem buddy hotline away message download password keyboard monitor screen
internet chatroom emoticon smiley computer floppy printer scanner joystick website browser
network server tracker nostalgia winamp playlist napster pager beeper walkman gameboy
tamagotchi dialup broadband pixel sprite icon folder desktop cursor window mouse""".split()


def _hangman_board(st) -> str:
    shown = " ".join(c if c in st["got"] else "_" for c in st["word"])
    missed = ", ".join(sorted(st["miss"])) or "none"
    return f"{shown}\nMisses: {missed} ({6 - len(st['miss'])} left)"


async def _hangman_mode(ctx: Ctx):
    st = ctx.state
    g = ctx.low.replace(" ", "")
    if not g.isalpha():
        return "Guess a letter (or the whole word)."
    if len(g) > 1:
        if g == st["word"]:
            ctx.end()
            return f"Yes! It was {st['word']}. You win!"
        st["miss"].add(g)
    elif g in st["got"] or g in st["miss"]:
        return f"You already tried {g}.\n{_hangman_board(st)}"
    elif g in st["word"]:
        st["got"].add(g)
        if all(c in st["got"] for c in st["word"]):
            ctx.end()
            return f"{st['word']}! You got it with {6 - len(st['miss'])} guesses to spare."
    else:
        st["miss"].add(g)
    if len(st["miss"]) >= 6:
        ctx.end()
        return f"Out of guesses! The word was {st['word']}. Rematch? Say \"hangman\"."
    return _hangman_board(st)


async def _number_mode(ctx: Ctx):
    st = ctx.state
    try:
        n = int(ctx.low)
    except ValueError:
        return "Guess a number from 1 to 100."
    st["tries"] += 1
    if n == st["n"]:
        ctx.end()
        return f"{n}! You got it in {st['tries']} {'try' if st['tries'] == 1 else 'tries'}."
    return "Higher!" if n < st["n"] else "Lower!"


EIGHT_BALL = ["Definitely.", "Signs point to yes.", "Ask again after lunch.", "My sources say no.",
              "Absolutely not.", "It is certain. Well, pretty certain.", "Don't count on it.",
              "The future is hazy. Try defragging it.", "Yes, but you won't like it.", "No way, José.",
              "Outlook good.", "I wouldn't bet your Beanie Babies on it.", "Without a doubt.",
              "Better not tell you now.", "Very doubtful.", "You already know the answer."]


def _games(b: Brain) -> None:
    @b.on(r"(?:play |let'?s play |start )?trivia(?: game)?|quiz me|ask me (?:a )?(?:question|trivia)")
    async def trivia(ctx: Ctx, m):
        return await _trivia_ask(ctx)

    @b.on(r"(?:play |let'?s play |start )?hangman")
    async def hangman(ctx: Ctx, m):
        ctx.start(_hangman_mode, word=random.choice(HANGMAN_WORDS), got=set(), miss=set())
        return "Hangman! Guess a letter.\n" + _hangman_board(ctx.state)

    @b.on(r"(?:play |let'?s play )?guess (?:a|the|my) number|number game")
    async def guess(ctx: Ctx, m):
        ctx.start(_number_mode, n=random.randint(1, 100), tries=0)
        return "I'm thinking of a number from 1 to 100. Guess!"

    @b.on(r"(?:magic )?8[- ]?ball\s*(.*)|(?:will|should|am|is|does|do|can) i .+")
    async def eight(ctx: Ctx, m):
        return random.choice(EIGHT_BALL)

    @b.on(r"(?:flip|toss) (?:a )?coin|heads or tails")
    async def coin(ctx: Ctx, m):
        return random.choice(["Heads!", "Tails!"]) if random.random() > .002 else "It landed on its edge. Seriously."

    @b.on(r"roll(?: (?:a|the|some))?(?: (\d{1,2}))? ?(?:d(\d{1,3})|dice|die)")
    async def roll(ctx: Ctx, m):
        n, sides = int(m[1] or 1), int(m[2] or 6)
        if not 1 <= n <= 20 or not 2 <= sides <= 1000:
            return "Let's keep it to 20 dice with up to 1000 sides."
        rolls = [random.randint(1, sides) for _ in range(n)]
        return f"🎲 {', '.join(map(str, rolls))}" + (f" (total {sum(rolls)})" if n > 1 else "")

    @b.on(r"(?:play )?rock,? paper,? scissors|rps|(rock|paper|scissors)")
    async def rps(ctx: Ctx, m):
        mine = random.choice(["rock", "paper", "scissors"])
        if not m.lastindex:
            return "Rock, paper, or scissors? Pick one!"
        theirs = m[1].lower()
        beats = {"rock": "scissors", "paper": "rock", "scissors": "paper"}
        if mine == theirs:
            return f"{mine.title()}! A tie. Again?"
        return f"{mine.title()}! " + ("I win!" if beats[mine] == theirs else "You win. Grr.")

    @b.on(r"pick (?:a |one )?(?:between |from )?(.+ or .+)")
    async def pick(ctx: Ctx, m):
        opts = [o.strip() for o in re.split(r",|\bor\b", m[1]) if o.strip()]
        return f"I pick {random.choice(opts)}." if len(opts) > 1 else None


# ---------- jokes ----------

JOKES = [
    "Why did the computer go to the doctor? It had a virus. (Don't worry, it's not contagious over IM.)",
    "Why was the math book sad? It had too many problems.",
    "What do you call a fake noodle? An impasta.",
    "Why don't skeletons fight each other? They don't have the guts.",
    "Why did the scarecrow win an award? He was outstanding in his field.",
    "What did the ocean say to the beach? Nothing, it just waved.",
    "Why can't you trust atoms? They make up everything.",
    "How does a computer get drunk? It takes screenshots.",
    "Why did the modem break up with the phone line? Too much static in the relationship.",
    "What's a bot's favorite snack? Computer chips. Obviously.",
    "Why did the bicycle fall over? It was two-tired.",
    "I told my computer a joke about UDP. I'm not sure it got it.",
    "What do you call a bear with no teeth? A gummy bear.",
    "Why do programmers prefer dark mode? Because light attracts bugs.",
    "What did one wall say to the other? I'll meet you at the corner.",
    "Why did the cookie go to the hospital? It felt crummy.",
    "What do you call cheese that isn't yours? Nacho cheese.",
    "Why was the broom late? It over-swept.",
]
FORTUNES = [
    "A buddy you haven't talked to in years is about to sign on.",
    "Your away message will make someone laugh today.",
    "Good things come to those who don't refresh the buddy list every five seconds.",
    "You will download something wonderful. At 56k, eventually.",
    "Today is a good day to change your buddy icon.",
    "The answer you seek is in your profile. Update it.",
    "Someone is typing... about you. Probably nice things.",
    "Fortune favors the bold, and the people with good screen names.",
]


def _jokes(b: Brain) -> None:
    @b.on(r"(?:tell me |tell |say )?(?:a |another |one more )?(?:joke|something funny|make me laugh)(?: please)?")
    async def joke(ctx: Ctx, m):
        return random.choice(JOKES)

    @b.on(r"(?:my |tell my |what'?s my )?fortune(?: cookie)?|tell my fortune")
    async def fortune(ctx: Ctx, m):
        return "Your fortune: " + random.choice(FORTUNES)


for _game in (_trivia_mode, _hangman_mode, _number_mode):
    _game.game = True  # games are for IM; the Hub chat points people there instead
