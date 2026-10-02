# The Translator

A buddy that translates. Add **translator** on VesperNet in HIM (or Adium or Pidgin)
and send it something in another language: it answers in English. Start a message
with a language to translate into it.

| You send | It answers |
|---|---|
| `¿dónde está la biblioteca?` | (Spanish → English) Where is the library? |
| `english: ¿dónde está la biblioteca?` | the same, saying so |
| `spanish: where is the library?` | (English → Spanish) ¿Dónde está la biblioteca? |
| `es: where is the library?` | codes work too, and native names (`español:`) |
| `my language is french` | from now on, it translates into French when you don't say |
| `languages` | the ones it knows |
| `help` | this list |

The translating is done by [LibreTranslate](https://github.com/LibreTranslate/LibreTranslate)
(open source), running on the same server; messages aren't sent to any outside
service. It runs from SmarterChild's code (`../smarterchild`, the `translator` command).

## Running it

```sh
cp .env.example .env    # then fill in the password
docker compose up -d --build
docker compose logs -f
```

The first start downloads the language models (a few GB, into a Docker volume), which
takes a while; until then the bot says its dictionary is stuck. LibreTranslate
wants roughly 2 to 4 GB of memory with the default 20 languages: trim
`LT_LOAD_ONLY` in `.env` on a small server.
