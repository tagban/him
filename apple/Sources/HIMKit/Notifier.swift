import Foundation
import UserNotifications
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Banners for IMs and requests that arrive while you're elsewhere.
@MainActor
public final class Notifier {
    public static let shared = Notifier()
    private var asked = false

    /// Only a bundled app can post notifications (a bare `swift run` can't).
    private var available: Bool { Bundle.main.bundleIdentifier != nil }

    public var appIsActive: Bool {
        #if os(macOS)
        NSApplication.shared.isActive
        #else
        UIApplication.shared.applicationState == .active
        #endif
    }

    public func requestPermission() {
        guard available, !asked else { return }
        asked = true
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in }
    }

    func post(title: String, body: String, id: String) {
        guard available, !appIsActive else { return }
        let c = UNMutableNotificationContent()
        c.title = title
        c.body = body
        c.sound = .default
        c.threadIdentifier = id
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: id, content: c, trigger: nil))
    }

    func clear(id: String) {
        guard available else { return }
        UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: [id])
    }
}
