import Foundation
import Security

/// The IM server HIM starts out pointed at, and where to get an account there.
public enum Defaults {
    public static let host = "hotline.vespernet.net"
    public static let port: UInt16 = 5500
    public static let signupPages = ["hotline.vespernet.net": "https://agora.vespernet.net/messenger"]

    /// Friendly names for servers HIM knows.
    public static let serverNames = ["hotline.vespernet.net": "VesperNet"]

    /// Buddies HIM suggests on a server (optional; people can hide them).
    public struct Suggestion: Identifiable {
        public let login: String
        public let name: String
        public let about: String
        public var id: String { login }
    }

    public static let suggestions: [String: [Suggestion]] = [
        "hotline.vespernet.net": [
            Suggestion(login: "john", name: "John", about: "Made HIM. Say hi!"),
            Suggestion(login: "smarterchild", name: "SmarterChild", about: "A chatbot: weather, news, trivia and more"),
        ],
        // Debug builds: the local test server (mock-server), to try it there.
        "127.0.0.1": debugOnly([
            Suggestion(login: "carol", name: "Carol", about: "A test buddy on the local server"),
        ]),
    ]

    private static func debugOnly(_ s: [Suggestion]) -> [Suggestion] {
        #if DEBUG
        s
        #else
        []
        #endif
    }

    /// Where "Report" goes: the project's issue tracker (App Review wants a way to report abuse).
    public static let reportPage = "https://github.com/tagban/him/issues/new"

    public static func serverName(_ host: String) -> String {
        serverNames[host.lowercased()] ?? host
    }

    public static func signupPage(for host: String) -> URL? {
        signupPages[host.lowercased()].flatMap(URL.init(string:))
    }
}

/// Passwords, in the Keychain, one per screen name and server.
enum Keychain {
    private static let service = "com.tagban.him.modern"

    private static func query(_ account: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: account]
    }

    static func get(_ account: String) -> String? {
        var q = query(account)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: AnyObject?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let d = out as? Data else { return nil }
        return String(data: d, encoding: .utf8)
    }

    static func set(_ account: String, _ password: String) {
        let data = Data(password.utf8)
        let status = SecItemUpdate(query(account) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if status == errSecItemNotFound {
            var q = query(account)
            q[kSecValueData as String] = data
            q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
            SecItemAdd(q as CFDictionary, nil)
        }
    }

    static func forget(_ account: String) {
        SecItemDelete(query(account) as CFDictionary)
    }
}

/// The account signed on last, and how.
public struct SavedAccount: Codable, Equatable {
    public var login: String
    public var host: String
    public var port: UInt16
    public var savePassword: Bool
    public var autoSignOn: Bool

    var key: String { "\(login.lowercased())@\(host.lowercased()):\(port)" }
}

/// Small settings kept in UserDefaults.
struct Settings {
    private static let d = UserDefaults.standard

    static var account: SavedAccount? {
        get { d.data(forKey: "account").flatMap { try? JSONDecoder().decode(SavedAccount.self, from: $0) } }
        set { d.set(newValue.flatMap { try? JSONEncoder().encode($0) }, forKey: "account") }
    }

    /// Away messages people have written, newest first.
    static var awayMessages: [String] {
        get { d.stringArray(forKey: "awayMessages") ?? ["I'm away from my computer right now."] }
        set { d.set(Array(newValue.prefix(10)), forKey: "awayMessages") }
    }

    /// The name used in chat rooms.
    static var roomNick: String? {
        get { d.string(forKey: "roomNick") }
        set { d.set(newValue, forKey: "roomNick") }
    }

    /// The classic icon number used in chat rooms, once someone picks one.
    static var roomIcon: UInt16 {
        get { UInt16(d.integer(forKey: "roomIcon")).nonZero ?? 128 }
        set { d.set(Int(newValue), forKey: "roomIcon") }
    }

    static var roomIconSet: Bool { d.integer(forKey: "roomIcon") != 0 }

    /// Accounts that chose "Not now" for the suggested buddies.
    static var suggestionsHidden: Set<String> {
        get { Set(d.stringArray(forKey: "suggestionsHidden") ?? []) }
        set { d.set(Array(newValue), forKey: "suggestionsHidden") }
    }

    /// People hidden in chat rooms, by server ("host:port" → names).
    static func ignored(_ server: String) -> Set<String> {
        Set((d.dictionary(forKey: "ignored")?[server] as? [String]) ?? [])
    }

    static func setIgnored(_ server: String, _ names: Set<String>) {
        var all = d.dictionary(forKey: "ignored") ?? [:]
        all[server] = Array(names).sorted()
        d.set(all, forKey: "ignored")
    }
}

extension UInt16 {
    var nonZero: UInt16? { self == 0 ? nil : self }
}

/// Where HIM keeps files: Application Support/HIM Modern/<name>.
func dataFolder(_ name: String) -> URL {
    let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("HIM Modern", isDirectory: true)
        .appendingPathComponent(name, isDirectory: true)
    try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
    return base
}

/// Startup breadcrumbs, when HIM_TRACE is set: Application Support/HIM Modern/trace.log.
public func trace(_ s: String) {
    guard ProcessInfo.processInfo.environment["HIM_TRACE"] != nil else { return }
    let url = dataFolder("").appendingPathComponent("trace.log")
    let line = "\(Date().timeIntervalSince1970) \(s)\n"
    if let h = try? FileHandle(forWritingTo: url) {
        h.seekToEndOfFile()
        h.write(Data(line.utf8))
        try? h.close()
    } else {
        try? Data(line.utf8).write(to: url)
    }
}
