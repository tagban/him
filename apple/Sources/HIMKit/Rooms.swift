import Foundation
import HIMCore
import Observation

/// One line of a chat room.
public struct RoomLine: Identifiable, Equatable {
    public enum Kind: Equatable { case chat, emote, join, leave, system }
    public let id = UUID()
    public let kind: Kind
    /// Who said it (chat lines).
    public let name: String
    public let text: String
    public var date = Date()
    public let mine: Bool
    /// A picture sent with the line.
    public var media: MediaRef? = nil
    /// From the server's chat history (said before we joined).
    public var earlier = false
}

/// A Hotline server's public chat, joined as a guest.
@MainActor @Observable
public final class Room: Identifiable {
    public enum State: Equatable { case joining, joined, left(String?) }

    public let id = UUID()
    public let host: String
    public let port: UInt16
    public var title: String
    public private(set) var state: State = .joining
    public private(set) var lines: [RoomLine] = []
    public private(set) var users: [UInt16: ChatUser] = [:]
    public var draft = ""
    public var unread = 0
    public let nick: String
    public let icon: UInt16
    /// People's GIF icons, by user ID (servers with GIF icons).
    public private(set) var gifIcons: [UInt16: DecodedImage] = [:]
    /// What the server takes for pictures; nil when it doesn't.
    public private(set) var mediaLimits: MediaLimits?
    public private(set) var sending = false
    /// The server has older chat to scroll back to.
    public private(set) var hasEarlier = false
    public private(set) var loadingEarlier = false
    @ObservationIgnored private var oldestHistoryID: UInt64?
    let media = MediaStore()

    @ObservationIgnored var session: Session?
    @ObservationIgnored weak var app: AppModel?
    @ObservationIgnored private var epoch = 0
    static let maxLines = 1000

    init(host: String, port: UInt16, title: String, nick: String, icon: UInt16, app: AppModel?) {
        self.host = host
        self.port = port
        self.title = title
        self.nick = nick
        self.icon = icon
        self.app = app
        ignored = Settings.ignored("\(host):\(port)")
    }

    /// Everyone here, admins first, then by name.
    public var people: [ChatUser] {
        users.values.sorted { ($0.admin ? 0 : 1, $0.name.lowercased()) < ($1.admin ? 0 : 1, $1.name.lowercased()) }
    }

    /// People you've hidden here: their lines don't show. Kept per server.
    public private(set) var ignored: Set<String> = []

    public func isIgnored(_ name: String) -> Bool { ignored.contains(name) }

    public func setIgnored(_ name: String, _ on: Bool) {
        if on { ignored.insert(name) } else { ignored.remove(name) }
        Settings.setIgnored("\(host):\(port)", ignored)
    }

    /// The lines to show (without anyone you've hidden).
    public var shownLines: [RoomLine] {
        ignored.isEmpty ? lines : lines.filter { !(($0.kind == .chat || $0.kind == .join || $0.kind == .leave) && ignored.contains($0.name)) }
    }

    /// A page to report someone: the issue tracker, with what happened filled in.
    public func reportURL(_ name: String) -> URL? {
        var c = URLComponents(string: Defaults.reportPage)
        let recent = lines.filter { $0.name == name && $0.kind == .chat }.suffix(5).map { "> \($0.text)" }.joined(separator: "\n")
        c?.queryItems = [
            URLQueryItem(name: "title", value: "Report: \(name) in \(title)"),
            URLQueryItem(name: "body", value: "Server: \(host):\(port)\nPerson: \(name)\n\nWhat happened:\n\n\nTheir recent lines:\n\(recent)"),
        ]
        return c?.url
    }

    /// Who said a chat line, by name.
    public func user(named name: String) -> ChatUser? {
        users.values.first { $0.name == name }
    }

    public var canSendPictures: Bool { mediaLimits != nil && state == .joined }

    public func image(for m: MediaRef) -> DecodedImage? { media.image(m, from: session) }
    public func pictureFailed(_ m: MediaRef) -> Bool { media.didFail(m) }

    func join() async {
        epoch += 1
        let mine = epoch
        state = .joining
        let so = SignOn(host: host, port: port, login: "", password: "", nickname: nick, icon: icon,
                        security: .auto, classic: true, media: true, history: true)
        do {
            let pipe = EventPipe()
            let s = try await connect(signOn: so, listener: pipe)
            guard epoch == mine else { s.disconnect(); return }
            session = s
            mediaLimits = s.info().media
            if let n = s.info().serverName?.trimmingCharacters(in: .whitespaces), !n.isEmpty { title = n }
            state = .joined
            push(.system, "", "You're in \(title).")
            Task { [weak self] in
                for await e in pipe.stream {
                    guard let self, self.epoch == mine else { return }
                    self.handle(e)
                }
            }
            await loadUsers()
            await loadEarlier()
            await shareDeviceIcon()
        } catch {
            state = .left(describe(error))
            push(.system, "", "Couldn't join: \(describe(error))")
        }
    }

    func leave() {
        epoch += 1
        session?.disconnect()
        session = nil
        state = .left(nil)
    }

    /// Emoji go as text faces: rooms are full of classic clients that can't show them.
    public func send(_ text: String) {
        let t = emojiToFaces(text: text.trimmingCharacters(in: .whitespacesAndNewlines))
        guard !t.isEmpty, let s = session else { return }
        if t.hasPrefix("/me ") {
            s.sendChat(text: String(t.dropFirst(4)), emote: true)
        } else {
            s.sendChat(text: t, emote: false)
        }
    }

    /// Sends a picture, shrunk to what the server takes. Nil on success, else why not.
    public func sendPicture(_ data: Data, caption: String = "") async -> String? {
        guard let s = session, let limits = mediaLimits else { return "This server doesn't take pictures." }
        guard let fitted = Shrink.forChat(data, limits: limits) else { return "That picture can't be sent here." }
        sending = true
        defer { sending = false }
        do {
            let m = try await s.uploadMedia(data: fitted)
            let c = emojiToFaces(text: caption.trimmingCharacters(in: .whitespacesAndNewlines))
            s.sendChatMedia(text: c.isEmpty ? "[image]" : c, media: m)
            return nil
        } catch {
            return describe(error)
        }
    }

    /// The server's chat history: the latest lines first, then older ones each time.
    public func loadEarlier() async {
        guard let s = session, s.info().chatHistory, !loadingEarlier else { return }
        loadingEarlier = true
        defer { loadingEarlier = false }
        guard let page = try? await s.chatHistory(before: oldestHistoryID, after: nil, limit: 50) else { return }
        let earlier = page.entries.map { e -> RoomLine in
            let kind: RoomLine.Kind = e.server ? .system : (e.emote ? .emote : .chat)
            let text = e.deleted ? "[message removed]" : (e.emote ? "\(e.nick) \(e.text)" : e.text)
            var l = RoomLine(kind: kind, name: e.nick, text: facesToEmoji(text: text), mine: e.nick == nick)
            l.date = Date(timeIntervalSince1970: TimeInterval(e.timestamp))
            l.earlier = true
            return l
        }
        lines.insert(contentsOf: earlier, at: 0)
        oldestHistoryID = page.entries.first?.id ?? oldestHistoryID
        hasEarlier = page.hasMore
    }

    /// Shows which device we're on, where the server has GIF icons; and fetches everyone's.
    private func shareDeviceIcon() async {
        guard let s = session else { return }
        if let gif = DeviceIcon.gif() { try? await s.setGifIcon(gif: gif) }
        guard let list = try? await s.gifIcons() else { return }
        for g in list {
            if let img = DecodedImage(data: g.data) { gifIcons[g.userId] = img }
        }
    }

    private func fetchGifIcon(_ uid: UInt16) {
        guard let s = session else { return }
        Task {
            do {
                let d = try await s.gifIcon(userId: uid)
                gifIcons[uid] = d.flatMap { DecodedImage(data: $0) }
            } catch {}  // keep what we had
        }
    }

    private func loadUsers() async {
        guard let s = session else { return }
        do {
            users = Dictionary(try await s.getUsers().map { ($0.id, $0) }, uniquingKeysWith: { $1 })
        } catch {
            push(.system, "", "The server won't show who's here (\(describe(error))).")
        }
    }

    private func handle(_ e: HimEvent) {
        switch e {
        case .agreement:
            // Accepted for you and never shown, as in the classic app.
            session?.agreeNowait(nickname: nick, icon: icon)
            Task { await loadUsers() }  // some servers only list users after the agreement
        case .chat(nil, let text, let media):
            for raw in text.split(whereSeparator: { $0 == "\r" || $0 == "\n" }) where !raw.trimmingCharacters(in: .whitespaces).isEmpty {
                let (kind, name, body) = Self.parse(String(raw))
                let mine = name == nick || (kind == .emote && body.hasPrefix("\(nick) "))
                push(kind, name, body, mine: mine)
                if !mine, kind == .chat || kind == .emote, app?.target != .room(id) { unread += 1 }
            }
            if let media, !lines.isEmpty {
                lines[lines.count - 1].media = media
            }
        case .gifIconChanged(let uid):
            fetchGifIcon(uid)
        case .userChanged(let u):
            if let old = users[u.id] {
                if !old.name.isEmpty, old.name != u.name { push(.system, "", "\(old.name) is now known as \(u.name).") }
            } else {
                push(.join, u.name, "\(u.name) joined.")
            }
            users[u.id] = u
        case .userLeft(let uid):
            if let u = users.removeValue(forKey: uid) { push(.leave, u.name, "\(u.name) left.") }
            gifIcons[uid] = nil
        case .serverMessage(let text):
            push(.system, "", text)
        case .privateMessage(_, let from, let text, let media):
            push(.system, "", "Private message from \(from): \(text)")
            if let media, !lines.isEmpty { lines[lines.count - 1].media = media }
        case .disconnected(let reason):
            session = nil
            state = .left(reason)
            push(.system, "", "Disconnected: \(reason)")
        default:
            break
        }
    }

    private func push(_ kind: RoomLine.Kind, _ name: String, _ text: String, mine: Bool = false) {
        lines.append(RoomLine(kind: kind, name: name, text: facesToEmoji(text: text), mine: mine))
        if lines.count > Self.maxLines { lines.removeFirst(lines.count - Self.maxLines) }
    }

    /// Classic chat: "    name:  text", or " *** name does something".
    static func parse(_ raw: String) -> (RoomLine.Kind, String, String) {
        let line = raw.drop(while: { $0 == " " })
        if line.hasPrefix("***") {
            return (.emote, "", line.dropFirst(3).trimmingCharacters(in: .whitespaces))
        }
        if let r = line.range(of: ":  ") {
            let name = line[..<r.lowerBound].trimmingCharacters(in: .whitespaces)
            if !name.isEmpty, name.count <= 64 { return (.chat, name, String(line[r.upperBound...])) }
        }
        return (.system, "", line.trimmingCharacters(in: .whitespaces))
    }
}

/// The rooms you're in, and the servers you could join.
@MainActor @Observable
public final class RoomsModel {
    public private(set) var joined: [Room] = []
    public private(set) var servers: [ListedServer] = []
    public private(set) var loadingServers = false
    @ObservationIgnored private var listedAt: Date?
    @ObservationIgnored weak var app: AppModel?

    init() {}

    /// The name you go by in rooms.
    public var nick: String {
        get { Settings.roomNick ?? (app?.isSignedOn == true ? app!.shownName : "") }
        set { Settings.roomNick = newValue.trimmingCharacters(in: .whitespaces) }
    }

    public var icon: UInt16 {
        get { Settings.roomIconSet ? Settings.roomIcon : DeviceIcon.classic }
        set { Settings.roomIcon = newValue }
    }

    public func refreshServers(force: Bool = false) async {
        if !force, let t = listedAt, Date.now.timeIntervalSince(t) < 300, !servers.isEmpty { return }
        loadingServers = true
        servers = await listServers(trackers: defaultTrackers(), timeoutSecs: 12)
        listedAt = .now
        loadingServers = false
    }

    @discardableResult
    public func join(host: String, port: UInt16, title: String) -> Room {
        if let r = joined.first(where: { $0.host == host && $0.port == port }) {
            if case .left = r.state { Task { await r.join() } }
            app?.target = .room(r.id)
            return r
        }
        let n = nick.isEmpty ? "HIM user" : nick
        let r = Room(host: host, port: port, title: title, nick: n, icon: icon, app: app)
        joined.append(r)
        app?.target = .room(r.id)
        Task { await r.join() }
        return r
    }

    public func room(_ id: UUID) -> Room? { joined.first { $0.id == id } }

    public func leave(_ room: Room) {
        room.leave()
        joined.removeAll { $0.id == room.id }
        if app?.target == .room(room.id) { app?.target = nil }
    }

    func leaveAll() {
        joined.forEach { $0.leave() }
        joined = []
    }
}
