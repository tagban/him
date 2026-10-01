import HIMCore
import SwiftUI

// ---------- rows ----------

/// A row that opens `target`: by the sidebar's selection on the Mac, by pushing on iOS.
struct TargetRow<Content: View>: View {
    let target: Target
    let content: Content

    init(_ target: Target, @ViewBuilder content: () -> Content) {
        self.target = target
        self.content = content()
    }

    var body: some View {
        #if os(macOS)
        content.tag(target)
        #else
        NavigationLink(value: target) { content }
        #endif
    }
}

struct BuddyRow: View {
    let buddy: Buddy

    var body: some View {
        HStack(spacing: 10) {
            Avatar(name: buddy.shownName, hash: buddy.iconHash, size: 34, presence: buddy.presence)
                .saturation(buddy.presence == .offline ? 0 : 1)
                .opacity(buddy.presence == .offline ? 0.6 : 1)
            VStack(alignment: .leading, spacing: 1) {
                Text(buddy.shownName)
                    .fontWeight(.medium)
                    .foregroundStyle(buddy.presence == .offline ? .secondary : .primary)
                if let s = buddy.statusText, !s.isEmpty, buddy.presence != .offline {
                    Text(s).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                } else if buddy.state == .pendingOut {
                    Text("Waiting for them to accept").font(.caption).foregroundStyle(.secondary)
                } else if buddy.state == .blocked {
                    Text("Blocked").font(.caption).foregroundStyle(.red)
                }
            }
            Spacer(minLength: 0)
            if buddy.presence == .away {
                Image(systemName: "moon.fill").font(.caption).foregroundStyle(.yellow)
            }
        }
        .padding(.vertical, 2)
    }
}

struct ConversationRow: View {
    @Environment(AppModel.self) private var app
    let conversation: Conversation

    var body: some View {
        let b = app.buddies[conversation.login]
        HStack(spacing: 10) {
            Avatar(name: b?.shownName ?? conversation.login, hash: b?.iconHash, size: 40, presence: b?.presence)
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline) {
                    Text(b?.shownName ?? conversation.login)
                        .fontWeight(conversation.unread > 0 ? .semibold : .medium)
                        .lineLimit(1)
                    Spacer(minLength: 4)
                    Text(conversation.lastDate.formatted(Date.FormatStyle(date: .omitted, time: .shortened)))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                HStack {
                    Group {
                        if conversation.typing {
                            Text("typing…").italic()
                        } else if let last = conversation.lines.last {
                            Text((last.direction == .outgoing ? "You: " : "") + last.text)
                        }
                    }
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    Spacer(minLength: 4)
                    if conversation.unread > 0 {
                        Text("\(conversation.unread)")
                            .font(.caption2.weight(.bold))
                            .foregroundStyle(.white)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Color.accentColor, in: Capsule())
                    }
                }
            }
        }
        .padding(.vertical, 3)
    }
}

struct RoomRow: View {
    let room: Room

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "number")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(.white)
                .frame(width: 34, height: 34)
                .background(nameColor(room.title).gradient, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            VStack(alignment: .leading, spacing: 1) {
                Text(room.title).fontWeight(.medium).lineLimit(1)
                Group {
                    switch room.state {
                    case .joining: Text("Joining…")
                    case .joined: Text("\(room.users.count) here")
                    case .left(let why): Text(why ?? "Left")
                    }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }
            Spacer(minLength: 0)
            if room.unread > 0 {
                Circle().fill(Color.accentColor).frame(width: 8, height: 8)
            }
        }
        .padding(.vertical, 2)
    }
}

// ---------- the lists ----------

/// Buddy requests waiting for an answer.
struct RequestsSection: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        if !app.requests.isEmpty {
            Section("Requests") {
                ForEach(app.requests) { r in
                    VStack(alignment: .leading, spacing: 6) {
                        HStack(spacing: 10) {
                            Avatar(name: r.login, hash: nil, size: 30)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(r.login).fontWeight(.medium)
                                Text(r.note?.isEmpty == false ? r.note! : "Wants to add you").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        HStack {
                            Button("Accept") { Task { await app.respond(to: r.login, accept: true) } }
                                .buttonStyle(.borderedProminent)
                            Button("Decline") { Task { await app.respond(to: r.login, accept: false) } }
                                .buttonStyle(.bordered)
                        }
                        .controlSize(.small)
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }
}

/// Buddies, online first.
struct BuddySections: View {
    @Environment(AppModel.self) private var app
    var filter = ""

    var body: some View {
        let all = app.buddies.values
            .filter { $0.state != .pendingIn }
            .filter { filter.isEmpty || $0.shownName.localizedCaseInsensitiveContains(filter) || $0.login.localizedCaseInsensitiveContains(filter) }
            .sorted { $0.shownName.localizedCaseInsensitiveCompare($1.shownName) == .orderedAscending }
        let online = all.filter { $0.presence != .offline }
        let offline = all.filter { $0.presence == .offline }
        RequestsSection()
        Section("Online · \(online.count)") {
            ForEach(online, id: \.login) { b in
                TargetRow(.im(b.login)) { BuddyRow(buddy: b) }.contextMenu { BuddyMenu(login: b.login) }
            }
        }
        if !offline.isEmpty {
            Section("Offline · \(offline.count)") {
                ForEach(offline, id: \.login) { b in
                    TargetRow(.im(b.login)) { BuddyRow(buddy: b) }.contextMenu { BuddyMenu(login: b.login) }
                }
            }
        }
    }
}

struct BuddyMenu: View {
    @Environment(AppModel.self) private var app
    let login: String

    var body: some View {
        Button("Send Message") { app.open(login) }
        Divider()
        if app.buddies[login]?.state == .blocked {
            Button("Unblock") { Task { await app.block(login, false) } }
        } else {
            Button("Block") { Task { await app.block(login, true) } }
        }
        Button("Remove from Buddy List", role: .destructive) { Task { await app.remove(login) } }
    }
}

struct ConversationSections: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        let list = app.recentConversations
        if list.isEmpty {
            ContentUnavailableView("No messages yet", systemImage: "bubble.left.and.bubble.right",
                                   description: Text("Pick a buddy to start a conversation."))
                .listRowSeparator(.hidden)
        }
        ForEach(list) { c in
            TargetRow(.im(c.login)) { ConversationRow(conversation: c) }
        }
    }
}

struct RoomSections: View {
    @Environment(AppModel.self) private var app
    @Binding var browsing: Bool

    var body: some View {
        if app.rooms.joined.isEmpty {
            ContentUnavailableView {
                Label("No chat rooms", systemImage: "number")
            } description: {
                Text("Chat rooms are Hotline servers' public chats.")
            } actions: {
                Button("Find a Room") { browsing = true }.buttonStyle(.borderedProminent)
            }
            .listRowSeparator(.hidden)
        }
        ForEach(app.rooms.joined) { r in
            TargetRow(.room(r.id)) { RoomRow(room: r) }
                .contextMenu { Button("Leave Room", role: .destructive) { app.rooms.leave(r) } }
        }
    }
}
