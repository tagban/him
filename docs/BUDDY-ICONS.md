# Buddy Icons

The protocol is fogWraith's Buddy Icons extension,
[Capabilities-Buddy-Icons.md](https://github.com/fogWraith/Hotline/blob/main/Docs/Protocol/Capabilities-Buddy-Icons.md)
(proposed in [fogWraith/Hotline#2](https://github.com/fogWraith/Hotline/pull/2)).
HIM implements the client side, and the test server implements the server side.

## How HIM does it

- **Support:** buddy icons exist only when the login reply carries
  `DATA_MAX_ICON_BYTES` (`0x0623`, capped at 65535). Without it HIM sends no 827 or
  828, and your icon stays local. `DATA_MAX_ICON_DIMENSION` (`0x0624`) defaults to
  64 when missing.
- **Choosing one:**
  - A picture that is already at most 64 x 64 pixels, 32 frames and 16 KB is kept
    byte for byte, so classic GIFs stay exactly as drawn.
  - Anything bigger is scaled to fit 48 x 48 and trimmed to 32 frames. If it is
    still over 16 KB, every other frame is dropped.
  - This happens in `src-tauri/src/icons.rs` (`prepare`).
- **Uploading (827):** happens when you choose or clear an icon while signed on.
- **Syncing at sign-on:** HIM asks Get User Info (825) about your own Login. For each
  account it remembers two hashes from the last sync (`iconSynced` in settings): its
  own file's, and the one the server reported for what it stored. The two differ
  because Janus re-encodes icons to strip metadata, so each is compared only with
  its own kind.
  - If your file changed since the last sync, HIM uploads it.
  - Otherwise, if the server's hash changed (someone changed it from another
    computer), HIM downloads the icon with 828 and uses it.
- **Your other sessions:** when a session changes the icon, the server sends an 827
  notification with the new hash, or none if it was cleared. HIM downloads the new
  icon, or clears its own.
- **Buddies' icons:**
  - `DATA_BUDDY_ICON_HASH` comes with every 801 and every 809. If it's missing, the
    buddy has no icon.
  - HIM fetches an icon with 828 only when a window needs it (an IM window or Buddy
    Info) and the hash isn't cached yet.
  - The picture must match its hash and have a sane size, then it's cached on disk
    under the hash.
  - If a fetch fails (for example `RateLimited`), the IM window tries again in 30
    seconds.
- **Where they show:** in an IM window, the buddy's icon is in the lower-left and
  yours in the lower-right. Buddy Info shows it top right. Chat rooms show no icons.
