// Renders HIM's app icon from the wordmark (Sources/HIMKit/Views/Wordmark.swift):
//   swiftc -parse-as-library -o build/make-icon scripts/make-icon.swift Sources/HIMKit/Views/Wordmark.swift && build/make-icon
// → Resources/AppIcon.png and iOS/Assets.xcassets/AppIcon.appiconset (1024), and the
// iconset for Resources/AppIcon.icns (then `iconutil -c icns build/AppIcon.iconset -o Resources/AppIcon.icns`).
import AppKit
import SwiftUI

/// iOS: the tile edge to edge (the system rounds it).
struct Mark: View {
    var body: some View {
        ZStack {
            LinearGradient(colors: [Color(white: 0.97), Color(white: 0.80)], startPoint: .top, endPoint: .bottom)
            Wordmark().padding(.horizontal, 90)
        }
        .frame(width: 1024, height: 1024)
    }
}

@MainActor func png(_ view: some View, side: CGFloat) -> Data {
    let r = ImageRenderer(content: view.frame(width: side, height: side))
    r.scale = 1
    let rep = NSBitmapImageRep(cgImage: r.cgImage!)
    return rep.representation(using: .png, properties: [:])!
}

@MainActor func run() throws {
    let fm = FileManager.default
    // iOS: one 1024 square (the system rounds it).
    try png(Mark(), side: 1024).write(to: URL(fileURLWithPath: "Resources/AppIcon.png"))
    try png(Mark(), side: 1024).write(to: URL(fileURLWithPath: "iOS/Assets.xcassets/AppIcon.appiconset/AppIcon.png"))
    // Mac: a rounded square on a transparent canvas, at every size iconutil wants.
    let set = URL(fileURLWithPath: "build/AppIcon.iconset")
    try? fm.removeItem(at: set)
    try fm.createDirectory(at: set, withIntermediateDirectories: true)
    // The Mac's shape: an 824 rounded square centered on 1024, scaled down for each size.
    let macMark = AppMark(size: 824).frame(width: 1024, height: 1024)
    let master = set.appendingPathComponent("master.png")
    try png(macMark, side: 1024).write(to: master)
    for (side, names) in [(16, ["16x16"]), (32, ["16x16@2x", "32x32"]), (64, ["32x32@2x"]), (128, ["128x128"]),
                          (256, ["128x128@2x", "256x256"]), (512, ["256x256@2x", "512x512"]), (1024, ["512x512@2x"])] {
        for n in names {
            let p = Process()
            p.executableURL = URL(fileURLWithPath: "/usr/bin/sips")
            p.arguments = ["-z", "\(side)", "\(side)", master.path, "--out", set.appendingPathComponent("icon_\(n).png").path]
            p.standardOutput = FileHandle.nullDevice
            try p.run()
            p.waitUntilExit()
        }
    }
    try fm.removeItem(at: master)
}

@main
struct MakeIcon {
    static func main() {
        MainActor.assumeIsolated { try! run() }
    }
}
