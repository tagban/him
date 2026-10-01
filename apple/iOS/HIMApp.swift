import HIMKit
import SwiftUI

@main
struct HIMApp: App {
    @State private var app = AppModel()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(app)
                .task { await TestHooks.run(app) }
        }
    }
}
