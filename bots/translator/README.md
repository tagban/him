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

It also sits in the Hotline Central Hub's public chat (`HUB_HOST` in `.env`), where it
answers only `!translate` (or `!tr`) lines, including ones the Discord bridge relays:

| In chat | It says |
|---|---|
| `!translate ¿dónde está la biblioteca?` | Pat: (Spanish → English) Where is the library? |
| `!translate spanish: where is the library?` | into Spanish (also `!translate to spanish ...`, `!tr es: ...`) |
| `!translate` | how to use it |

Private messages to it on the Hub are answered like IMs.

The translating is done by Google Translate's free endpoint (the one its Chrome extension
uses; no account or key). It's unofficial, so Google could change or limit it. With a
[LibreTranslate](https://github.com/LibreTranslate/LibreTranslate) server of your own
(it wants 2 to 4 GB of memory), set `TRANSLATE_ENGINE=libre` and `TRANSLATE_URL` in `.env`.
What people send it goes to Google to be translated.

It runs from SmarterChild's code (`../smarterchild`, the `translator` command).

## Running it

```sh
cp .env.example .env    # then fill in the password
docker compose up -d --build
docker compose logs -f
```
