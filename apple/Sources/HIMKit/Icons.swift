import Foundation
import HIMCore
import ImageIO
import Observation
import SwiftUI

/// A picture ready to draw: one frame, or an animation.
public struct DecodedImage {
    public let frames: [CGImage]
    /// Seconds each frame shows.
    public let delays: [Double]
    public var duration: Double { delays.reduce(0, +) }
    public var isAnimated: Bool { frames.count > 1 }
    /// Stored sideways and turned upright here.
    var turned = false

    public init?(data: Data) {
        guard let src = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let n = min(CGImageSourceGetCount(src), 64)
        // A photo from a phone is often stored sideways with an orientation tag: turn it upright,
        // since a shrunk copy loses the tag.
        if n == 1, let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any],
           let o = props[kCGImagePropertyOrientation] as? UInt32, o != 1,
           let w = props[kCGImagePropertyPixelWidth] as? Int, let h = props[kCGImagePropertyPixelHeight] as? Int,
           let upright = CGImageSourceCreateThumbnailAtIndex(src, 0, [
               kCGImageSourceCreateThumbnailFromImageAlways: true,
               kCGImageSourceCreateThumbnailWithTransform: true,
               kCGImageSourceThumbnailMaxPixelSize: max(w, h),
           ] as CFDictionary) {
            frames = [upright]
            delays = [0.1]
            turned = true
            return
        }
        var frames: [CGImage] = [], delays: [Double] = []
        for i in 0..<n {
            guard let img = CGImageSourceCreateImageAtIndex(src, i, nil) else { continue }
            frames.append(img)
            let props = CGImageSourceCopyPropertiesAtIndex(src, i, nil) as? [CFString: Any]
            let gif = props?[kCGImagePropertyGIFDictionary] as? [CFString: Any]
            let png = props?[kCGImagePropertyPNGDictionary] as? [CFString: Any]
            let d = (gif?[kCGImagePropertyGIFUnclampedDelayTime] as? Double)
                ?? (gif?[kCGImagePropertyGIFDelayTime] as? Double)
                ?? (png?[kCGImagePropertyAPNGUnclampedDelayTime] as? Double)
                ?? 0.1
            delays.append(d < 0.02 ? 0.1 : d)  // as browsers do with "as fast as possible"
        }
        guard !frames.isEmpty else { return nil }
        self.frames = frames
        self.delays = delays
    }

    init(still: CGImage) {
        frames = [still]
        delays = [1]
    }

    func frame(at t: Double) -> CGImage {
        guard isAnimated, duration > 0 else { return frames[0] }
        var x = t.truncatingRemainder(dividingBy: duration)
        for (i, d) in delays.enumerated() {
            if x < d { return frames[i] }
            x -= d
        }
        return frames[frames.count - 1]
    }
}

/// Draws a `DecodedImage`, animating it when it has frames.
public struct IconImage: View {
    let image: DecodedImage
    var pixelated = false

    public var body: some View {
        if image.isAnimated {
            TimelineView(.periodic(from: .now, by: max(0.04, image.delays.min() ?? 0.1))) { ctx in
                still(image.frame(at: ctx.date.timeIntervalSinceReferenceDate))
            }
        } else {
            still(image.frames[0])
        }
    }

    private func still(_ img: CGImage) -> some View {
        Image(decorative: img, scale: 1)
            .resizable()
            .interpolation(pixelated ? .none : .high)
            .aspectRatio(contentMode: .fit)
    }
}

/// Buddy Icons, by hash: memory, then disk, then the server.
@MainActor @Observable
public final class BuddyIcons {
    private var decoded: [String: DecodedImage] = [:]
    @ObservationIgnored private var raw: [String: Data] = [:]
    @ObservationIgnored private var inFlight: Set<String> = []
    @ObservationIgnored private let folder = dataFolder("icons")

    public init() {}

    public func image(_ hash: String?) -> DecodedImage? {
        guard let hash else { return nil }
        if let d = decoded[hash] { return d }
        if let data = try? Data(contentsOf: folder.appendingPathComponent(hash)), let img = DecodedImage(data: data) {
            raw[hash] = data
            // Not stored during a view update; the next lookup finds it in memory.
            Task { @MainActor in self.decoded[hash] = img }
            return img
        }
        return nil
    }

    public func data(_ hash: String) -> Data? {
        raw[hash] ?? (try? Data(contentsOf: folder.appendingPathComponent(hash)))
    }

    /// Fetches `login`'s icon unless we have `hash` already.
    func fetch(login: String, hash: String?, from session: Session?) {
        guard let hash, let session, image(hash) == nil, !inFlight.contains(hash) else { return }
        inFlight.insert(hash)
        Task {
            defer { inFlight.remove(hash) }
            guard let icon = try? await session.getBuddyIcon(login: login) else { return }
            store(icon.data, as: hash)
            if icon.hash != hash { store(icon.data, as: icon.hash) }
        }
    }

    func store(_ data: Data, as hash: String) {
        guard let img = DecodedImage(data: data) else { return }
        try? data.write(to: folder.appendingPathComponent(hash))
        raw[hash] = data
        decoded[hash] = img
    }
}

/// Classic Hotline icons (by number) for chat rooms. The wide ones are banners whose
/// icon is the left end; that part is used, centered in a square (as the Discord bridge does).
@MainActor @Observable
public final class ClassicIcons {
    private var images: [UInt16: DecodedImage] = [:]
    @ObservationIgnored private var asked: Set<UInt16> = []
    @ObservationIgnored private let folder = dataFolder("classic-icons")
    static let base = "https://hlwiki.com/ik0ns/"

    public init() {}

    public func image(_ n: UInt16) -> DecodedImage? {
        if let i = images[n] { return i }
        load(n)
        return nil
    }

    private func load(_ n: UInt16) {
        guard !asked.contains(n) else { return }
        asked.insert(n)
        let file = folder.appendingPathComponent("\(n).png")
        Task {
            var data = try? Data(contentsOf: file)
            if data == nil, let url = URL(string: "\(Self.base)\(n).png"),
               let (d, r) = try? await URLSession.shared.data(from: url),
               (r as? HTTPURLResponse)?.statusCode == 200 {
                try? d.write(to: file)
                data = d
            }
            guard let data, let full = DecodedImage(data: data)?.frames.first, let sq = Self.square(full) else { return }
            images[n] = DecodedImage(still: sq)
        }
    }

    /// The icon end of a banner (about 1.45 × its height), centered in a square.
    static func square(_ img: CGImage) -> CGImage? {
        var src = img
        let w = img.width, h = img.height
        if w > h * 2, let c = img.cropping(to: CGRect(x: 0, y: 0, width: min(w, Int((Double(h) * 1.45).rounded())), height: h)) {
            src = c
        }
        let side = max(src.width, src.height)
        guard let ctx = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        ctx.interpolationQuality = .none
        ctx.draw(src, in: CGRect(x: (side - src.width) / 2, y: (side - src.height) / 2, width: src.width, height: src.height))
        return ctx.makeImage()
    }
}

// ---------- avatars ----------

/// A soft color for someone without a picture, the same every time for a name.
func nameColor(_ name: String) -> Color {
    let palette: [Color] = [.blue, .indigo, .purple, .pink, .orange, .teal, .green, .cyan, .mint, .red]
    let n = name.lowercased().unicodeScalars.reduce(UInt32(5381)) { ($0 &* 33) &+ $1.value }
    return palette[Int(n % UInt32(palette.count))]
}

func initials(_ name: String) -> String {
    let words = name.split(whereSeparator: { $0 == " " || $0 == "_" || $0 == "." }).prefix(2)
    let s = words.compactMap(\.first).map(String.init).joined()
    return (s.isEmpty ? String(name.prefix(1)) : s).uppercased()
}

/// Someone's Buddy Icon, or their initials on a color.
public struct Avatar: View {
    @Environment(BuddyIcons.self) private var icons
    let name: String
    let hash: String?
    var size: CGFloat = 36
    var presence: Presence? = nil

    public init(name: String, hash: String?, size: CGFloat = 36, presence: Presence? = nil) {
        self.name = name
        self.hash = hash
        self.size = size
        self.presence = presence
    }

    public var body: some View {
        let shape = RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
        Group {
            if let img = icons.image(hash) {
                IconImage(image: img)
                    .frame(width: size, height: size)
                    .background(Color(white: 0.914))  // #e9e9e9, as in the classic app
            } else {
                Text(initials(name))
                    .font(.system(size: size * 0.38, weight: .semibold, design: .rounded))
                    .foregroundStyle(.white)
                    .frame(width: size, height: size)
                    .background(nameColor(name).gradient)
            }
        }
        .clipShape(shape)
        .overlay(shape.strokeBorder(.primary.opacity(0.08)))
        .overlay(alignment: .bottomTrailing) {
            if let presence { PresenceDot(presence: presence, size: max(9, size * 0.3)).offset(x: 2, y: 2) }
        }
    }
}

/// A chat room member's icon: their GIF icon when the server has them, else their
/// classic icon, squared (the icon end of a banner).
public struct ClassicAvatar: View {
    @Environment(ClassicIcons.self) private var icons
    let icon: UInt16
    let name: String
    var size: CGFloat = 28
    var gif: DecodedImage? = nil

    public var body: some View {
        let shape = RoundedRectangle(cornerRadius: size * 0.24, style: .continuous)
        Group {
            if let gif {
                IconImage(image: gif, pixelated: gif.frames[0].width <= Int(size))
                    .frame(width: size, height: size)
                    .background(Color(white: 0.914))
            } else if let img = icons.image(icon) {
                IconImage(image: img, pixelated: true)
                    .padding(size * 0.08)
                    .frame(width: size, height: size)
                    .background(Color(white: 0.914))
            } else {
                Text(initials(name))
                    .font(.system(size: size * 0.4, weight: .semibold, design: .rounded))
                    .foregroundStyle(.white)
                    .frame(width: size, height: size)
                    .background(nameColor(name).gradient)
            }
        }
        .clipShape(shape)
        .overlay(shape.strokeBorder(.primary.opacity(0.08)))
    }
}

public struct PresenceDot: View {
    let presence: Presence
    var size: CGFloat = 10

    public var body: some View {
        Circle()
            .fill(color)
            .frame(width: size, height: size)
            .overlay(Circle().strokeBorder(.background, lineWidth: size * 0.2))
    }

    var color: Color {
        switch presence {
        case .online: .green
        case .away: .yellow
        case .busy: .red
        case .invisible, .offline: .gray
        }
    }
}
