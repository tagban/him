// swift run HIMCheck <port>: drives AppModel against `mock-server <port>` and checks what
// people would see. Exits non-zero on the first failure.
import CoreGraphics
import Foundation
import HIMCore
import HIMKit
import ImageIO
import UniformTypeIdentifiers

@MainActor
func check(_ ok: Bool, _ what: String) {
    print(ok ? "ok   \(what)" : "FAIL \(what)")
    if !ok { exit(1) }
}

/// Waits up to `secs` for `cond`.
@MainActor
func until(_ secs: Double = 8, _ cond: () -> Bool) async -> Bool {
    let end = Date().addingTimeInterval(secs)
    while Date() < end {
        if cond() { return true }
        try? await Task.sleep(nanoseconds: 50_000_000)
    }
    return cond()
}

func picture(w: Int, h: Int, frames: Int = 1, type: UTType = .png) -> Data {
    let buf = NSMutableData()
    let dest = CGImageDestinationCreateWithData(buf, type.identifier as CFString, frames, nil)!
    for f in 0..<frames {
        let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
        for y in stride(from: 0, to: h, by: 8) {  // noise-ish stripes, so it doesn't compress to nothing
            ctx.setFillColor(red: CGFloat((y * 7 + f * 40) % 255) / 255, green: CGFloat((y * 3) % 255) / 255, blue: 0.5, alpha: 1)
            ctx.fill(CGRect(x: 0, y: y, width: w, height: 8))
        }
        CGImageDestinationAddImage(dest, ctx.makeImage()!, frames > 1 ? [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: 0.1]] as CFDictionary : nil)
    }
    CGImageDestinationFinalize(dest)
    return buf as Data
}

@main
struct Check {
    @MainActor
    static func main() async {
        let port = UInt16(CommandLine.arguments.dropFirst().first ?? "15500") ?? 15500
        // Start clean, and leave nothing behind: the room name and the test account's history.
        let testHistory = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("HIM Modern/history/alice@127.0.0.1:\(port)")
        func cleanUp() {
            UserDefaults.standard.removeObject(forKey: "roomNick")
            try? FileManager.default.removeItem(at: testHistory)
        }
        cleanUp()
        atexit_b { cleanUp() }

        // ---- shrinking ----
        let limits = MediaLimits(maxBytes: 256 * 1024, maxDimension: 2048, maxPixels: 2048 * 2048, maxFrames: 150)
        let big = picture(w: 3000, h: 2000, type: .jpeg)
        let fitted = Shrink.forChat(big, limits: limits)
        let info = fitted.flatMap { inspectIcon(data: $0) }
        check(fitted != nil && fitted!.count <= 256 * 1024 && (info?.width ?? 9999) <= 1600, "a 3000×2000 photo shrinks to fit chat (\(info.map { "\($0.width)×\($0.height)" } ?? "?"), \(fitted?.count ?? 0) bytes)")
        let anim = picture(w: 100, h: 100, frames: 40, type: .gif)
        let icon = Shrink.forBuddyIcon(anim, maxBytes: 16384, maxDimension: 64)
        let ii = icon.flatMap { inspectIcon(data: $0) }
        check(ii != nil && ii!.width <= 48 && ii!.frames > 1 && ii!.frames <= 32 && icon!.count <= 16384,
              "a 40-frame 100px GIF becomes an animated Buddy Icon (\(ii.map { "\($0.width)px, \($0.frames) frames" } ?? "?"), \(icon?.count ?? 0) bytes)")
        check(emojiToFaces(text: "hi 😀 👍") == "hi :D (Y)" && facesToEmoji(text: "hey :)") == "hey 🙂", "emoji ↔ faces")

        // ---- IM ----
        let app = AppModel()
        await app.signOn(login: "alice", password: "hotline", host: "127.0.0.1", port: port, savePassword: false, autoSignOn: false)
        check(app.phase == .online, "alice signs on (\(app.signOnError ?? "no error"))")
        check(Set(app.buddies.keys).isSuperset(of: ["bob", "carol", "hotbot"]), "her buddies load: \(app.buddies.keys.sorted())")
        let hotbot = app.buddies["hotbot"]
        check(await until { app.icons.image(hotbot?.iconHash) != nil }, "HotBot's Buddy Icon arrives")
        app.open("hotbot")
        app.send("hello there :) 😀", to: "hotbot")
        let c = app.conversation("hotbot")
        check(await until { c.lines.contains { $0.direction == .incoming } }, "HotBot answers")
        print("     HotBot: \(c.lines.first { $0.direction == .incoming }!.text)")
        check(c.lines.first!.text == "hello there 🙂 😀", "our line shows faces as emoji: \(c.lines.first!.text)")
        check(await until { [.sent, .delivered, .read].contains(c.lines.first!.status) }, "our line is sent (\(c.lines.first!.status))")

        // ---- files ----
        let bob = AppModel()
        await bob.signOn(login: "bob", password: "hotline", host: "127.0.0.1", port: port, savePassword: false, autoSignOn: false)
        check(bob.phase == .online, "bob signs on too")
        check(await until { app.buddies["bob"]?.presence == .online }, "alice sees bob online")
        let pic = picture(w: 640, h: 480)
        let picURL = FileManager.default.temporaryDirectory.appendingPathComponent("him-check.png")
        try? pic.write(to: picURL)
        let offerErr = await app.sendFile(picURL, to: "bob")
        check(offerErr == nil, "alice offers bob a picture (\(offerErr ?? "ok"))")
        let bc = bob.conversation("alice")
        check(await until { bc.lines.contains { $0.file?.state == .incoming } }, "bob is asked to accept it")
        let guid = bc.lines.last { $0.file != nil }!.file!.guid
        bob.acceptFile(guid)
        check(await until(15) { bc.lines.last { $0.file != nil }?.file?.state == .done }, "it arrives (\(String(describing: bc.lines.last { $0.file != nil }?.file?.state)))")
        let saved = bc.lines.last { $0.file != nil }!.file!.path!
        check((try? Data(contentsOf: URL(fileURLWithPath: saved))) == pic, "byte for byte, as \((saved as NSString).lastPathComponent)")
        check(await until { app.conversation("bob").lines.last { $0.file != nil }?.file?.state == .done }, "alice sees it sent")
        bob.signOff()

        // ---- chat rooms ----
        app.rooms.nick = "Alice"
        let room = app.rooms.join(host: "127.0.0.1", port: port, title: "Test")
        check(await until { room.state == .joined }, "alice joins the room")
        check(room.mediaLimits != nil, "the room takes pictures")
        let other = AppModel()
        other.roomsOnly()
        other.rooms.nick = "Visitor"
        let room2 = other.rooms.join(host: "127.0.0.1", port: port, title: "Test")
        check(await until { room2.state == .joined }, "a visitor joins too")
        check(await until { room2.gifIcons.count >= 1 }, "the visitor sees GIF icons (\(room2.gifIcons.count))")
        room.send("hi room 😀")
        check(await until { room2.lines.contains { $0.text == "hi room 😀" } }, "emoji cross a room as faces and come back as emoji")
        let err = await room.sendPicture(picture(w: 900, h: 600), caption: "look 👀")
        check(err == nil, "alice sends a picture (\(err ?? "ok"))")
        check(await until { room2.lines.contains { $0.media != nil } }, "the visitor gets the picture's line")
        let line = room2.lines.last { $0.media != nil }!
        print("     line: \(line.name): \(line.text) [\(line.media!.mime), \(line.media!.bytes) bytes]")
        check(line.name == "Alice", "from Alice")
        check(await until { room2.image(for: line.media!) != nil }, "and the picture itself")
        let img = room2.image(for: line.media!)!
        check(img.frames[0].width == 900, "at full size (\(img.frames[0].width)×\(img.frames[0].height))")
        // ---- chat history ----
        let late = AppModel()
        late.roomsOnly()
        late.rooms.nick = "Latecomer"
        let room3 = late.rooms.join(host: "127.0.0.1", port: port, title: "Test")
        check(await until { room3.lines.contains { $0.earlier && $0.text == "hi room 😀" } },
              "someone joining later sees what was said before (\(room3.lines.filter(\.earlier).count) earlier lines)")
        try? await Task.sleep(nanoseconds: 800_000_000)  // let the IM history be written
        let said = app.conversation("hotbot").lines.count
        app.signOff()
        let again = AppModel()
        await again.signOn(login: "alice", password: "hotline", host: "127.0.0.1", port: port, savePassword: false, autoSignOn: false)
        check(again.conversations["hotbot"]?.lines.count == said, "alice's conversation with HotBot is still there after signing on again (\(said) lines)")
        again.signOff()
        print("all good")
        exit(0)
    }
}
