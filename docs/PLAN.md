# Plan

## Where it stands (2026-09-30)

Done and tested against the mock server (and, for the room list, the live trackers):

- Sign On window, saved screen names, Save password (keychain), Auto-login.
- Buddy List: Online / List Setup tabs, local groups (add, rename, delete, drag
  buddies between them), door icons and sounds as buddies come and go, away and
  busy marks, unread counts, reconnecting on its own after a dropped connection.
- IM windows: receipts (queued, delivered, read), typing notices, smileys, text
  size, save conversation.
- Add Buddy (with a note), buddy requests (accept, decline, block), remove, block
  and unblock, rename (private alias), Find a Buddy (the server directory),
  Buddy Info and Edit Profile.
- Away messages (saved list, auto-response once per buddy), I'm Back.
- Setup: sounds on/off and your own sound files, timestamps, text size,
  auto-response, directory listing.
- Chat rooms: the room list from the public trackers (always hidden: names containing
  "MAJOR MAC BACKUP", "----" or "Welcome to hotline"; `hiddenRooms` in settings adds more), recent rooms, any
  server by address, guest sign-on, public chat with join and leave notices,
  /me, Ignore, save transcript, rejoin. Agreements are accepted automatically.
  "Chat rooms only" on the Sign On window joins rooms as a guest without an IM account.
- Buddy Icons (fogWraith's Buddy Icons extension; docs/BUDDY-ICONS.md): yours from a
  file or a pasted link (BadassBuddy), scaled to 48 x 48 when needed, kept on the
  server and synced across sessions. Buddies' icons show animated in the lower-left
  of IM windows and in Buddy Info, fetched on change and cached by hash. The test
  server implements the server side (Janus support is in progress). A built-in
  BadassBuddy gallery waits on the site owner's OK (docs/BADASSBUDDY.md).
- Default server: VesperNet (hotline.vespernet.net:5500), changeable in Setup;
  "Get a Screen Name" opens the server's sign-up page (agora.vespernet.net/messenger).
- Smileys: :-) ;-) :-( :-P :-D =-O :-* >:o 8-) :-$ :-! :-[ O:-) :-\ :'( :-X <3 drawn as
  AIM-style faces (HIM's own drawings) in IMs and chat rooms, with a picker; the text
  itself is sent unchanged, so other clients see the codes. Setup can turn it off.
- Windows reopen where you left them; IM windows note when the buddy signs on or off.
- Tested for real: HOPE sign-on to VesperNet (Janus), buddy list and away
  status, IM, and a chat room (MacDomain).
- Tested for real: the live room list from the four trackers, and a silent guest join
  of The Mobius Strip (agreement accepted, user list read).

## Next

1. **A real server.** Everything above has run against the mock server only.
   Sign on to a Janus server with messaging enabled and check each flow, since
   Janus may differ from the spec in places the mock follows it.
2. **Discord** (the reason for the plan): the Discord Social SDK in the app, with
   Discord friends in the Buddy List (a Discord badge on them), DMs that land in
   their regular Discord, and Rich Presence with a Join button for the chat room
   you're in. Needs a Discord application (yours) and the SDK download from the
   Developer Portal. Development is capped at 100 DMs every 2 hours until Discord
   approves production use.
3. Buddy Icons against Janus once fogWraith ships them (chat rooms stay names-only).
4. File transfer (IM File Offer/Accept and the HTXF relay), then voice.
5. Windows and Linux builds, signing and notarizing the Mac build.

## Separate projects

- A libpurple plugin (Pidgin, and Adium through a small wrapper) speaking Hotline
  IM with HOPE, in C, in its own repository.
- hlwiki: a Protocol Extensions page (drafted on the `protocol-extensions` branch
  of the wiki repo, not published), with HIM added to it once released.
