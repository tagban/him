# Discord in HIM (not built yet)

The point: every AIM revival fails for lack of people. Discord already has them.
With the **Discord Social SDK**, HIM can show a user's Discord friends in the same
Buddy List, with a Discord badge, and IMs to them land in the friend's regular
Discord, so they can answer without installing anything.

Nothing here needs fogWraith's server (Janus) to change: Discord lives entirely in
the app, beside the Hotline connection.

## What you need to do first (Developer Portal, your account)

1. Create a developer team, and an application in it named HIM.
2. **OAuth2**: add the redirect URI `http://127.0.0.1/callback` and turn on
   **Public Client** (a desktop app can't keep a client secret).
3. **Discord Social SDK > Getting Started**: fill in the form to enable the SDK.
   It's written for games; describe HIM honestly, with Rich Presence "Join" as the
   way to bring a friend into the Hotline chat room you're in.
4. **Downloads**: get the latest C++ SDK for macOS. The parts HIM needs are
   `lib/release/libdiscord_partner_sdk.dylib` and `include/discordpp.h`. Put the SDK
   folder at `third_party/discord_social_sdk/` in this repo (it isn't
   redistributable in source form, so it stays out of git).
5. Send over the **Application ID** (it isn't secret).

## How it will work

- **Sign in**: Setup gets a "Connect Discord" button. The SDK opens the browser
  (PKCE: `CreateAuthorizationCodeVerifier`, `Authorize`, `GetToken`), HIM keeps
  the token in the keychain beside the Hotline password, then `UpdateToken` and
  `Connect`. The SDK needs `RunCallbacks` called regularly, so it gets its own
  thread in the Rust backend, behind a small C++ shim (the SDK is C++ only).
- **Buddy List**: Discord friends in a "Discord" group (and movable into any
  group), each with the Discord logo next to the name, online/idle/do-not-disturb
  mapped onto AIM's online/away/busy look. A person who is both a Hotline buddy and
  a Discord friend can be merged into one row.
- **IMs**: the same IM window. Messages go through `SendUserMessage` and arrive
  through the message callback; they show up in the friend's Discord DMs.
- **Presence**: Rich Presence shows "In HIM", or "In the <server> chat room" with
  a Join button that opens HIM in that room.

## Limits to know

- Until Discord approves production use, the whole app is capped at **100 DMs
  every 2 hours**. Plenty for testing with a few people.
- Production needs account linking, Rich Presence with Join, invites and the full
  friends list working end to end, plus a demo video. Discord decides; a messenger
  that isn't a game may or may not qualify.
- The SDK only returns about 200 messages from the last 72 hours of a DM, so HIM
  keeps its own history for Discord conversations.
- Discord friends' messages sync to Discord only when at least one side is a full
  Discord account (not a "provisional" one).
