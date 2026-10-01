import Foundation
import HIMCore
import ImageIO
import Observation
import UniformTypeIdentifiers

/// One line of an IM conversation.
public struct Line: Identifiable, Equatable, Codable {
    public enum Direction: Equatable, Codable { case incoming, outgoing, system }
    public enum Status: Equatable, Codable {
        case sending, sent, queued, delivered, read
        case failed(String)
        /// Incoming, not yet seen.
        case unread
        case seen
    }

    public let id: String
    public let direction: Direction
    public let body: String
    public let date: Date
    public var status: Status
    /// An away message sent automatically.
    public var isAuto: Bool { body.hasPrefix(Self.autoPrefix) }
    /// What to show: without the auto-reply prefix, and with text faces as emoji.
    public var text: String { facesToEmoji(text: isAuto ? String(body.dropFirst(Self.autoPrefix.count)) : body) }

    static let autoPrefix = "Auto-response: "
}

@MainActor @Observable
public final class Conversation: Identifiable {
    public let login: String
    public var lines: [Line] = []
    public var typing = false
    public var draft = ""
    public nonisolated var id: String { login }

    init(login: String) { self.login = login }

    public var unread: Int { lines.lazy.filter { $0.status == .unread }.count }
    public var lastDate: Date { lines.last?.date ?? .distantPast }
}

public struct FriendRequest: Identifiable, Equatable {
    public let login: String
    public let note: String?
    public nonisolated var id: String { login }
}

/// What the main area shows.
public enum Target: Hashable {
    case im(String)
    case room(UUID)
}

public enum Phase: Equatable {
    case signedOff
    case signingOn(String)
    case online
    /// Lost the connection; trying again.
    case reconnecting(String)
    /// Rooms only, no IM account.
    case guest
}

@MainActor @Observable
public final class AppModel {
    public var phase: Phase = .signedOff
    public var signOnError: String?
    public private(set) var account: SavedAccount? = Settings.account
    public private(set) var serverInfo: ServerInfo?
    public private(set) var buddies: [String: Buddy] = [:]
    public private(set) var conversations: [String: Conversation] = [:]
    public private(set) var requests: [FriendRequest] = []
    public private(set) var presence: Presence = .online
    public private(set) var status = ""
    /// The name buddies see us as, when it isn't just the screen name.
    public private(set) var myName: String?
    public private(set) var myIconHash: String?
    /// A passing message from the server or about the connection.
    public var notice: String?
    public var target: Target?

    public let icons = BuddyIcons()
    public let classicIcons = ClassicIcons()
    public let rooms = RoomsModel()

    @ObservationIgnored var session: Session?
    @ObservationIgnored private var signOn: SignOn?
    @ObservationIgnored private var epoch = 0
    @ObservationIgnored private var autoAnswered: Set<String> = []
    @ObservationIgnored private var typingSent: [String: Date] = [:]
    @ObservationIgnored private var history: IMHistory?

    public init() {
        rooms.app = self
    }

    public var login: String { account?.login ?? "" }
    public var shownName: String { myName ?? login }
    public var isSignedOn: Bool {
        switch phase {
        case .online, .reconnecting: true
        default: false
        }
    }

    // ---------- signing on and off ----------

    public func savedPassword(for a: SavedAccount) -> String? {
        a.savePassword ? Keychain.get(a.key) : nil
    }

    public func signOn(login: String, password: String, host: String, port: UInt16,
                       savePassword: Bool, autoSignOn: Bool) async {
        let login = login.trimmingCharacters(in: .whitespaces)
        let host = host.trimmingCharacters(in: .whitespaces)
        guard !login.isEmpty else { signOnError = "Enter your screen name."; return }
        guard !host.isEmpty else { signOnError = "Choose a server."; return }
        let acct = SavedAccount(login: login, host: host, port: port, savePassword: savePassword, autoSignOn: autoSignOn)
        let pw = password.isEmpty ? (Keychain.get(acct.key) ?? "") : password
        let so = SignOn(host: host, port: port, login: login, password: pw, nickname: login, icon: 0,
                        security: .auto, classic: false, media: false, history: false)
        signOnError = nil
        phase = .signingOn("Connecting…")
        do {
            let pipe = EventPipe()
            let s = try await connect(signOn: so, listener: pipe)
            guard s.info().messaging else {
                s.disconnect()
                throw HimError.Server(message: "Instant messaging isn't available on this server (or for this account).", reason: nil)
            }
            phase = .signingOn("Loading your buddies…")
            if savePassword, !pw.isEmpty { Keychain.set(acct.key, pw) } else if !savePassword { Keychain.forget(acct.key) }
            Settings.account = acct
            account = acct
            signOn = so
            loadHistory(acct)
            begin(s, pipe)
            await ready()
            phase = .online
        } catch {
            phase = .signedOff
            signOnError = describe(error)
        }
    }

    public func signOff() {
        epoch += 1
        session?.disconnect()
        session = nil
        signOn = nil
        history = nil
        if var a = account, a.autoSignOn {
            a.autoSignOn = false  // signing off by hand means "not automatically next time"
            Settings.account = a
            account = a
        }
        buddies = [:]
        conversations = [:]
        requests = []
        myName = nil
        myIconHash = nil
        target = nil
        presence = .online
        status = ""
        rooms.leaveAll()
        phase = .signedOff
    }

    /// Chat rooms without an IM account.
    public func roomsOnly() {
        signOnError = nil
        phase = .guest
    }

    private func loadHistory(_ acct: SavedAccount) {
        let h = IMHistory(account: acct)
        history = h
        conversations = [:]
        for (login, lines) in h.loadAll() {
            let c = Conversation(login: login)
            c.lines = lines
            conversations[login] = c
        }
    }

    /// Keeps a conversation on this device.
    private func saved(_ c: Conversation) {
        history?.save(c)
    }

    /// Clears a conversation here and on this device.
    public func clearHistory(_ login: String) {
        conversations[login]?.lines = []
        history?.forget(login)
    }

    private func begin(_ s: Session, _ pipe: EventPipe) {
        epoch += 1
        let mine = epoch
        session = s
        serverInfo = s.info()
        Task { [weak self] in
            for await e in pipe.stream {
                guard let self, self.epoch == mine else { return }
                self.handle(e)
            }
        }
    }

    /// Presence first, then the roster (which also brings the offline backlog).
    private func ready() async {
        guard let s = session else { return }
        try? await s.setPresence(presence: presence, status: status, discoverable: true)
        do {
            let list = try await s.getRoster()
            buddies = Dictionary(list.filter { $0.state != .removed }.map { ($0.login, $0) }, uniquingKeysWith: { $1 })
            for b in list where b.state == .pendingIn && !requests.contains(where: { $0.login == b.login }) {
                requests.append(FriendRequest(login: b.login, note: nil))
            }
            for b in buddies.values { icons.fetch(login: b.login, hash: b.iconHash, from: s) }
        } catch {
            notice = "Your Buddy List couldn't be loaded: \(describe(error))"
        }
        await loadMe()
    }

    private func loadMe() async {
        guard let s = session, let info = try? await s.getInfo(login: login) else { return }
        myName = [info.profile?.nickname, info.name].compactMap { $0 }
            .first { !$0.trimmingCharacters(in: .whitespaces).isEmpty && $0 != login }
        myIconHash = info.iconHash
        icons.fetch(login: login, hash: info.iconHash, from: s)
    }

    private func reconnect(after reason: String) {
        guard let so = signOn else { return }
        let mine = epoch
        phase = .reconnecting(reason)
        for k in Array(buddies.keys) { buddies[k]?.presence = .offline }
        Task { [weak self] in
            var wait: UInt64 = 2
            while true {
                try? await Task.sleep(nanoseconds: wait * 1_000_000_000)
                guard let self, self.epoch == mine, self.signOn != nil else { return }
                do {
                    let pipe = EventPipe()
                    let s = try await connect(signOn: so, listener: pipe)
                    guard self.epoch == mine else { s.disconnect(); return }
                    self.begin(s, pipe)
                    await self.ready()
                    self.phase = .online
                    return
                } catch HimError.LoginFailed(let m) {
                    self.signOff()
                    self.signOnError = m
                    return
                } catch {
                    self.phase = .reconnecting(describe(error))
                    wait = min(wait * 2, 60)
                }
            }
        }
    }

    // ---------- what the server says ----------

    private func handle(_ e: HimEvent) {
        switch e {
        case .buddy(let b):
            if b.state == .removed {
                buddies[b.login] = nil
                requests.removeAll { $0.login == b.login }
            } else {
                buddies[b.login] = b
                if b.state == .pendingIn, !requests.contains(where: { $0.login == b.login }) {
                    requests.append(FriendRequest(login: b.login, note: nil))
                }
                if b.state != .pendingIn { requests.removeAll { $0.login == b.login } }
                icons.fetch(login: b.login, hash: b.iconHash, from: session)
            }
        case .friendRequest(let login, let note):
            requests.removeAll { $0.login == login }
            requests.append(FriendRequest(login: login, note: note))
            Notifier.shared.post(title: "Buddy request", body: "\(login) wants to add you to their Buddy List.", id: "req-\(login)")
        case .presence(let u):
            guard var b = buddies[u.login] else { return }
            b.presence = u.presence
            b.statusText = u.statusText
            b.iconHash = u.iconHash
            if let n = u.displayName { b.displayName = n }
            buddies[u.login] = b
            icons.fetch(login: b.login, hash: b.iconHash, from: session)
        case .message(let m):
            receive(m)
        case .ack(let guid, let login, let kind):
            guard let c = conversations[login], let i = c.lines.firstIndex(where: { $0.id == guid }) else { return }
            if c.lines[i].status != .read { c.lines[i].status = kind == .read ? .read : .delivered }
            saved(c)
        case .typing(let login, let typing):
            conversation(login).typing = typing
        case .agreement:
            // Agreements are accepted automatically and never shown, as in the classic app.
            if let s = session { Task { try? await s.agree(nickname: login, icon: 0) } }
        case .serverMessage(let text):
            notice = text
        case .ownIconChanged(let hash):
            myIconHash = hash
            icons.fetch(login: login, hash: hash, from: session)
        case .disconnected(let reason):
            if signOn != nil { reconnect(after: reason) }
        case .chat, .userChanged, .userLeft, .privateMessage, .gifIconChanged:
            break  // chat rooms have their own sessions
        }
    }

    // ---------- conversations ----------

    public func conversation(_ login: String) -> Conversation {
        if let c = conversations[login] { return c }
        let c = Conversation(login: login)
        conversations[login] = c
        return c
    }

    /// Conversations with something in them, most recent first.
    public var recentConversations: [Conversation] {
        conversations.values.filter { !$0.lines.isEmpty }.sorted { $0.lastDate > $1.lastDate }
    }

    public var unreadTotal: Int { conversations.values.reduce(0) { $0 + $1.unread } }

    public func name(of login: String) -> String {
        buddies[login]?.shownName ?? login
    }

    private func receive(_ m: IncomingMessage) {
        let c = conversation(m.from)
        guard !c.lines.contains(where: { $0.id == m.guid }) else { return }  // redelivered across a reconnect
        let date = m.timestamp > 0 ? Date(timeIntervalSince1970: TimeInterval(m.timestamp)) : .now
        let viewing = target == .im(m.from) && Notifier.shared.appIsActive
        c.lines.append(Line(id: m.guid, direction: .incoming, body: m.body, date: date, status: viewing ? .seen : .unread))
        c.typing = false
        saved(c)
        if viewing {
            session?.ack(guid: m.guid, from: m.from, kind: .read)
        } else {
            Notifier.shared.post(title: name(of: m.from), body: m.body, id: "im-\(m.from)")
        }
        // Away: answer once with the away message.
        if presence == .away, !status.isEmpty, !autoAnswered.contains(m.from), !m.body.hasPrefix(Line.autoPrefix) {
            autoAnswered.insert(m.from)
            send(Line.autoPrefix + status, to: m.from)
        }
    }

    public func send(_ body: String, to login: String) {
        let text = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, let s = session else { return }
        let c = conversation(login)
        let guid = newGuid()
        // Emoji need UTF-8; on an older server they go as the faces classic clients show.
        let wire = serverInfo?.utf8 == true ? text : emojiToFaces(text: text)
        c.lines.append(Line(id: guid, direction: .outgoing, body: text, date: .now, status: .sending))
        saved(c)
        s.typing(to: login, typing: false)
        typingSent[login] = nil
        Task {
            let result: Line.Status
            do {
                result = try await s.sendIm(to: login, guid: guid, body: wire) ? .queued : .sent
            } catch {
                result = .failed(describe(error))
            }
            // A receipt may already have beaten the reply here.
            if let i = c.lines.firstIndex(where: { $0.id == guid }), c.lines[i].status == .sending {
                c.lines[i].status = result
            }
            self.saved(c)
        }
    }

    /// Typing indicators, at most every few seconds.
    public func typing(to login: String, _ typing: Bool) {
        guard let s = session else { return }
        if typing {
            if let t = typingSent[login], Date.now.timeIntervalSince(t) < 4 { return }
            typingSent[login] = .now
        } else {
            guard typingSent[login] != nil else { return }
            typingSent[login] = nil
        }
        s.typing(to: login, typing: typing)
    }

    /// Read receipts for everything showing.
    public func markRead(_ login: String) {
        guard let c = conversations[login] else { return }
        var any = false
        for i in c.lines.indices where c.lines[i].status == .unread {
            c.lines[i].status = .seen
            session?.ack(guid: c.lines[i].id, from: login, kind: .read)
            any = true
        }
        if any { saved(c) }
        Notifier.shared.clear(id: "im-\(login)")
    }

    public func open(_ login: String) {
        _ = conversation(login)
        target = .im(login)
    }

    // ---------- presence ----------

    public func setPresence(_ p: Presence, status text: String = "") async {
        guard let s = session else { return }
        do {
            try await s.setPresence(presence: p, status: text, discoverable: nil)
            presence = p
            status = text
            autoAnswered = []
            if p == .away, !text.isEmpty {
                Settings.awayMessages = [text] + Settings.awayMessages.filter { $0 != text }
            }
        } catch {
            notice = describe(error)
        }
    }

    public var awayMessages: [String] { Settings.awayMessages }

    // ---------- buddies ----------

    @discardableResult
    private func attempt(_ f: () async throws -> Void) async -> String? {
        do {
            try await f()
            return nil
        } catch {
            return describe(error)
        }
    }

    /// Nil on success, else what went wrong.
    public func addBuddy(_ login: String, note: String) async -> String? {
        guard let s = session else { return "You're not signed on." }
        let l = login.trimmingCharacters(in: .whitespaces)
        guard !l.isEmpty else { return "Enter a screen name." }
        return await attempt { try await s.addFriend(login: l, note: note) }
    }

    public func respond(to login: String, accept: Bool) async {
        guard let s = session else { return }
        if let err = await attempt({ try await s.respond(login: login, accept: accept) }) { notice = err; return }
        requests.removeAll { $0.login == login }
    }

    public func remove(_ login: String) async {
        guard let s = session else { return }
        if let err = await attempt({ try await s.removeFriend(login: login) }) { notice = err }
    }

    public func block(_ login: String, _ on: Bool) async {
        guard let s = session else { return }
        if let err = await attempt({ on ? try await s.block(login: login) : try await s.unblock(login: login) }) { notice = err }
    }

    public func setAlias(_ login: String, _ alias: String) async {
        guard let s = session else { return }
        if let err = await attempt({ try await s.setAlias(login: login, alias: alias.trimmingCharacters(in: .whitespaces)) }) {
            notice = err
        }
    }

    public func info(_ login: String) async -> UserInfo? {
        try? await session?.getInfo(login: login)
    }

    public func search(_ q: String) async -> [FoundUser] {
        let q = q.trimmingCharacters(in: .whitespaces)
        guard q.count >= 2, let s = session else { return [] }
        return (try? await s.search(query: q)) ?? []
    }

    // ---------- me ----------

    /// Changes only the name buddies see; Set User Info replaces the whole profile, so the rest goes back as it was.
    public func setDisplayName(_ name: String) async -> String? {
        guard let s = session else { return "You're not signed on." }
        let err = await attempt {
            var p = try await s.getInfo(login: login).profile ?? Profile(
                nickname: nil, firstName: nil, lastName: nil, email: nil, gender: 0,
                birthYear: 0, birthMonth: 0, birthDay: 0, country: nil, postcode: nil, languages: [])
            let n = name.trimmingCharacters(in: .whitespaces)
            p.nickname = n.isEmpty ? nil : n
            try await s.setInfo(profile: p)
        }
        await loadMe()
        return err
    }

    public var canSetIcon: Bool { serverInfo?.maxIconBytes != nil }

    /// Sets our Buddy Icon from any picture (it's resized when the server needs it smaller).
    public func setBuddyIcon(_ picture: Data) async -> String? {
        guard let s = session, let info = serverInfo else { return "You're not signed on." }
        guard let maxBytes = info.maxIconBytes else { return "This server doesn't have Buddy Icons." }
        guard let data = Shrink.forBuddyIcon(picture, maxBytes: Int(maxBytes), maxDimension: Int(max(64, info.maxIconDimension))) else {
            return "That picture can't be used as a Buddy Icon."
        }
        do {
            let hash = try await s.setBuddyIcon(picture: data) ?? iconHash(data: data)
            icons.store(data, as: hash)
            myIconHash = hash
            return nil
        } catch {
            return describe(error)
        }
    }
}
