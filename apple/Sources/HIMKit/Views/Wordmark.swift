import SwiftUI

/// "HIM" in the Hotline H's style (art/him-wordmark.svg): beveled red strokes that end in
/// points, a darker layer showing on the right of each stroke and under the bars.
/// Drawn in a 32 × 16 box; keep that aspect ratio.
public struct Wordmark: View {
    public init() {}

    static let dark = [
        "M0 2L3 0V13H0Z M7 2H10V12L7 14Z M3 6H7V9H3Z",                                     // H
        "M12 2L15 0V12L12 14Z",                                                              // I
        "M17 1L20 0V12L17 14Z M27 1L30 0V12L27 14Z M20 0L23.5 6L27 1V4.5L23.5 10L20 4Z",    // M
    ]
    static let face = [
        "M0 2L2 0.667V13H0Z M7 2H9V12.667L7 14Z M0 6H9V8H0Z",
        "M12 2L14 0.667V12.667L12 14Z",
        "M17 1L19 0.333V12.667L17 14Z M27 1L29 0.333V12.667L27 14Z M19 0.333L23.5 6.2L27 1V3.4L23.5 8.6L19 2.9Z",
    ]
    static let shine = "M0.35 2.2L1.65 1.35 M12.35 2.2L13.65 1.35 M17.35 1.2L18.65 0.75 M27.35 1.2L28.65 0.75"

    public var body: some View {
        GeometryReader { g in
            let s = min(g.size.width / 32, g.size.height / 16)
            let t = CGAffineTransform(translationX: (g.size.width - 32 * s) / 2 + s, y: (g.size.height - 16 * s) / 2 + s).scaledBy(x: s, y: s)
            ZStack {
                ForEach(Self.dark, id: \.self) { d in
                    let p = SVGPath.parse(d).applying(t)
                    p.fill(Color(red: 0.54, green: 0, blue: 0))
                    p.stroke(Color(red: 0.23, green: 0, blue: 0), style: StrokeStyle(lineWidth: 0.35 * s, lineJoin: .round))
                }
                ForEach(Self.face, id: \.self) { d in
                    SVGPath.parse(d).applying(t).fill(LinearGradient(
                        colors: [Color(red: 1, green: 0.165, blue: 0.118), Color(red: 0.847, green: 0, blue: 0)],
                        startPoint: UnitPoint(x: 0.5, y: 0.06), endPoint: UnitPoint(x: 0.5, y: 0.94)))
                }
                SVGPath.parse(Self.shine).applying(t)
                    .stroke(Color(red: 1, green: 0.6, blue: 0.56), style: StrokeStyle(lineWidth: 0.3 * s, lineCap: .round))
            }
        }
        .aspectRatio(2, contentMode: .fit)
        .accessibilityLabel("HIM")
    }
}

/// The few SVG path commands the wordmark uses: absolute M, L, H, V and Z.
struct SVGPath: Shape {
    let d: String
    init(_ d: String) { self.d = d }

    func path(in rect: CGRect) -> Path { Self.parse(d) }

    static func parse(_ d: String) -> Path {
        var p = Path()
        var cmd: Character = "M"
        var nums: [CGFloat] = []
        var cur = CGPoint.zero
        func flush() {
            switch cmd {
            case "M", "L":
                var i = 0
                while i + 1 < nums.count {
                    cur = CGPoint(x: nums[i], y: nums[i + 1])
                    if cmd == "M" && i == 0 { p.move(to: cur) } else { p.addLine(to: cur) }
                    i += 2
                }
            case "H": for x in nums { cur.x = x; p.addLine(to: cur) }
            case "V": for y in nums { cur.y = y; p.addLine(to: cur) }
            case "Z": p.closeSubpath()
            default: break
            }
            nums = []
        }
        var number = ""
        for ch in d + " " {
            if ch.isLetter {
                if !number.isEmpty { nums.append(CGFloat(Double(number) ?? 0)); number = "" }
                flush()
                cmd = ch
                if ch == "Z" { flush() }
            } else if ch == " " || ch == "," {
                if !number.isEmpty { nums.append(CGFloat(Double(number) ?? 0)); number = "" }
            } else {
                number.append(ch)
            }
        }
        flush()
        return p
    }
}

/// The app's mark: the wordmark on a light, Hotline-gray tile.
public struct AppMark: View {
    var size: CGFloat = 72

    public init(size: CGFloat = 72) { self.size = size }

    public var body: some View {
        RoundedRectangle(cornerRadius: size * 0.225, style: .continuous)
            .fill(LinearGradient(colors: [Color(white: 0.97), Color(white: 0.80)], startPoint: .top, endPoint: .bottom))
            .overlay(RoundedRectangle(cornerRadius: size * 0.225, style: .continuous).strokeBorder(Color(white: 0.55), lineWidth: max(1, size * 0.012)))
            .overlay(RoundedRectangle(cornerRadius: size * 0.2, style: .continuous).strokeBorder(.white.opacity(0.8), lineWidth: max(1, size * 0.012)).padding(size * 0.02))
            .overlay(Wordmark().padding(.horizontal, size * 0.08))
            .frame(width: size, height: size)
            .shadow(color: .black.opacity(0.18), radius: size * 0.1, y: size * 0.04)
    }
}
