# Getting HIM onto an iPhone and the App Store

Three ways, from quickest to widest. All use the Apple Developer team **JOHN D LEIGHOW
(D36X678376)**. The Xcode project is made from `project.yml`:

```bash
cd apple && scripts/build-core.sh && xcodegen
```

## A. On your own iPhone today (no review)

1. Plug the iPhone into the Mac and tap **Trust** on the phone.
2. Open `apple/HIM.xcodeproj` in Xcode.
3. Click the **HIM** project, then the **HIM** target, then **Signing & Capabilities**. Check
   that **Automatically manage signing** is on and **Team** is JOHN D LEIGHOW. Xcode registers
   the bundle ID `com.tagban.him.modern` on its own.
4. In the toolbar, pick your iPhone as the destination, then **Product → Run** (⌘R).
5. The first time, the iPhone asks for **Developer Mode**: Settings → Privacy & Security →
   Developer Mode → On, then restart the phone and run again.

The app stays on the phone for about a year (the development profile's life); run again from
Xcode to update it.

## B. TestFlight: you and friends, before the App Store

1. **App Store Connect** (appstoreconnect.apple.com) → **Apps** → **+** → **New App**:
   - Platforms: **iOS** (add macOS later, for one listing with "Universal Purchase").
   - Name: something free on the store, e.g. **HIM – Hotline Messenger** (names are unique).
   - Primary language: English (U.S.). Bundle ID: **com.tagban.him.modern**. SKU: `him-modern`.
2. **Upload a build.** In Xcode pick **Any iOS Device (arm64)** as the destination, then
   **Product → Archive**. When the Organizer opens: **Distribute App → App Store Connect →
   Upload**, keeping the defaults. (Or make an App Store Connect API key, under Users and
   Access → Integrations → keys, save the .p8 file on the Mac, and Claude can upload from
   the command line. Never paste the key into chat.)
3. In App Store Connect, the build appears under **TestFlight** after a few minutes of
   processing. Answer **export compliance** when it asks (see below).
4. **TestFlight → Internal Testing → +** to make a group, add yourself (anyone with access to
   your App Store Connect team can be an internal tester), and add the build.
5. On the iPhone, install **TestFlight** from the App Store, open the invitation, **Install**.

Builds last 90 days. Friends without App Store Connect access are **external testers**: add an
external group, fill in "What to Test" and the review contact and demo account (below), and the
build goes through a short Beta App Review first (usually about a day).

## C. The App Store

In App Store Connect, on the app:

1. **Agreements** (Business → Agreements): the account holder accepts the free-apps agreement.
   A free app needs no tax or banking forms.
2. **App Information**: subtitle, category **Social Networking**, content rights.
3. **Age Rating**: answer the questionnaire honestly. People chat freely (IMs and public chat
   rooms on any Hotline server), which raises the rating.
4. **App Privacy**:
   - **Privacy Policy URL**, required. Claude can write the page; it can live in this repo.
   - **Data collection.** The app itself sends nothing anywhere except the Hotline server you
     choose. But VesperNet, the default server, is yours. So declare what it keeps:
     - **User Content: Other User Content** (messages, profiles, Buddy Icons);
     - **Identifiers: User ID** (the screen name).

     Both are linked to the user, used for **App Functionality**, and **not for tracking**.
5. **Version page**:
   - **Screenshots.** 6.9" iPhone (1320 × 2868) is required. HIM is also built for iPad, which
     needs 13" iPad shots; or switch to iPhone only for 1.0 (`TARGETED_DEVICE_FAMILY: "1"` in
     `project.yml`). Claude can take them in the Simulator.
   - **Description, keywords, support URL** (the GitHub page works).
6. **App Review Information**:
   - **Sign-in required → a demo account.** Make a VesperNet account just for Apple (say
     `appreview`) with a buddy or two, and put its name and password here. Reviewers must be
     able to sign on, so VesperNet has to be up during review.
   - **Notes**: "HIM is an instant messenger for the Hotline network. It signs on to VesperNet
     by default (sign-up at vespernet.net/register). Chat Rooms are public chats on Hotline
     servers listed by the Hotline trackers. Ignore and Report are in a room's People list
     (long-press a name); Block and Report are in a buddy's Info."
7. **Build**: pick the TestFlight build, then **Add for Review → Submit**. Review usually takes
   one to three days. Choose to release it yourself or as soon as it's approved.

### Export compliance (asked for each build)

HIM encrypts with standard, published algorithms (TLS; HOPE with HMAC-SHA256 and
ChaCha20-Poly1305, RFC 8439) that it implements itself, not only through Apple's.

- Uses encryption: **Yes**.
- Proprietary or non-standard algorithms: **No**.
- Standard algorithms beyond Apple's own: **Yes**.

That is mass-market encryption, normally exempt. If App Store Connect asks for documents (for
example France's import declaration), stop there and look at it before going on.

### What reviewers look at for a chat app (Guideline 1.2)

- **Block**: a buddy's Info.
- **Report**: a buddy's Info, or a room's People list. It opens the project's issue page; switch
  it to an email address if you'd rather (`Defaults.reportPage`). Answer reports promptly.
- **Ignore**: a room's People list. It hides that person's lines in that room.
- **Contact information**: the support URL.

## The Mac

The same listing can carry the Mac version ("Universal Purchase"). The Mac App Store needs a
sandboxed Mac target in `project.yml`, with network, user-selected files and Downloads
access; it isn't there yet. Until then the Mac version can ship the way the classic app does:
signed with Developer ID, notarized, and downloaded from GitHub (`scripts/bundle-mac.sh` with
`SIGN=…`).

## Things to know

- **Messages arrive only while HIM is open.** iOS stops background connections. The server
  holds IMs for people who are offline, and HIM gets them at the next sign-on. Push
  notifications would need support in the server.
- **Emoji are sent as plain text** (😀 → `:D`, 🦄 → `:unicorn_face:`), because Hotline clients
  can't show them; HIM shows them as emoji again.
