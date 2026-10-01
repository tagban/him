import Foundation

/// IM conversations kept on this device, one file per buddy under
/// Application Support/HIM Modern/history/<screen name@server>/, newest 2,000 lines.
@MainActor
final class IMHistory {
    private let folder: URL
    private var pending: [String: Task<Void, Never>] = [:]
    static let keep = 2000

    init(account: SavedAccount) {
        let key = account.key.replacingOccurrences(of: "/", with: "_")
        folder = dataFolder("history").appendingPathComponent(key, isDirectory: true)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    }

    private func file(_ login: String) -> URL {
        let safe = login.lowercased().map { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_" || $0 == "." ? $0 : "_" }
        return folder.appendingPathComponent(String(safe) + ".json")
    }

    /// Everyone we've talked to, with what was said.
    func loadAll() -> [(login: String, lines: [Line])] {
        let files = (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
        return files.filter { $0.pathExtension == "json" }.compactMap { url in
            guard let d = try? Data(contentsOf: url), let saved = try? JSONDecoder().decode(Saved.self, from: d) else { return nil }
            // Anything mid-send when HIM closed didn't make it.
            let lines = saved.lines.map { l -> Line in
                var l = l
                if l.status == .sending { l.status = .failed("HIM closed before it was sent") }
                // An offer lives only as long as both sessions: one left over can't be taken now.
                switch l.file?.state {
                case .offered, .incoming, .starting, .moving: l.file?.state = .failed("it ended when HIM signed off")
                default: break
                }
                return l
            }
            return (saved.login, lines)
        }
    }

    /// Writes a conversation soon (changes in a burst are written once).
    func save(_ c: Conversation) {
        pending[c.login]?.cancel()
        let login = c.login
        pending[login] = Task { [weak self, weak c] in
            try? await Task.sleep(nanoseconds: 400_000_000)
            guard !Task.isCancelled, let self, let c else { return }
            let saved = Saved(login: login, lines: Array(c.lines.suffix(Self.keep)))
            if let d = try? JSONEncoder().encode(saved) { try? d.write(to: self.file(login), options: .atomic) }
            self.pending[login] = nil
        }
    }

    func forget(_ login: String) {
        try? FileManager.default.removeItem(at: file(login))
    }

    private struct Saved: Codable {
        let login: String
        let lines: [Line]
    }
}
