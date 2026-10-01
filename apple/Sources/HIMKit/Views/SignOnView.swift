import SwiftUI

public struct SignOnView: View {
    @Environment(AppModel.self) private var app
    @Environment(\.openURL) private var openURL
    @State private var login = ""
    @State private var password = ""
    @State private var host = Defaults.host
    @State private var port = String(Defaults.port)
    @State private var savePassword = true
    @State private var autoSignOn = false
    @State private var showServer = false
    @State private var hasSavedPassword = false
    /// First run: no account saved yet, so start by offering to make one.
    @State private var welcome = Settings.account == nil
    @State private var madeOne = false
    @FocusState private var focus: Field?
    private enum Field { case login, password }

    public init() {}

    private var busy: Bool {
        if case .signingOn = app.phase { return true }
        return false
    }

    public var body: some View {
        Group {
            if welcome { welcomeView.transition(.opacity) } else { form.transition(.opacity) }
        }
        .animation(.smooth, value: welcome)
        .background(backdrop)
        .onAppear(perform: load)
    }

    private var welcomeView: some View {
        ScrollView {
            VStack(spacing: 26) {
                VStack(spacing: 12) {
                    AppMark(size: 96)
                    Text("Welcome to HIM")
                        .font(.system(size: 32, weight: .bold, design: .rounded))
                    Text("Instant messages, buddies and chat rooms on the Hotline network.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
                .padding(.top, 48)

                VStack(alignment: .leading, spacing: 10) {
                    Label {
                        VStack(alignment: .leading, spacing: 1) {
                            Text(Defaults.serverName(host)).fontWeight(.semibold)
                            Text(host).font(.caption).foregroundStyle(.secondary)
                        }
                    } icon: {
                        Image(systemName: "server.rack").foregroundStyle(Brand.accent)
                    }
                    DisclosureGroup("Use a different server", isExpanded: $showServer) { serverFields }
                        .font(.callout)
                }
                .padding(14)
                .frame(maxWidth: 360, alignment: .leading)
                .background(.background.secondary, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(.separator))

                VStack(spacing: 10) {
                    if let url = Defaults.signupPage(for: host) {
                        Button {
                            openURL(url)
                            madeOne = true
                            welcome = false
                        } label: {
                            Text("Create a Screen Name").font(.headline).frame(maxWidth: .infinity).padding(.vertical, 6)
                        }
                        .buttonStyle(.borderedProminent)
                        .buttonBorderShape(.roundedRectangle(radius: 12))
                        .controlSize(.large)
                        Text("It's free, and opens \(Defaults.serverName(host))'s sign-up page.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else {
                        Text("\(host) makes its own accounts: ask its admin for a screen name.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                    }
                    Button { welcome = false } label: {
                        Text("I Have a Screen Name").frame(maxWidth: .infinity).padding(.vertical, 4)
                    }
                    .buttonStyle(.bordered)
                    .buttonBorderShape(.roundedRectangle(radius: 12))
                    .controlSize(.large)
                    Button("Just browse chat rooms") { app.roomsOnly() }
                        .buttonStyle(.borderless)
                        .font(.callout)
                        .padding(.top, 4)
                }
                .frame(maxWidth: 360)
            }
            .padding(24)
            .frame(maxWidth: .infinity)
        }
        .scrollBounceBehavior(.basedOnSize)
        .scrollDismissesKeyboard(.interactively)
    }

    private var serverFields: some View {
        HStack {
            TextField("Server", text: $host)
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
                #endif
            TextField("Port", text: $port)
                .frame(width: 64)
                #if os(iOS)
                .keyboardType(.numberPad)
                #endif
        }
        .textFieldStyle(.roundedBorder)
        .padding(.top, 6)
    }

    private var form: some View {
        ScrollView {
            VStack(spacing: 28) {
                VStack(spacing: 12) {
                    AppMark(size: 84)
                    Text("HIM")
                        .font(.system(size: 34, weight: .bold, design: .rounded))
                    Text("Hotline Instant Messenger")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .padding(.top, 36)

                VStack(spacing: 12) {
                    if madeOne {
                        Label("Once you've made your screen name, sign on with it here.", systemImage: "checkmark.circle.fill")
                            .font(.callout)
                            .foregroundStyle(.green)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 4)
                    }
                    VStack(spacing: 0) {
                        field {
                            Image(systemName: "person.fill").foregroundStyle(.secondary).frame(width: 20)
                            TextField("Screen name", text: $login)
                                .textContentType(.username)
                                .focused($focus, equals: .login)
                                .onSubmit { focus = .password }
                                #if os(iOS)
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                                #endif
                        }
                        Divider().padding(.leading, 44)
                        field {
                            Image(systemName: "lock.fill").foregroundStyle(.secondary).frame(width: 20)
                            SecureField(hasSavedPassword && password.isEmpty ? "Saved password" : "Password", text: $password)
                                .textContentType(.password)
                                .focused($focus, equals: .password)
                                .onSubmit(go)
                        }
                    }
                    .background(.background.secondary, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(.separator))

                    HStack {
                        Toggle("Remember password", isOn: $savePassword)
                        Spacer()
                        Toggle("Sign on automatically", isOn: $autoSignOn)
                    }
                    .font(.callout)
                    .toggleStyle(.switch)
                    .controlSize(.small)
                    .padding(.horizontal, 4)

                    if let err = app.signOnError {
                        Label(err, systemImage: "exclamationmark.triangle.fill")
                            .font(.callout)
                            .foregroundStyle(.red)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 4)
                            .transition(.opacity)
                    }

                    Button(action: go) {
                        Group {
                            if case .signingOn(let step) = app.phase {
                                HStack(spacing: 8) {
                                    ProgressView().controlSize(.small)
                                    Text(step)
                                }
                            } else {
                                Text("Sign On")
                            }
                        }
                        .font(.headline)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                    }
                    .buttonStyle(.borderedProminent)
                    .buttonBorderShape(.roundedRectangle(radius: 12))
                    .controlSize(.large)
                    .disabled(busy || login.trimmingCharacters(in: .whitespaces).isEmpty)
                    .keyboardShortcut(.defaultAction)
                }
                .frame(maxWidth: 360)

                VStack(spacing: 10) {
                    DisclosureGroup(isExpanded: $showServer) {
                        serverFields
                    } label: {
                        Label("Server: \(Defaults.serverName(host))", systemImage: "server.rack")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }

                    HStack(spacing: 16) {
                        if let url = Defaults.signupPage(for: host) {
                            Button("Get a screen name") { openURL(url) }
                        }
                        Button("Chat rooms only") { app.roomsOnly() }
                    }
                    .buttonStyle(.borderless)
                    .font(.callout)
                }
                .frame(maxWidth: 360)
            }
            .padding(24)
            .frame(maxWidth: .infinity)
            .animation(.snappy, value: app.signOnError)
        }
        .scrollBounceBehavior(.basedOnSize)
        .scrollDismissesKeyboard(.interactively)
    }

    private var backdrop: some View {
        LinearGradient(colors: [Color.indigo.opacity(0.10), Color.blue.opacity(0.04), .clear],
                       startPoint: .top, endPoint: .bottom)
            .ignoresSafeArea()
    }

    private func field<C: View>(@ViewBuilder _ c: () -> C) -> some View {
        HStack(spacing: 12) { c() }
            .textFieldStyle(.plain)
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
    }

    private func load() {
        guard let a = app.account else { focus = .login; return }
        login = a.login
        host = a.host
        port = String(a.port)
        savePassword = a.savePassword
        autoSignOn = a.autoSignOn
        hasSavedPassword = app.savedPassword(for: a) != nil
        focus = hasSavedPassword ? nil : .password
        if a.autoSignOn, hasSavedPassword, app.signOnError == nil { go() }
    }

    private func go() {
        guard !busy else { return }
        let p = UInt16(port.trimmingCharacters(in: .whitespaces)) ?? Defaults.port
        Task {
            await app.signOn(login: login, password: password, host: host, port: p,
                             savePassword: savePassword, autoSignOn: autoSignOn)
            if app.signOnError == nil { password = "" }
        }
    }
}
