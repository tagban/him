# SmarterChild for Hotline

A buddy that answers, in the spirit of AIM's SmarterChild: add **smarterchild** on
VesperNet in [HIM](../../README.md) and ask it things. It's plain rules and free web
services; there's no AI model and no API key.

It can also sit in a Hotline server's public chat (the Hotline Central Hub) as a
robot (classic icon 168), answering only lines addressed to it, and pointing people
who ask where it's from to HIM.

## What it does

Type **help** for the numbered menu. Some things to try:

| | |
|---|---|
| Weather | `weather in Boston`, `forecast`, `I live in Portland, OR` (then just `weather`), `use celsius` |
| Look it up | `who is Ada Lovelace`, `tell me about Hotline`, then `more` |
| Dictionary | `define serendipity`, `what does ennui mean` |
| Math | `12*(3+4)`, `2^10`, `15% of 80`, `sqrt(144)`, `20% tip on 45`, `tip 60` |
| Conversions | `10 km in miles`, `72 f in c`, `how many ounces in a pound` |
| Time | `what time is it in Tokyo`, `what's the date` |
| Reminders | `remind me in 20 minutes to check the oven`, `remind me at 5pm to call Mom`, `my reminders` |
| Games | `trivia`, `hangman`, `guess a number`, `rock paper scissors`, `8 ball will I win`, `roll 2d6`, `flip a coin` |
| News | `news`, `headlines` (from Wikipedia's In the news) |
| Hotline | `chat rooms` (the busiest servers, from the trackers) |
| Fun | `tell me a joke`, `fortune`, `on this day`, `fun fact` |
| About you | `my name is Sam`, `what's my name`, `remember my favorite band is Weezer`, `what do you know about me`, `forget me` |

And small talk, with some attitude.

Reminders are sent as IMs, so they wait on the server if you're offline.

In IM you just talk to it. In a **public chat**, where everyone shares one room, it
answers only:

- `!` commands: `!weather Boston` (`!w`), `!news`, `!define ennui` (`!d`), `!wiki Hotline`,
  `!time Tokyo`, `!calc 12*7`, `!joke`, `!fact`, `!rooms`, `!otd`, `!8ball will it work`,
  `!roll 2d6`, `!help`, or any question after a `!`;
- lines that start or end with its name (`SmarterChild, what's 6*7?`);
- the weather for a named place (`weather in Austin`).

People on Discord count too: lines the
[Discord bridge](https://github.com/tagban/hotline_discord_bridge) posts for them
(`Discord | Name: !weather Boston`) are answered as theirs. Answers there are kept to a few lines; games and follow-ups are for IM. Private
messages sent to it in a server's chat are answered like IMs, and `!` works in IM
too for anyone used to it.

### Free services it uses

- **Weather and places:** [Open-Meteo](https://open-meteo.com).
- **Wikipedia and Wiktionary:** from Wikimedia.
- **Trivia questions:** [Open Trivia DB](https://opentdb.com), with a built-in set when it's down.
- **The chat room list:** the Hotline trackers.

## Running it

It needs an account on the IM server (**smarterchild** on VesperNet). Its settings go in
`.env` (copy `.env.example`), which git ignores; the password never goes in the repo.

On a server, with Docker:

```bash
cp .env.example .env          # then put the password in it
docker compose up -d --build
docker compose logs -f
```

Or with Python 3.10+:

```bash
python3 -m venv .venv && .venv/bin/pip install .
.venv/bin/smarterchild         # reads .env in the current folder
```

What it remembers (names, places, facts, trivia scores, reminders) is kept as JSON in
`data/` (`/data` in Docker, mounted from `./data`).

| Setting | Default | |
|---|---|---|
| `HOTLINE_HOST`, `HOTLINE_PORT` | `hotline.vespernet.net`, `5500` | The IM server |
| `SMARTERCHILD_LOGIN`, `SMARTERCHILD_PASSWORD` | | Its account (required) |
| `SMARTERCHILD_NAME` | `SmarterChild` | The name buddies see (set on every sign-on) |
| `SMARTERCHILD_STATUS` | `Ask me anything! Type "help".` | Its status line |
| `SMARTERCHILD_DATA` | `data` | Where it keeps what it remembers |
| `HUB_HOST`, `HUB_PORT` | none, `5500` | Servers whose public chat to join, comma-separated (`host` or `host:port`); empty stays out |
| `HUB_LOGIN`, `HUB_PASSWORD` | guest | An account there, if guests can't chat |
| `HUB_ICON` | `168` | Its classic user icon |
| `HUB_TRIGGER` | `!` | What starts a chat command |

It signs on with HOPE (HMAC-SHA256) and encrypts the session with ChaCha20-Poly1305
when the server agrees, the same as HIM. `smarterchild/hotline.py` is a small
Python client for the Hotline IM protocol that other bots can reuse.

## Adding a skill

Skills are functions in `smarterchild/skills.py`, each with the patterns that reach
it (the first full match wins, so put specific ones first):

```python
@b.on(r"(?:what'?s|tell me) the (?:secret|password)")
async def secret(ctx, m):
    return f"The password is swordfish, {ctx.name}."
```

A skill can return one reply, a list of them, or `None` to let the next skill try.
`ctx.start(handler, **state)` hands the person's next messages to a game or a
follow-up (see hangman); `ctx.remember(key, value)` keeps something about them.

## Tests

```bash
cargo build -p hotline-im --features mock-server --bin mock-server   # from the repo root
.venv/bin/pip install pytest && .venv/bin/python -m pytest
```

The protocol and Hub tests run against HIM's test server. The brain's tests need no
network.

SmarterChild was ActiveBuddy's bot for AIM and MSN (2000); this is a fan re-creation
for the Hotline network and isn't affiliated with Microsoft, which owns the name.

## What it didn't understand

Messages it shrugged at, and questions only Wikipedia or the dictionary caught
(right for "who is Ada Lovelace", wrong for "what are you wearing"), are counted in
`data/misses.json`: the words only, never who said them, and nothing that looks like
an email address or phone number. The most asked are where new answers
(`smarterchild/answers.py`) help most:

```sh
docker exec smarterchild smarterchild-misses              # the top 40
docker exec smarterchild smarterchild-misses 100 fallback # the top 100 shrugs
docker exec smarterchild smarterchild-misses --clear      # start over
```
