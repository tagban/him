import HIMCore
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// You: picture, name, and how you're showing to buddies.
struct MeHeader: View {
    @Environment(AppModel.self) private var app
    @Binding var editing: Bool
    @State private var writingAway = false

    var body: some View {
        HStack(spacing: 10) {
            Button { editing = true } label: {
                Avatar(name: app.shownName, hash: app.myIconHash, size: 38, presence: app.isSignedOn ? app.presence : nil)
            }
            .buttonStyle(.plain)
            .help("Your profile and Buddy Icon")
            VStack(alignment: .leading, spacing: 1) {
                Text(app.phase == .guest ? "Guest" : app.shownName).font(.headline).lineLimit(1)
                if app.phase != .guest { presenceMenu }
            }
            Spacer(minLength: 0)
        }
        .sheet(isPresented: $writingAway) { AwayEditor() }
    }

    @ViewBuilder private var presenceMenu: some View {
        if case .reconnecting(let why) = app.phase {
            Label("Reconnecting…", systemImage: "arrow.triangle.2.circlepath")
                .font(.caption)
                .foregroundStyle(.orange)
                .help(why)
        } else {
            Menu {
                Button { Task { await app.setPresence(.online) } } label: { Label("Available", systemImage: "circle.fill") }
                Button { Task { await app.setPresence(.busy, status: "Busy") } } label: { Label("Busy", systemImage: "minus.circle.fill") }
                Section("Away") {
                    ForEach(app.awayMessages.prefix(4), id: \.self) { m in
                        Button(m) { Task { await app.setPresence(.away, status: m) } }
                    }
                    Button("New Away Message…") { writingAway = true }
                }
                Button { Task { await app.setPresence(.invisible) } } label: { Label("Invisible", systemImage: "eye.slash") }
                Divider()
                Button("Sign Off", role: .destructive) { app.signOff() }
            } label: {
                HStack(spacing: 4) {
                    Text(app.status.isEmpty ? app.presence.label : app.status).lineLimit(1)
                    Image(systemName: "chevron.down").font(.system(size: 8, weight: .bold))
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .fixedSize()
        }
    }
}

struct AwayEditor: View {
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("I'm away because…", text: $text, axis: .vertical).lineLimit(3...6)
                } footer: {
                    Text("Buddies see this, and anyone who IMs you gets it as an auto-reply once.")
                }
            }
            .formStyle(.grouped)
            .navigationTitle("Away Message")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Go Away") {
                        Task { await app.setPresence(.away, status: text.trimmingCharacters(in: .whitespacesAndNewlines)); dismiss() }
                    }
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
        .macMinSize(width: 360, height: 240)
    }
}

/// Your display name and Buddy Icon.
struct ProfileView: View {
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var error: String?
    @State private var saving = false
    @State private var picking = false
    @State private var photo: PhotosPickerItem?
    @State private var browsing = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    VStack(spacing: 10) {
                        Avatar(name: app.shownName, hash: app.myIconHash, size: 96)
                        if app.canSetIcon {
                            Button("BadassBuddy Icons…") { browsing = true }
                                .buttonStyle(.borderedProminent)
                            #if os(macOS)
                            Button("Choose a Picture…") { picking = true }
                            #else
                            PhotosPicker("Choose a Photo", selection: $photo, matching: .images)
                            #endif
                            BadassBuddyCredit()
                        } else {
                            Text("This server doesn't have Buddy Icons.").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                }
                Section {
                    TextField("Display name", text: $name, prompt: Text(app.login))
                    LabeledContent("Screen name", value: app.login)
                } footer: {
                    Text("The name buddies see. Leave it empty to go by your screen name.")
                }
                if let error {
                    Section { Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.red) }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("My Profile")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }.disabled(saving || name == (app.myName ?? ""))
                }
            }
            .fileImporter(isPresented: $picking, allowedContentTypes: [.image]) { result in
                guard case .success(let url) = result else { return }
                let ok = url.startAccessingSecurityScopedResource()
                defer { if ok { url.stopAccessingSecurityScopedResource() } }
                if let d = try? Data(contentsOf: url) { setIcon(d) }
            }
            .sheet(isPresented: $browsing) { IconGalleryView() }
            .onChange(of: photo) {
                guard let photo else { return }
                Task { if let d = try? await photo.loadTransferable(type: Data.self) { setIcon(d) } }
            }
        }
        .macMinSize(width: 380, height: 420)
        .onAppear { name = app.myName ?? "" }
    }

    private func setIcon(_ d: Data) {
        Task { error = await app.setBuddyIcon(d) }
    }

    private func save() {
        saving = true
        Task {
            error = await app.setDisplayName(name)
            saving = false
            if error == nil { dismiss() }
        }
    }
}

struct AddBuddyView: View {
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var login = ""
    @State private var note = ""
    @State private var results: [FoundUser] = []
    @State private var error: String?
    @State private var busy = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Screen name", text: $login)
                        #if os(iOS)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        #endif
                    TextField("Note (optional)", text: $note, prompt: Text("Hi, it's me!"))
                } footer: {
                    Text("They'll get a request, and show up once they accept.")
                }
                if !results.isEmpty {
                    Section("People") {
                        ForEach(results, id: \.login) { u in
                            Button { login = u.login } label: {
                                HStack {
                                    Avatar(name: u.name ?? u.login, hash: nil, size: 26)
                                    VStack(alignment: .leading) {
                                        Text(u.name ?? u.login)
                                        if u.name != nil { Text(u.login).font(.caption).foregroundStyle(.secondary) }
                                    }
                                }
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                if let error {
                    Section { Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.red) }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("Add a Buddy")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add") {
                        busy = true
                        Task {
                            error = await app.addBuddy(login, note: note)
                            busy = false
                            if error == nil { dismiss() }
                        }
                    }
                    .disabled(busy || login.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            .task(id: login) {
                try? await Task.sleep(nanoseconds: 400_000_000)
                results = await app.search(login)
            }
        }
        .macMinSize(width: 380, height: 320)
    }
}

/// Someone's profile, and what you can do about them.
struct BuddyInfoView: View {
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    let login: String
    @State private var info: UserInfo?
    @State private var alias = ""
    @State private var loaded = false

    var body: some View {
        let b = app.buddies[login]
        NavigationStack {
            Form {
                Section {
                    VStack(spacing: 8) {
                        Avatar(name: b?.shownName ?? login, hash: b?.iconHash ?? info?.iconHash, size: 84, presence: b?.presence)
                        Text(b?.shownName ?? info?.profile?.nickname ?? info?.name ?? login).font(.title3.weight(.semibold))
                        Text(login).font(.caption).foregroundStyle(.secondary)
                        if let s = b?.statusText, !s.isEmpty {
                            Text(s).font(.callout).multilineTextAlignment(.center).foregroundStyle(.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 6)
                }
                if let p = info?.profile {
                    Section("Profile") {
                        let full = [p.firstName, p.lastName].compactMap { $0 }.joined(separator: " ")
                        if !full.isEmpty { LabeledContent("Name", value: full) }
                        if let c = p.country { LabeledContent("Country", value: c) }
                        if !p.languages.isEmpty { LabeledContent("Languages", value: p.languages.joined(separator: ", ")) }
                        if let e = p.email { LabeledContent("Email", value: e) }
                    }
                }
                if b != nil {
                    Section {
                        TextField("Your name for them", text: $alias, prompt: Text(b?.displayName ?? login))
                            .onSubmit { Task { await app.setAlias(login, alias) } }
                    } header: {
                        Text("Nickname")
                    } footer: {
                        Text("Only you see this.")
                    }
                    Section {
                        if b?.state == .blocked {
                            Button("Unblock") { Task { await app.block(login, false) } }
                        } else {
                            Button("Block", role: .destructive) { Task { await app.block(login, true) } }
                        }
                        Button("Remove from Buddy List", role: .destructive) {
                            Task { await app.remove(login); dismiss() }
                        }
                    }
                    Section {
                        Button("Report…") {
                            var c = URLComponents(string: Defaults.reportPage)
                            c?.queryItems = [URLQueryItem(name: "title", value: "Report: \(login)"),
                                             URLQueryItem(name: "body", value: "Screen name: \(login)\n\nWhat happened:\n")]
                            if let url = c?.url { openURL(url) }
                        }
                    } footer: {
                        Text("Block stops their messages; Report tells the HIM project about abuse.")
                    }
                    Section {
                        Button("Clear Conversation History", role: .destructive) { app.clearHistory(login) }
                    } footer: {
                        Text("Conversations are kept on this device only.")
                    }
                } else {
                    Section {
                        Button("Add to Buddy List") { Task { _ = await app.addBuddy(login, note: "") } }
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("Info")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        if alias != (b?.nickname ?? "") { Task { await app.setAlias(login, alias) } }
                        dismiss()
                    }
                }
            }
        }
        .macMinSize(width: 380, height: 440)
        .task {
            alias = b?.nickname ?? ""
            info = await app.info(login)
        }
    }
}
