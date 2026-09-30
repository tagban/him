# HIM, the Hotline Instant Messenger

A buddy list in the spirit of late-90s AIM, for the Hotline network: gray
Windows 98 windows, the Sign On screen, a Buddy List with Online and List Setup
tabs, one little window per conversation, away messages, door sounds when
buddies come and go, and chat rooms.

- **Instant messages** use fogWraith's
  [Hotline Instant Messaging extension](https://github.com/fogWraith/Hotline/blob/main/Docs/IM/Client-Creation-Guide.md)
  on any Hotline server that has messaging turned on (Janus). Buddy lists live on
  the server, with buddy requests, presence, away messages, offline messages, and
  delivered/read receipts.
- **Chat rooms** are ordinary Hotline servers. HIM lists every server the public
  trackers know about, joins one as a guest, and shows its public chat the way AIM
  showed a chat room: the conversation, and the people there, by name only.

HIM starts out pointed at VesperNet (`hotline.vespernet.net`), a Janus server with
messaging on; **Setup** on the Sign On window changes it. Hotline has no way to make
an account from a client, so **Get a Screen Name** opens the server's own sign-up
page (for servers HIM knows one for; see `SIGNUP_PAGES` in `src-tauri/src/settings.rs`).

No IM account? **Chat rooms only** on the Sign On window joins rooms as a guest.

macOS first; Windows and Linux later (it's Tauri, so they're mostly a build away).

## Download

From [Releases](https://github.com/tagban/him/releases):

| System | File |
|---|---|
| macOS, Apple silicon | `HIM_x.y.z_aarch64.dmg` (signed and notarized) |
| macOS, Intel | `HIM_x.y.z_x64.dmg` (signed and notarized) |
| Windows | `HIM_x.y.z_x64-setup.exe`, or the `.msi` |
| Linux | `HIM_vx.y.z_x86_64.flatpak` or `_aarch64.flatpak` (`flatpak install --user HIM_*.flatpak`), or the `.deb` |

The flatpak needs the GNOME 47 runtime from Flathub
(`flatpak install flathub org.gnome.Platform//47`).

## Running it from source

```bash
cargo run -p him
```

You need a server with messaging enabled and an account on it. To try everything
without one, run the test server in another terminal:

```bash
cargo run -p hotline-im --features mock-server --bin mock-server
```

It listens on 127.0.0.1:5500 with accounts `alice`, `bob`, `carol` and `dave`
(password `hotline`), and HotBot, who answers every message. In HIM, click
**Setup**, enter `127.0.0.1`, and sign on as `alice`. Scripted people can join in:

```bash
cargo run -p hotline-im --example buddy -- bob hotline alice
cargo run -p hotline-im --example chatter -- Pat
```

`buddy` signs on as bob, messages alice, goes away and signs off (and accepts
buddy requests). `chatter` joins the test server's chat room and says a few things.

## How it's built

| Folder | What's there |
|---|---|
| `hotline-im/` | The protocol, as a Rust library: transactions, HOPE secure sign-on (HMAC-SHA256) with ChaCha20-Poly1305 encryption, TLS, the info port, UTF-8/Mac Roman text, the 800-block messaging transactions, classic chat and user lists, and tracker lists. Also `mock` (the test server) and its tests. |
| `src-tauri/` | The app: the session, reconnecting, chat rooms, settings (passwords go to the keychain), sounds. |
| `ui/` | The windows, in plain HTML/CSS/JS: `aim.css` is the Windows 98 look; `common.js` has the title bars, menus, icons and the synthesized sounds. |
| `art/` | The app icon's source. The red H is the Hotline mark from bigredh.com, redrawn as a vector (`ui/img/hotline-h.svg`). |

```bash
cargo test -p hotline-im      # protocol and end-to-end flows against the mock server
cargo tauri build             # HIM.app and a .dmg in target/release/bundle
```

### Connections

**Automatic** (the default) uses HOPE on the server's normal port: the password is
proven with HMAC-SHA256 and never sent, and when the server agrees the whole
session is encrypted with ChaCha20-Poly1305. HIM uses TLS only when the server has
no HOPE or requires TLS. A TLS attempt whose certificate can't be verified hangs up
mid-handshake, and Janus can take that for a hostile probe and ban the address.
If a server has neither HOPE nor TLS, HIM falls back to the classic login and says
so in the Buddy List's status bar.

Chat rooms sign on as a guest, the classic way, since public chat gains nothing
from encryption. Server agreements are accepted automatically and not shown.

### Smileys

Type `:-)`, `;-)`, `:-P`, `8-)`, `<3` and the rest (or pick one with the smiley
button) and IM windows and chat rooms draw them as little yellow faces, AIM style.
The message itself stays plain text. **Setup > Show smileys as pictures** turns it off.

### Sounds

HIM makes its own door, message and alert sounds (synthesized at run time; no
recordings ship with it). **My HIM > Setup > Sounds** takes any sound file for
each event instead, so you can use the classic ones from an install you have.

### Buddy Icon

**My HIM > Buddy Icon** takes a GIF (animated is fine), PNG or JPEG from a file, or
from one pasted link (say, an icon you found on BadassBuddy). **Choose File** also
opens a .zip: one picture is used as is, and a collection (BadassBuddy's download)
opens as a searchable gallery, reachable again later under **My Icons...**. Pictures bigger than
64 x 64 are scaled to 48 x 48 first. In an IM window your buddy's icon sits in the
lower-left and yours in the lower-right, as in AIM; Buddy Info shows it too.

Icons are kept by the server for your account, so buddies see yours wherever you
sign on. This is the Buddy Icons extension
([Capabilities-Buddy-Icons.md](https://github.com/fogWraith/Hotline/blob/main/Docs/Protocol/Capabilities-Buddy-Icons.md));
on a server without it, your icon stays on your side. Buddies' icons are fetched
only when theirs changes, and cached by hash. Details: [docs/BUDDY-ICONS.md](docs/BUDDY-ICONS.md).

To try icons with the test server, give accounts pictures:
`MOCK_ICONS="hotbot=robot.gif,bob=bob.png" cargo run -p hotline-im --features mock-server --bin mock-server`.

### Diagnostics

HIM keeps a short log, `him.log`, next to its settings
(`~/Library/Application Support/com.tagban.him/`). It records what the server said about
Buddy Icons (roster hashes, fetches, uploads) and never holds passwords or messages.

### Testing without a mouse (debug builds)

| Variable | Effect |
|---|---|
| `HIM_DEV_SIGNON="login:password@host:port"` | Signs on at launch (empty: stays on the Sign On window). |
| `HIM_DEV_OPEN="away,info=bob,chatrooms"` | Opens dialogs after signing on. |
| `HIM_DEV_EVAL="label\|delay_ms\|js;;..."` | Runs page code in a window (label `open` opens a dialog). |
| `HIM_DEV_IM="login:text"` | Opens an IM window and sends one message. |

## Credits

The protocol is fogWraith's; see the [Hotline documentation](https://github.com/fogWraith/Hotline).
The tracker code follows BigRedH's crawler.
