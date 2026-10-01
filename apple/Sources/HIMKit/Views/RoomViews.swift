import HIMCore
import PhotosUI
import SwiftUI

public struct RoomView: View {
    @Environment(AppModel.self) private var app
    @Bindable var room: Room
    @State private var showPeople = true
    @State private var picking = false
    @State private var photo: PhotosPickerItem?
    @State private var pictureError: String?
    @FocusState private var composing: Bool

    public var body: some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        if room.hasEarlier {
                            Button {
                                Task { await room.loadEarlier() }
                            } label: {
                                if room.loadingEarlier { ProgressView().controlSize(.small) } else { Text("Earlier messages") }
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 8)
                        }
                        let shown = room.shownLines
                        ForEach(Array(shown.enumerated()), id: \.element.id) { i, line in
                            RoomLineView(room: room, line: line,
                                         continued: i > 0 && shown[i - 1].kind == .chat && line.kind == .chat
                                             && shown[i - 1].name == line.name)
                        }
                        Color.clear.frame(height: 1).id("end")
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                }
                .defaultScrollAnchor(.bottom)
                .onChange(of: room.lines.count) { proxy.scrollTo("end", anchor: .bottom) }
            }
            HStack(alignment: .bottom, spacing: 8) {
                if room.canSendPictures {
                    attachButton
                }
                TextField("Message \(room.title)", text: $room.draft, axis: .vertical)
                    .textFieldStyle(.plain)
                    .lineLimit(1...4)
                    .focused($composing)
                    .onSubmit(send)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .background(.background, in: RoundedRectangle(cornerRadius: 19, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 19, style: .continuous).strokeBorder(.separator))
                Button(action: send) {
                    Image(systemName: "arrow.up.circle.fill").font(.system(size: 30)).symbolRenderingMode(.hierarchical)
                }
                .buttonStyle(.plain)
                .foregroundStyle(Brand.accent)
                .disabled(room.state != .joined || room.draft.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .background(.bar)
        }
        .navigationTitle(room.title)
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { showPeople.toggle() } label: { Label("People", systemImage: "person.2") }
            }
        }
        #if os(macOS)
        .inspector(isPresented: $showPeople) { PeopleList(room: room).inspectorColumnWidth(min: 180, ideal: 210, max: 280) }
        #else
        .sheet(isPresented: $showPeople) { NavigationStack { PeopleList(room: room).navigationTitle("People") }.presentationDetents([.medium, .large]) }
        #endif
        .onAppear {
            room.unread = 0
            composing = true
            #if os(iOS)
            showPeople = false
            #endif
        }
        .onChange(of: room.lines.count) { if app.target == .room(room.id) { room.unread = 0 } }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.image]) { result in
            guard case .success(let url) = result else { return }
            sendFile(url)
        }
        .onChange(of: photo) {
            guard let photo else { return }
            Task {
                if let d = try? await photo.loadTransferable(type: Data.self) { await sendPicture(d) }
                self.photo = nil
            }
        }
        #if os(macOS)
        .dropDestination(for: URL.self) { urls, _ in
            guard room.canSendPictures, let url = urls.first else { return false }
            sendFile(url)
            return true
        }
        #endif
        .alert("Couldn't send the picture", isPresented: Binding(get: { pictureError != nil }, set: { if !$0 { pictureError = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(pictureError ?? "")
        }
    }

    @ViewBuilder private var attachButton: some View {
        Group {
            if room.sending {
                ProgressView().controlSize(.small).frame(width: 30, height: 30)
            } else {
                #if os(macOS)
                Button { picking = true } label: { attachIcon }
                #else
                PhotosPicker(selection: $photo, matching: .images) { attachIcon }
                #endif
            }
        }
        .buttonStyle(.plain)
        .help("Send a picture")
    }

    private var attachIcon: some View {
        Image(systemName: "photo.on.rectangle.angled")
            .font(.system(size: 19))
            .foregroundStyle(.secondary)
            .frame(width: 30, height: 34)
    }

    private func sendFile(_ url: URL) {
        let ok = url.startAccessingSecurityScopedResource()
        defer { if ok { url.stopAccessingSecurityScopedResource() } }
        guard let d = try? Data(contentsOf: url) else { pictureError = "That file couldn't be read."; return }
        Task { await sendPicture(d) }
    }

    private func sendPicture(_ d: Data) async {
        let caption = room.draft
        if let err = await room.sendPicture(d, caption: caption) {
            pictureError = err
        } else {
            room.draft = ""
        }
    }

    private func send() {
        room.send(room.draft)
        room.draft = ""
        composing = true
    }
}

private struct RoomLineView: View {
    let room: Room
    let line: RoomLine
    let continued: Bool

    var body: some View {
        switch line.kind {
        case .chat:
            HStack(alignment: .top, spacing: 10) {
                if continued {
                    Color.clear.frame(width: 30, height: 1)
                } else {
                    let u = room.user(named: line.name)
                    ClassicAvatar(icon: u?.icon ?? 0, name: line.name, size: 30, gif: u.flatMap { room.gifIcons[$0.id] })
                }
                VStack(alignment: .leading, spacing: 2) {
                    if !continued {
                        HStack(alignment: .firstTextBaseline, spacing: 6) {
                            Text(line.name)
                                .fontWeight(.semibold)
                                .foregroundStyle(line.mine ? Brand.accent : nameColor(line.name))
                            Text(line.date.formatted(date: .omitted, time: .shortened)).font(.caption2).foregroundStyle(.tertiary)
                        }
                    }
                    if let m = line.media {
                        if line.text != "[image]" { Text(linkified(line.text)).textSelection(.enabled) }
                        RoomPicture(room: room, media: m)
                    } else {
                        Text(linkified(line.text)).textSelection(.enabled)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.top, continued ? 2 : 10)
        case .emote:
            Text(linkified("* " + line.text))
                .italic()
                .foregroundStyle(.secondary)
                .padding(.leading, 40)
                .padding(.top, 6)
        case .join, .leave:
            Label(line.text, systemImage: line.kind == .join ? "arrow.right.circle" : "arrow.left.circle")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .padding(.leading, 40)
                .padding(.top, 4)
        case .system:
            Text(line.text)
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity)
                .multilineTextAlignment(.center)
                .padding(.vertical, 6)
        }
    }
}

/// A picture in a chat line: a placeholder its size while it loads, then the picture.
private struct RoomPicture: View {
    let room: Room
    let media: MediaRef
    @State private var zoomed = false

    var body: some View {
        let maxW: CGFloat = 300, maxH: CGFloat = 260
        let w = CGFloat(max(media.width, 1)), h = CGFloat(max(media.height, 1))
        let f = min(1, maxW / w, maxH / h)
        let size = media.width > 0 ? CGSize(width: w * f, height: h * f) : CGSize(width: 220, height: 160)
        Group {
            if let img = room.image(for: media) {
                IconImage(image: img)
                    .frame(maxWidth: maxW, maxHeight: maxH)
                    .onTapGesture { zoomed = true }
            } else if room.pictureFailed(media) {
                Label("The picture couldn't be shown.", systemImage: "photo.badge.exclamationmark")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(width: size.width, height: 60)
                    .background(.fill.tertiary)
            } else {
                ProgressView()
                    .frame(width: size.width, height: size.height)
                    .background(.fill.tertiary)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .padding(.top, 2)
        .sheet(isPresented: $zoomed) {
            if let img = room.image(for: media) {
                IconImage(image: img)
                    .padding()
                    .macMinSize(width: 360, height: 300)
                    .onTapGesture { zoomed = false }
            }
        }
    }
}

private struct PeopleList: View {
    let room: Room
    @Environment(\.openURL) private var openURL

    var body: some View {
        List {
            Section("\(room.users.count) here") {
                ForEach(room.people, id: \.id) { u in
                    HStack(spacing: 8) {
                        ClassicAvatar(icon: u.icon, name: u.name, size: 26, gif: room.gifIcons[u.id])
                        Text(u.name)
                            .foregroundStyle(u.admin ? Color.red : (u.away ? Color.secondary : Color.primary))
                            .lineLimit(1)
                        Spacer(minLength: 0)
                        if u.away { Image(systemName: "moon.fill").font(.caption2).foregroundStyle(.secondary) }
                        if room.isIgnored(u.name) { Image(systemName: "eye.slash").font(.caption2).foregroundStyle(.secondary) }
                    }
                    .opacity(room.isIgnored(u.name) ? 0.5 : 1)
                    .contextMenu {
                        if room.isIgnored(u.name) {
                            Button { room.setIgnored(u.name, false) } label: { Label("Show Their Messages", systemImage: "eye") }
                        } else {
                            Button { room.setIgnored(u.name, true) } label: { Label("Ignore", systemImage: "eye.slash") }
                        }
                        Button { if let url = room.reportURL(u.name) { openURL(url) } } label: {
                            Label("Report…", systemImage: "exclamationmark.bubble")
                        }
                    }
                }
            }
        }
    }
}

/// The servers the trackers know about.
public struct RoomBrowser: View {
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var nick = ""
    @State private var address = ""

    public var body: some View {
        let rooms = app.rooms
        let list = rooms.servers.filter {
            query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) || $0.description.localizedCaseInsensitiveContains(query)
        }
        NavigationStack {
            List {
                Section {
                    TextField("Your name in rooms", text: $nick, prompt: Text("HIM user"))
                    HStack {
                        TextField("Or join by address", text: $address, prompt: Text("server.example.com"))
                            #if os(iOS)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            #endif
                        Button("Join") { joinAddress() }.disabled(address.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
                Section(rooms.loadingServers && rooms.servers.isEmpty ? "Asking the trackers…" : "\(list.count) servers") {
                    ForEach(list, id: \.self) { s in
                        Button { join(s.host, s.port, s.name) } label: {
                            HStack(alignment: .top, spacing: 10) {
                                Image(systemName: "number")
                                    .font(.system(size: 13, weight: .semibold))
                                    .foregroundStyle(.white)
                                    .frame(width: 30, height: 30)
                                    .background(nameColor(s.name).gradient, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(s.name).fontWeight(.medium).foregroundStyle(.primary)
                                    if !s.description.isEmpty {
                                        Text(s.description).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                                    }
                                }
                                Spacer(minLength: 4)
                                Label("\(s.users)", systemImage: "person.fill")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .labelStyle(.titleAndIcon)
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .searchable(text: $query, prompt: "Search servers")
            .navigationTitle("Chat Rooms")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() } }
                ToolbarItem(placement: .primaryAction) {
                    Button { Task { await rooms.refreshServers(force: true) } } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                        .disabled(rooms.loadingServers)
                }
            }
            .task { await rooms.refreshServers() }
        }
        .macMinSize(width: 460, height: 520)
        .onAppear { nick = rooms.nick }
    }

    private func join(_ host: String, _ port: UInt16, _ name: String) {
        app.rooms.nick = nick
        app.rooms.join(host: host, port: port, title: name)
        dismiss()
    }

    private func joinAddress() {
        let a = address.trimmingCharacters(in: .whitespaces)
        let parts = a.split(separator: ":")
        let host = String(parts.first ?? "")
        let port = parts.count > 1 ? UInt16(parts[1]) ?? 5500 : 5500
        join(host, port, host)
    }
}
