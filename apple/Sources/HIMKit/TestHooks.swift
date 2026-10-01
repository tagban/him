import Foundation

/// For trying the app against the local test server without clicking through it:
///   HIM_TEST_SIGNON=alice:hotline@127.0.0.1:15500  sign on (password not saved)
///   HIM_TEST_OPEN=hotbot   open a conversation;  HIM_TEST_SAY=hello   say something there
///   HIM_TEST_ROOM=127.0.0.1:15500   join a chat room
@MainActor
public enum TestHooks {
    public static func run(_ app: AppModel) async {
        let env = ProcessInfo.processInfo.environment
        if let spec = env["HIM_TEST_SIGNON"], let at = spec.lastIndex(of: "@") {
            let cred = spec[..<at].split(separator: ":", maxSplits: 1).map(String.init)
            let addr = spec[spec.index(after: at)...].split(separator: ":").map(String.init)
            await app.signOn(login: cred[0], password: cred.count > 1 ? cred[1] : "",
                             host: addr[0], port: addr.count > 1 ? UInt16(addr[1]) ?? 5500 : 5500,
                             savePassword: false, autoSignOn: false)
        }
        if let room = env["HIM_TEST_ROOM"] {
            let a = room.split(separator: ":").map(String.init)
            if app.phase == .signedOff { app.roomsOnly() }
            app.rooms.join(host: a[0], port: a.count > 1 ? UInt16(a[1]) ?? 5500 : 5500, title: a[0])
        }
        if let who = env["HIM_TEST_OPEN"] {
            app.open(who)
            if let say = env["HIM_TEST_SAY"] { app.send(say, to: who) }
        }
    }
}
