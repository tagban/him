import HIMCore
import SwiftUI

/// Text with its links made tappable.
func linkified(_ s: String) -> AttributedString {
    var out = AttributedString(s)
    guard let det = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue) else { return out }
    for m in det.matches(in: s, range: NSRange(s.startIndex..., in: s)) {
        guard let url = m.url, let r = Range(m.range, in: s),
              let lo = AttributedString.Index(r.lowerBound, within: out),
              let hi = AttributedString.Index(r.upperBound, within: out) else { continue }
        out[lo..<hi].link = url
        out[lo..<hi].underlineStyle = .single
    }
    return out
}

public struct ConversationView: View {
    @Environment(AppModel.self) private var app
    let login: String
    @State private var showInfo = false
    @FocusState private var composing: Bool

    public init(login: String) { self.login = login }

    public var body: some View {
        let c = app.conversation(login)
        let buddy = app.buddies[login]
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 2) {
                        Header(login: login).padding(.vertical, 24)
                        ForEach(Array(c.lines.enumerated()), id: \.element.id) { i, line in
                            if let sep = separator(c.lines, i) {
                                Text(sep)
                                    .font(.caption2.weight(.medium))
                                    .foregroundStyle(.secondary)
                                    .padding(.top, 14)
                                    .padding(.bottom, 6)
                            }
                            Bubble(line: line, login: login,
                                   first: isFirst(c.lines, i), last: isLast(c.lines, i),
                                   showStatus: line.direction == .outgoing && i == c.lines.lastIndex { $0.direction == .outgoing })
                        }
                        if c.typing {
                            TypingBubble(login: login).transition(.opacity.combined(with: .scale(scale: 0.8, anchor: .bottomLeading)))
                        }
                        Color.clear.frame(height: 1).id("end")
                    }
                    .padding(.horizontal, 14)
                    .padding(.bottom, 8)
                    .animation(.snappy(duration: 0.25), value: c.lines.count)
                    .animation(.snappy(duration: 0.25), value: c.typing)
                }
                .defaultScrollAnchor(.bottom)
                .onChange(of: c.lines.count) { proxy.scrollTo("end", anchor: .bottom) }
                .onChange(of: c.typing) { if c.typing { withAnimation { proxy.scrollTo("end", anchor: .bottom) } } }
            }
            Composer(conversation: c, focus: $composing)
        }
        .navigationTitle(buddy?.shownName ?? login)
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .principal) {
                VStack(spacing: 0) {
                    Text(buddy?.shownName ?? login).font(.headline)
                    Text(subtitle(buddy)).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            ToolbarItem(placement: .primaryAction) {
                Button { showInfo = true } label: { Label("Info", systemImage: "info.circle") }
            }
        }
        .sheet(isPresented: $showInfo) { BuddyInfoView(login: login) }
        .onAppear { app.markRead(login); composing = true }
        .onChange(of: c.lines.count) { app.markRead(login) }
    }

    private func subtitle(_ b: Buddy?) -> String {
        guard let b else { return "Not on your Buddy List" }
        if let s = b.statusText, !s.isEmpty, b.presence != .offline { return "\(b.presence.label) · \(s)" }
        return b.presence.label
    }

    private func isFirst(_ l: [Line], _ i: Int) -> Bool {
        i == 0 || l[i - 1].direction != l[i].direction || l[i].date.timeIntervalSince(l[i - 1].date) > 300
    }

    private func isLast(_ l: [Line], _ i: Int) -> Bool {
        i == l.count - 1 || l[i + 1].direction != l[i].direction || l[i + 1].date.timeIntervalSince(l[i].date) > 300
    }

    /// A time heading before the first line and after a quiet spell.
    private func separator(_ l: [Line], _ i: Int) -> String? {
        guard i == 0 || l[i].date.timeIntervalSince(l[i - 1].date) > 15 * 60 else { return nil }
        let d = l[i].date
        let time = d.formatted(date: .omitted, time: .shortened)
        if Calendar.current.isDateInToday(d) { return "Today \(time)" }
        if Calendar.current.isDateInYesterday(d) { return "Yesterday \(time)" }
        return d.formatted(.dateTime.weekday(.wide).month().day().hour().minute())
    }
}

/// Who you're talking to, at the top of the conversation.
private struct Header: View {
    @Environment(AppModel.self) private var app
    let login: String

    var body: some View {
        let b = app.buddies[login]
        VStack(spacing: 8) {
            Avatar(name: b?.shownName ?? login, hash: b?.iconHash, size: 72)
            Text(b?.shownName ?? login).font(.title3.weight(.semibold))
            if b?.shownName != login {
                Text(login).font(.caption).foregroundStyle(.secondary)
            }
            if b == nil {
                Button("Add to Buddy List") { Task { _ = await app.addBuddy(login, note: "") } }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
            }
        }
        .frame(maxWidth: .infinity)
    }
}

private struct Bubble: View {
    @Environment(AppModel.self) private var app
    let line: Line
    let login: String
    let first: Bool
    let last: Bool
    let showStatus: Bool

    private var mine: Bool { line.direction == .outgoing }

    var body: some View {
        VStack(alignment: mine ? .trailing : .leading, spacing: 3) {
            HStack(alignment: .bottom, spacing: 8) {
                if mine { Spacer(minLength: 48) }
                if !mine {
                    if last {
                        let b = app.buddies[login]
                        Avatar(name: b?.shownName ?? login, hash: b?.iconHash, size: 28)
                    } else {
                        Color.clear.frame(width: 28, height: 1)
                    }
                }
                VStack(alignment: .leading, spacing: 2) {
                    if line.isAuto {
                        Label("Auto-reply", systemImage: "moon.zzz.fill")
                            .font(.caption2.weight(.semibold))
                            .opacity(0.75)
                    }
                    Text(linkified(line.text))
                        .italic(line.isAuto)
                        .textSelection(.enabled)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .foregroundStyle(mine ? .white : .primary)
                .background(shape.fill(mine ? AnyShapeStyle(Color.accentColor.gradient) : AnyShapeStyle(.fill.tertiary)))
                .opacity(line.status == .sending ? 0.7 : 1)
                .help(line.date.formatted(date: .abbreviated, time: .shortened))
                if !mine { Spacer(minLength: 48) }
            }
            if mine, let s = statusText {
                Text(s)
                    .font(.caption2)
                    .foregroundStyle(isFailed ? AnyShapeStyle(.red) : AnyShapeStyle(.secondary))
                    .padding(.trailing, 4)
            }
        }
        .padding(.top, first ? 6 : 0)
    }

    private var shape: UnevenRoundedRectangle {
        let big: CGFloat = 18, small: CGFloat = 6
        return mine
            ? UnevenRoundedRectangle(topLeadingRadius: big, bottomLeadingRadius: big,
                                     bottomTrailingRadius: last ? big : small, topTrailingRadius: first ? big : small, style: .continuous)
            : UnevenRoundedRectangle(topLeadingRadius: first ? big : small, bottomLeadingRadius: last ? big : small,
                                     bottomTrailingRadius: big, topTrailingRadius: big, style: .continuous)
    }

    private var isFailed: Bool {
        if case .failed = line.status { return true }
        return false
    }

    private var statusText: String? {
        switch line.status {
        case .failed(let why): return "Not sent: \(why)"
        case .queued: return "They're offline: delivers when they sign on"
        case _ where !showStatus: return nil
        case .sending: return "Sending…"
        case .sent: return "Sent"
        case .delivered: return "Delivered"
        case .read: return "Read"
        default: return nil
        }
    }
}

private struct TypingBubble: View {
    @Environment(AppModel.self) private var app
    let login: String

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            let b = app.buddies[login]
            Avatar(name: b?.shownName ?? login, hash: b?.iconHash, size: 28)
            TimelineView(.periodic(from: .now, by: 0.35)) { ctx in
                let step = Int(ctx.date.timeIntervalSinceReferenceDate / 0.35) % 3
                HStack(spacing: 4) {
                    ForEach(0..<3) { i in
                        Circle().frame(width: 7, height: 7).opacity(i == step ? 0.9 : 0.35)
                    }
                }
                .foregroundStyle(.secondary)
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
                .background(.fill.tertiary, in: Capsule())
            }
            Spacer()
        }
        .padding(.top, 6)
    }
}

private struct Composer: View {
    @Environment(AppModel.self) private var app
    @Bindable var conversation: Conversation
    var focus: FocusState<Bool>.Binding

    private var empty: Bool { conversation.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField("Message", text: $conversation.draft, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...6)
                .focused(focus)
                .onSubmit(send)
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
                .background(.background, in: RoundedRectangle(cornerRadius: 19, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 19, style: .continuous).strokeBorder(.separator))
                .onChange(of: conversation.draft) { _, new in
                    app.typing(to: conversation.login, !new.isEmpty)
                }
            Button(action: send) {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.system(size: 30))
                    .symbolRenderingMode(.hierarchical)
            }
            .buttonStyle(.plain)
            .foregroundStyle(empty ? AnyShapeStyle(.tertiary) : AnyShapeStyle(Color.accentColor))
            .disabled(empty || !app.isSignedOn)
            .keyboardShortcut(.return, modifiers: .command)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(.bar)
    }

    private func send() {
        guard !empty else { return }
        app.send(conversation.draft, to: conversation.login)
        conversation.draft = ""
        focus.wrappedValue = true
    }
}
