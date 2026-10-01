import AppKit
import HIMKit
import SwiftUI

@main
struct HIMApp: App {
    @State private var app = AppModel()

    init() {
        // Run straight from `swift run` too: a regular app with a Dock icon and a menu bar.
        NSApplication.shared.setActivationPolicy(.regular)
    }

    var body: some Scene {
        WindowGroup("HIM") {
            RootView()
                .environment(app)
                .frame(minWidth: 720, minHeight: 520)
                .task { await TestHooks.run(app) }
        }
        .defaultSize(width: 980, height: 680)
        .commands {
            CommandGroup(replacing: .newItem) {}
            CommandMenu("Account") {
                Button("Sign Off") { app.signOff() }
                    .disabled(app.phase == .signedOff)
            }
        }
    }
}
