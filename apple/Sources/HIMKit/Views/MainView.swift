import SwiftUI

/// The whole app: sign on, then everything else.
public struct RootView: View {
    @Environment(AppModel.self) private var app

    public init() {}

    public var body: some View {
        Group {
            switch app.phase {
            case .signedOff, .signingOn: SignOnView()
            case .online, .reconnecting, .guest: MainView()
            }
        }
        .environment(app.icons)
        .environment(app.classicIcons)
        .tint(Brand.accent)
        .animation(.smooth, value: app.phase == .signedOff)
        .onAppear { Notifier.shared.requestPermission() }
    }
}

enum Tab: String, CaseIterable, Identifiable {
    case chats = "Chats", buddies = "Buddies", rooms = "Rooms"
    var id: String { rawValue }
    var symbol: String {
        switch self {
        case .chats: "bubble.left.and.bubble.right"
        case .buddies: "person.2"
        case .rooms: "number"
        }
    }
}

struct MainView: View {
    @Environment(AppModel.self) private var app
    @State private var tab: Tab = .buddies
    @State private var editingMe = false
    @State private var addingBuddy = false
    @State private var browsing = false
    @State private var filter = ""

    var body: some View {
        content
            .sheet(isPresented: $editingMe) { ProfileView() }
            .sheet(isPresented: $addingBuddy) { AddBuddyView() }
            .sheet(isPresented: $browsing) { RoomBrowser() }
            .onAppear { if app.phase == .guest { tab = .rooms } }
            .overlay(alignment: .top) { NoticeBanner() }
    }

    #if os(macOS)
    @ViewBuilder private var content: some View {
        @Bindable var app = app
        NavigationSplitView {
            VStack(spacing: 0) {
                MeHeader(editing: $editingMe)
                    .padding(.horizontal, 14)
                    .padding(.top, 6)
                    .padding(.bottom, 10)
                Picker("", selection: $tab) {
                    ForEach(availableTabs) { t in Text(t.rawValue).tag(t) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .padding(.horizontal, 12)
                .padding(.bottom, 8)
                List(selection: $app.target) { listContent }
                    .listStyle(.sidebar)
            }
            .navigationSplitViewColumnWidth(min: 250, ideal: 290, max: 380)
            .toolbar {
                ToolbarItem {
                    Button { tab == .rooms ? (browsing = true) : (addingBuddy = true) } label: {
                        Label(tab == .rooms ? "Find a Room" : "Add a Buddy", systemImage: tab == .rooms ? "plus.bubble" : "person.badge.plus")
                    }
                    .disabled(tab != .rooms && app.phase == .guest)
                }
            }
        } detail: {
            NavigationStack { detail }
        }
        .onChange(of: app.target) { _, t in
            if case .im = t, tab == .rooms { tab = .chats }
        }
    }
    #else
    @ViewBuilder private var content: some View {
        TabView(selection: $tab) {
            ForEach(availableTabs) { t in
                NavigationStack {
                    List {
                        if t == .buddies {
                            MeHeader(editing: $editingMe).listRowBackground(Color.clear)
                        }
                        listContent(for: t)
                    }
                    .navigationTitle(t.rawValue)
                    .navigationDestination(for: Target.self) { target in destination(target) }
                    .toolbar {
                        ToolbarItem(placement: .primaryAction) {
                            if t == .rooms {
                                Button { browsing = true } label: { Label("Find a Room", systemImage: "plus") }
                            } else if app.phase != .guest {
                                Button { addingBuddy = true } label: { Label("Add a Buddy", systemImage: "person.badge.plus") }
                            }
                        }
                    }
                    .searchable(text: $filter, prompt: "Search")
                }
                .tabItem { Label(t.rawValue, systemImage: t.symbol) }
                .badge(t == .chats ? app.unreadTotal : 0)
                .tag(t)
            }
        }
    }
    #endif

    private var availableTabs: [Tab] {
        app.phase == .guest ? [.rooms] : Tab.allCases
    }

    @ViewBuilder private var listContent: some View {
        listContent(for: tab)
    }

    @ViewBuilder private func listContent(for t: Tab) -> some View {
        switch t {
        case .chats: rows { ConversationSections() }
        case .buddies: rows { BuddySections(filter: filter) }
        case .rooms: rows { RoomSections(browsing: $browsing) }
        }
    }

    @ViewBuilder private func rows<C: View>(@ViewBuilder _ c: () -> C) -> some View {
        c()
    }

    @ViewBuilder private var detail: some View {
        switch app.target {
        case .im(let login)?: destination(.im(login))
        case .room(let id)?: destination(.room(id))
        case nil:
            ContentUnavailableView {
                Label(app.phase == .guest ? "Pick a chat room" : "Pick someone to talk to", systemImage: "bubble.left.and.bubble.right")
            } description: {
                Text(app.phase == .guest ? "Rooms are Hotline servers' public chats." : "Your conversations show up here.")
            }
        }
    }

    @ViewBuilder private func destination(_ t: Target) -> some View {
        switch t {
        case .im(let login): ConversationView(login: login).id(login)
        case .room(let id):
            if let r = app.rooms.room(id) { RoomView(room: r).id(id) } else { ContentUnavailableView("You left that room", systemImage: "number") }
        }
    }
}

/// A passing message from the server, at the top of the window.
struct NoticeBanner: View {
    @Environment(AppModel.self) private var app

    var body: some View {
        if let n = app.notice {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: "megaphone.fill").foregroundStyle(Color.accentColor)
                Text(n).font(.callout).textSelection(.enabled)
                Spacer(minLength: 0)
                Button { app.notice = nil } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary) }
                    .buttonStyle(.plain)
            }
            .padding(12)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .shadow(color: .black.opacity(0.12), radius: 12, y: 4)
            .padding(.horizontal, 16)
            .padding(.top, 8)
            .frame(maxWidth: 520)
            .transition(.move(edge: .top).combined(with: .opacity))
        }
    }
}
