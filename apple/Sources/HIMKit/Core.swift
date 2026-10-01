import Foundation
import HIMCore

/// The core calls `onEvent` on its own threads; this hands events to the main actor in order.
final class EventPipe: EventListener, @unchecked Sendable {
    let stream: AsyncStream<HimEvent>
    private let continuation: AsyncStream<HimEvent>.Continuation

    init() {
        (stream, continuation) = AsyncStream.makeStream(of: HimEvent.self)
    }

    func onEvent(event: HimEvent) {
        continuation.yield(event)
    }

    func finish() {
        continuation.finish()
    }
}

extension HimError {
    /// What to show a person.
    public var text: String {
        switch self {
        case .Connect(let m), .LoginFailed(let m), .Security(let m), .Server(let m, _): m
        case .Timeout: "The server didn't answer in time."
        case .NotConnected: "You're not signed on."
        }
    }
}

/// A person-readable message for any error.
func describe(_ error: Error) -> String {
    (error as? HimError)?.text ?? error.localizedDescription
}

extension Presence {
    public var label: String {
        switch self {
        case .online: "Available"
        case .away: "Away"
        case .busy: "Busy"
        case .invisible: "Invisible"
        case .offline: "Offline"
        }
    }

    public var isOnline: Bool { self != .offline }
}

extension Buddy {
    /// Alias, then the name they go by, then the screen name.
    public var shownName: String {
        [nickname, displayName].compactMap { $0 }.first { !$0.trimmingCharacters(in: .whitespaces).isEmpty } ?? login
    }
}
