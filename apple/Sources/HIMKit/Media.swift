import Foundation
import HIMCore
import ImageIO
import Observation
import SwiftUI
import UniformTypeIdentifiers

/// Fitting pictures to what a server takes: shrunk, re-encoded, metadata dropped.
public enum Shrink {
    /// A picture for chat, within the server's inline media limits. Animated GIFs stay
    /// animated when they fit; photos become JPEG, pictures with transparency PNG.
    public static func forChat(_ data: Data, limits: MediaLimits) -> Data? {
        guard let img = DecodedImage(data: data) else { return nil }
        let maxSide = Int(min(limits.maxDimension, 1600))
        let w = img.frames[0].width, h = img.frames[0].height
        let fits = w <= maxSide && h <= maxSide && UInt32(w * h) <= limits.maxPixels
        if img.isAnimated {
            if fits, data.count <= Int(limits.maxBytes), UInt32(img.frames.count) <= limits.maxFrames { return data }
            for side in [min(maxSide, 480), 320, 240, 160] {
                if let gif = animatedGIF(img, maxSide: side, maxFrames: Int(min(limits.maxFrames, 60))),
                   gif.count <= Int(limits.maxBytes) { return gif }
            }
        }
        let first = img.frames[0]
        let alpha = hasAlpha(first)
        for side in [maxSide, 1280, 1024, 800, 640] where side <= maxSide {
            let scaled = scale(first, maxSide: side)
            for q in [0.85, 0.7, 0.55] {
                if let out = encode(scaled, as: alpha ? .png : .jpeg, quality: q), out.count <= Int(limits.maxBytes) {
                    return out
                }
                if alpha { break }  // PNG has no quality to turn down
            }
        }
        return nil
    }

    /// A Buddy Icon within the server's limits (keeps animation, up to 32 frames).
    public static func forBuddyIcon(_ data: Data, maxBytes: Int, maxDimension: Int) -> Data? {
        if let i = inspectIcon(data: data), Int(i.width) <= maxDimension, Int(i.height) <= maxDimension,
           i.frames <= 32, data.count <= maxBytes {
            return data  // fine as it is
        }
        guard let img = DecodedImage(data: data) else { return nil }
        if img.isAnimated {
            for side in [48, 40, 32] {
                for frames in [32, 16, 8] {
                    if let gif = animatedGIF(img, maxSide: side, maxFrames: frames, square: true), gif.count <= maxBytes { return gif }
                }
            }
        }
        for side in [48, 40, 32] {
            if let png = encode(square(img.frames[0], side: side), as: .png), png.count <= maxBytes { return png }
        }
        return nil
    }

    // ---------- pieces ----------

    static func hasAlpha(_ img: CGImage) -> Bool {
        switch img.alphaInfo {
        case .none, .noneSkipFirst, .noneSkipLast: false
        default: true
        }
    }

    static func context(_ w: Int, _ h: Int) -> CGContext? {
        CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                  space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    }

    /// At most `maxSide` on the long edge (never enlarged).
    static func scale(_ img: CGImage, maxSide: Int) -> CGImage {
        let long = max(img.width, img.height)
        guard long > maxSide else { return img }
        let f = Double(maxSide) / Double(long)
        let w = max(1, Int(Double(img.width) * f)), h = max(1, Int(Double(img.height) * f))
        guard let ctx = context(w, h) else { return img }
        ctx.interpolationQuality = .high
        ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
        return ctx.makeImage() ?? img
    }

    /// The middle square, at `side` pixels.
    static func square(_ img: CGImage, side: Int) -> CGImage {
        let s = min(img.width, img.height)
        let crop = img.cropping(to: CGRect(x: (img.width - s) / 2, y: (img.height - s) / 2, width: s, height: s)) ?? img
        guard let ctx = context(side, side) else { return crop }
        ctx.interpolationQuality = .high
        ctx.draw(crop, in: CGRect(x: 0, y: 0, width: side, height: side))
        return ctx.makeImage() ?? crop
    }

    static func encode(_ img: CGImage, as type: UTType, quality: Double = 0.85) -> Data? {
        let buf = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(buf, type.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(dest, img, [kCGImageDestinationLossyCompressionQuality: quality] as CFDictionary)
        return CGImageDestinationFinalize(dest) ? buf as Data : nil
    }

    /// An animated GIF: frames evenly thinned to `maxFrames` (timing kept), scaled down.
    static func animatedGIF(_ img: DecodedImage, maxSide: Int, maxFrames: Int, square sq: Bool = false) -> Data? {
        let n = img.frames.count
        let step = max(1, Int((Double(n) / Double(maxFrames)).rounded(.up)))
        var frames: [(CGImage, Double)] = []
        var i = 0
        while i < n {
            let delay = img.delays[i..<min(n, i + step)].reduce(0, +)
            frames.append((sq ? square(img.frames[i], side: maxSide) : scale(img.frames[i], maxSide: maxSide), delay))
            i += step
        }
        let buf = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(buf, UTType.gif.identifier as CFString, frames.count, nil) else { return nil }
        CGImageDestinationSetProperties(dest, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary)
        for (f, d) in frames {
            CGImageDestinationAddImage(dest, f, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: d]] as CFDictionary)
        }
        return CGImageDestinationFinalize(dest) ? buf as Data : nil
    }
}

/// The icon HIM shows in chat rooms (GIF icons): which device you're on, in HIM's colors.
@MainActor
enum DeviceIcon {
    #if os(iOS)
    static let symbol = "iphone"
    /// A classic icon for servers without GIF icons: a phone (the left end of "NOKIA", 5418).
    static let classic: UInt16 = 5418
    #else
    static let symbol = "bubble.left.and.bubble.right.fill"
    /// An iMac.
    static let classic: UInt16 = 2027
    #endif

    static func gif() -> Data? {
        let view = RoundedRectangle(cornerRadius: 7, style: .continuous)
            .fill(LinearGradient(colors: [Brand.light, Brand.deep], startPoint: .topLeading, endPoint: .bottomTrailing))
            .frame(width: 32, height: 32)
            .overlay {
                Image(systemName: symbol)
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(.white)
            }
        let r = ImageRenderer(content: view)
        r.scale = 1
        guard let img = r.cgImage else { return nil }
        let buf = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(buf, UTType.gif.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(dest, img, nil)
        return CGImageDestinationFinalize(dest) ? buf as Data : nil
    }
}

/// HIM's colors.
public enum Brand {
    public static let light = Color(red: 0.33, green: 0.62, blue: 1.0)
    public static let deep = Color(red: 0.36, green: 0.30, blue: 0.95)
    public static let accent = Color(red: 0.30, green: 0.50, blue: 1.0)
}

/// Pictures from chat lines, fetched once and kept for the session.
@MainActor @Observable
final class MediaStore {
    private var images: [String: DecodedImage] = [:]
    private var failed: Set<String> = []
    @ObservationIgnored private var asked: Set<String> = []

    func image(_ m: MediaRef, from session: Session?) -> DecodedImage? {
        if let i = images[m.id] { return i }
        guard let session, !asked.contains(m.id) else { return nil }
        asked.insert(m.id)
        Task {
            if let d = try? await session.downloadMedia(id: m.id), let img = DecodedImage(data: d.data) {
                images[m.id] = img
            } else {
                failed.insert(m.id)
            }
        }
        return nil
    }

    func didFail(_ m: MediaRef) -> Bool { failed.contains(m.id) }
}

/// Hands the core's transfer progress to a closure.
final class ProgressSink: TransferProgress, @unchecked Sendable {
    private let f: (UInt64, UInt64) -> Void
    init(_ f: @escaping (UInt64, UInt64) -> Void) { self.f = f }
    func update(done: UInt64, total: UInt64) { f(done, total) }
}

extension Shrink {
    /// A picture for an IM: big photos come down to 2048 px and a few MB; small pictures
    /// and animations under 8 MB go as they are. Nil for anything that isn't a picture.
    static func forIM(_ data: Data) -> Data? {
        guard let img = DecodedImage(data: data) else { return nil }
        let w = img.frames[0].width, h = img.frames[0].height
        if img.isAnimated { return data.count <= 8 << 20 ? data : animatedGIF(img, maxSide: 640, maxFrames: 120) }
        if max(w, h) <= 2048, data.count <= 4 << 20 { return data }
        let limits = MediaLimits(maxBytes: 4 << 20, maxDimension: 2048, maxPixels: 2048 * 2048, maxFrames: 1)
        return forChat(data, limits: limits)
    }

    static func isJPEG(_ d: Data) -> Bool { d.starts(with: [0xFF, 0xD8]) }
}
