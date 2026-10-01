import ImageIO
import SwiftUI

/// The BadassBuddy collection that comes with HIM (icons/badassbuddy in the repo, used by
/// permission: LICENSE-ICONS.txt), without the ones BadassBuddy marks NSFW.
enum BuddyIconCollection {
    struct Icon: Decodable, Identifiable, Hashable {
        let title: String
        let filename: String
        var id: String { filename }
    }

    static let site = URL(string: "https://www.badassbuddy.com")!

    static var folder: URL? {
        let candidates = [Bundle.main.resourceURL?.appendingPathComponent("badassbuddy")]
        return candidates.compactMap { $0 }.first { FileManager.default.fileExists(atPath: $0.appendingPathComponent("index.json").path) }
    }

    static func load() -> [Icon] {
        guard let f = folder, let d = try? Data(contentsOf: f.appendingPathComponent("index.json")) else { return [] }
        return ((try? JSONDecoder().decode([Icon].self, from: d)) ?? []).filter { !$0.filename.contains("/") }
    }

    static func data(_ icon: Icon) -> Data? {
        folder.flatMap { try? Data(contentsOf: $0.appendingPathComponent(icon.filename)) }
    }
}

/// "Icons courtesy of BadassBuddy.com ♥", linked, wherever the collection shows.
struct BadassBuddyCredit: View {
    var body: some View {
        HStack(spacing: 4) {
            Text("Icons courtesy of")
            Link("BadassBuddy.com", destination: BuddyIconCollection.site).foregroundStyle(Brand.accent)
            Text("♥")
        }
        .font(.footnote)
        .foregroundStyle(.secondary)
    }
}

/// Picks a Buddy Icon from the bundled collection.
struct IconGalleryView: View {
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var icons: [BuddyIconCollection.Icon] = []
    @State private var query = ""
    @State private var chosen: BuddyIconCollection.Icon?
    @State private var error: String?
    @State private var saving = false

    private var shown: [BuddyIconCollection.Icon] {
        query.isEmpty ? icons : icons.filter { $0.title.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                BadassBuddyCredit().padding(.vertical, 8)
                ScrollView {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 58, maximum: 64), spacing: 6)], spacing: 6) {
                        ForEach(shown) { icon in
                            Cell(icon: icon, selected: chosen == icon)
                                .onTapGesture { chosen = icon }
                                .onTapGesture(count: 2) { chosen = icon; use() }
                        }
                    }
                    .padding(8)
                }
                // The icons were drawn for a light gray (BadassBuddy's #e6e6e6), not white.
                .background(Color(white: 0.914))
                HStack {
                    Text(chosen?.title ?? "\(shown.count) icons").lineLimit(1).foregroundStyle(.secondary)
                    Spacer()
                    if let error { Text(error).foregroundStyle(.red).lineLimit(2) }
                }
                .font(.callout)
                .padding(10)
            }
            .searchable(text: $query, prompt: "Find an icon")
            .navigationTitle("BadassBuddy Icons")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Use This Icon", action: use).disabled(chosen == nil || saving)
                }
            }
        }
        .macMinSize(width: 480, height: 520)
        .task { icons = BuddyIconCollection.load() }
    }

    private func use() {
        guard let c = chosen, let d = BuddyIconCollection.data(c) else { return }
        saving = true
        Task {
            error = await app.setBuddyIcon(d)
            saving = false
            if error == nil { dismiss() }
        }
    }

    private struct Cell: View {
        let icon: BuddyIconCollection.Icon
        let selected: Bool
        @State private var image: CGImage?

        var body: some View {
            ZStack {
                if let image {
                    Image(decorative: image, scale: 1).interpolation(.none).resizable().frame(width: 48, height: 48)
                }
            }
            .frame(width: 58, height: 58)
            .background(selected ? Brand.accent.opacity(0.25) : .clear, in: RoundedRectangle(cornerRadius: 6))
            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(selected ? Brand.accent : .clear, lineWidth: 2))
            .help(icon.title)
            .task(id: icon.filename) {
                // The first frame is enough for picking; animations play once it's your icon.
                guard let d = BuddyIconCollection.data(icon),
                      let src = CGImageSourceCreateWithData(d as CFData, nil) else { return }
                image = CGImageSourceCreateImageAtIndex(src, 0, nil)
            }
        }
    }
}

extension View {
    /// A sheet's smallest size on the Mac; iPhone sheets take the screen they're given.
    @ViewBuilder func macMinSize(width: CGFloat, height: CGFloat) -> some View {
        #if os(macOS)
        frame(minWidth: width, minHeight: height)
        #else
        self
        #endif
    }
}
