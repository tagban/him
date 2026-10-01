//! End-to-end flows against the mock server: every sign-on path, the roster,
//! presence, messages with receipts, offline delivery and friend requests.

use hotline_im::mock::{MockConfig, MockServer};
use hotline_im::*;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

async fn server(cfg: MockConfig) -> MockServer {
    let s = MockServer::start(cfg).await.unwrap();
    for (l, n) in [("alice", "Alice"), ("bob", "Bob"), ("carol", "Carol")] {
        s.add_account(l, "pw", n);
    }
    s.befriend("alice", "bob");
    s
}

fn opts(s: &MockServer, login: &str, security: Security) -> ConnectOptions {
    ConnectOptions {
        host: "127.0.0.1".into(),
        port: s.addr.port(),
        login: login.into(),
        password: "pw".into(),
        nickname: login.into(),
        icon: 0,
        security,
        classic: false,
        media: false,
    }
}

async fn next(ev: &mut UnboundedReceiver<Event>, pred: impl Fn(&Event) -> bool) -> Event {
    loop {
        let e = tokio::time::timeout(Duration::from_secs(5), ev.recv())
            .await
            .expect("event in time")
            .expect("channel open");
        if pred(&e) {
            return e;
        }
    }
}

#[tokio::test]
async fn hope_aead_sign_on_and_messaging() {
    let s = server(MockConfig::default()).await;
    let a = connect(&opts(&s, "alice", Security::Auto)).await.unwrap();
    assert_eq!(a.client.info.transport, "HOPE (ChaCha20-Poly1305)");
    assert!(a.client.info.encrypted && a.client.info.messaging && a.client.info.utf8);
    let mut b = connect(&opts(&s, "bob", Security::HopeEncrypted))
        .await
        .unwrap();
    let (a_client, mut a_ev) = (a.client, a.events);

    a_client
        .set_presence(Presence::Away, "brb", None)
        .await
        .unwrap();
    let roster = b.client.get_roster().await.unwrap();
    let alice = roster.iter().find(|e| e.login == "alice").unwrap();
    assert_eq!(alice.state, RosterState::Accepted);
    assert_eq!(alice.presence, Presence::Away);
    assert_eq!(alice.status_text.as_deref(), Some("brb"));
    assert_eq!(alice.shown_name(), "Alice");

    let guid = messaging_guid();
    let r = b.client.send_im("alice", &guid, "héllo ✨").await.unwrap();
    assert_eq!(r, 0);
    let Event::Message { message } = next(&mut a_ev, |e| matches!(e, Event::Message { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(message.from, "bob");
    assert_eq!(message.body, "héllo ✨");
    assert!(message.timestamp > 0);
    // alice's client acknowledged delivery on its own; bob sees it
    let Event::Ack { ack, .. } = next(&mut b.events, |e| matches!(e, Event::Ack { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(ack, AckKind::Delivered);
    a_client.ack_nowait(&message.guid, "bob", AckKind::Read);
    let Event::Ack { ack, .. } = next(&mut b.events, |e| matches!(e, Event::Ack { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(ack, AckKind::Read);

    // a resend with the same GUID is accepted and not delivered twice
    assert_eq!(
        b.client.send_im("alice", &guid, "héllo ✨").await.unwrap(),
        0
    );

    b.client.typing("alice", true);
    let Event::Typing { typing, login } =
        next(&mut a_ev, |e| matches!(e, Event::Typing { .. })).await
    else {
        unreachable!()
    };
    assert!(typing && login == "bob");

    // presence changes reach friends
    b.client
        .set_presence(Presence::Busy, "", None)
        .await
        .unwrap();
    let Event::Presence { update } = next(
        &mut a_ev,
        |e| matches!(e, Event::Presence { update } if update.login == "bob"),
    )
    .await
    else {
        unreachable!()
    };
    assert_eq!(update.presence, Presence::Busy);
    assert_eq!(update.status_text, None);
}

fn messaging_guid() -> [u8; 16] {
    hotline_im::messaging::new_guid()
}

#[tokio::test]
async fn hope_without_aead_and_mac_login() {
    let s = server(MockConfig {
        aead: false,
        mac_login: true,
        ..Default::default()
    })
    .await;
    let a = connect(&opts(&s, "alice", Security::Auto)).await.unwrap();
    assert_eq!(a.client.info.transport, "HOPE (HMAC-SHA256 sign-in only)");
    assert!(!a.client.info.encrypted);
    assert!(!a.client.info.warnings.is_empty());
    // requiring encryption refuses the same server
    assert!(matches!(
        connect(&opts(&s, "alice", Security::HopeEncrypted)).await,
        Err(Error::Security(_))
    ));
}

#[tokio::test]
async fn legacy_login_and_fallback() {
    let s = server(MockConfig {
        hope: false,
        ..Default::default()
    })
    .await;
    let a = connect(&opts(&s, "alice", Security::Plain)).await.unwrap();
    assert_eq!(a.client.info.transport, "Plaintext");
    // Auto falls back to the legacy login and warns
    let b = connect(&opts(&s, "bob", Security::Auto)).await.unwrap();
    assert_eq!(b.client.info.transport, "Plaintext");
    assert!(!b.client.info.warnings.is_empty());
}

#[tokio::test]
async fn wrong_password_is_refused() {
    let s = server(MockConfig::default()).await;
    let mut o = opts(&s, "alice", Security::Auto);
    o.password = "nope".into();
    match connect(&o).await {
        Err(Error::LoginFailed(t)) => assert_eq!(t, "Incorrect login."),
        other => panic!("expected a refusal, got {:?}", other.map(|_| ())),
    }
}

#[tokio::test]
async fn offline_messages_arrive_after_get_roster() {
    let s = server(MockConfig::default()).await;
    let b = connect(&opts(&s, "bob", Security::Auto)).await.unwrap();
    let code = b
        .client
        .send_im("alice", &messaging_guid(), "while you were out")
        .await
        .unwrap();
    assert_eq!(code, 7, "OfflineQueued");
    let mut a = connect(&opts(&s, "alice", Security::Auto)).await.unwrap();
    a.client
        .set_presence(Presence::Online, "", None)
        .await
        .unwrap();
    a.client.get_roster().await.unwrap();
    let Event::Message { message } =
        next(&mut a.events, |e| matches!(e, Event::Message { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(message.body, "while you were out");
}

#[tokio::test]
async fn friend_request_accept_then_remove() {
    let s = server(MockConfig::default()).await;
    let mut a = connect(&opts(&s, "alice", Security::Auto)).await.unwrap();
    let mut c = connect(&opts(&s, "carol", Security::Auto)).await.unwrap();
    a.client.get_roster().await.unwrap();
    c.client.get_roster().await.unwrap();

    // not friends yet: messages are refused with the server's reason
    match a.client.send_im("carol", &messaging_guid(), "hi").await {
        Err(Error::Server { reason, .. }) => assert_eq!(reason, Some(6)),
        other => panic!("{:?}", other.map(|_| ())),
    }
    assert!(matches!(
        a.client.add_friend("nobody", "").await,
        Err(Error::Server {
            reason: Some(1),
            ..
        })
    ));

    a.client.add_friend("carol", "it's Alice").await.unwrap();
    let Event::FriendRequest { login, note } =
        next(&mut c.events, |e| matches!(e, Event::FriendRequest { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(
        (login.as_str(), note.as_deref()),
        ("alice", Some("it's Alice"))
    );
    c.client.respond("alice", true).await.unwrap();
    let Event::RosterEntry { entry } = next(&mut a.events, |e| matches!(e, Event::RosterEntry { entry } if entry.login == "carol" && entry.state == RosterState::Accepted)).await else { unreachable!() };
    assert_eq!(entry.display_name.as_deref(), Some("Carol"));
    assert_eq!(entry.presence, Presence::Online);

    a.client.set_alias("carol", "Caz").await.unwrap();
    let Event::RosterEntry { entry } = next(
        &mut a.events,
        |e| matches!(e, Event::RosterEntry { entry } if entry.nickname.is_some()),
    )
    .await
    else {
        unreachable!()
    };
    assert_eq!(entry.shown_name(), "Caz");

    c.client.remove_friend("alice").await.unwrap();
    next(&mut a.events, |e| matches!(e, Event::RosterEntry { entry } if entry.login == "carol" && entry.state == RosterState::Removed)).await;
}

#[tokio::test]
async fn agreement_and_disconnect() {
    let s = server(MockConfig {
        agreement: Some("Be nice.".into()),
        ..Default::default()
    })
    .await;
    let mut a = connect(&opts(&s, "alice", Security::Auto)).await.unwrap();
    let Event::Agreement { text } =
        next(&mut a.events, |e| matches!(e, Event::Agreement { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(text, "Be nice.");
    a.client.agree("alice", 0).await.unwrap();
    a.client.disconnect();
    next(&mut a.events, |e| matches!(e, Event::Disconnected { .. })).await;
    assert!(matches!(
        a.client.get_roster().await,
        Err(Error::Closed) | Err(Error::Timeout)
    ));
}

fn guest(s: &MockServer, nick: &str) -> ConnectOptions {
    ConnectOptions {
        host: "127.0.0.1".into(),
        port: s.addr.port(),
        login: String::new(),
        password: String::new(),
        nickname: nick.into(),
        icon: 0,
        security: Security::Auto,
        classic: true,
        media: false,
    }
}

#[tokio::test]
async fn guests_chat_in_a_room_and_messengers_stay_hidden() {
    let s = server(MockConfig {
        agreement: Some("Rules: be kind.".into()),
        ..Default::default()
    })
    .await;
    // a pure messenger is on the server too, but must not show up in the room
    let _m = connect(&opts(&s, "alice", Security::Auto)).await.unwrap();
    let mut a = connect(&guest(&s, "RoomieA")).await.unwrap();
    assert_eq!(a.client.info.transport, "Plaintext");
    assert!(!a.client.info.messaging);
    let Event::Agreement { text } =
        next(&mut a.events, |e| matches!(e, Event::Agreement { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(text, "Rules: be kind.");
    a.client.agree_nowait("RoomieA", 0);

    let mut b = connect(&guest(&s, "RoomieB")).await.unwrap();
    let Event::UserChanged { user } =
        next(&mut a.events, |e| matches!(e, Event::UserChanged { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(user.name, "RoomieB");
    let users = b.client.get_users().await.unwrap();
    let names: Vec<_> = users.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(names, vec!["RoomieA", "RoomieB"]);

    b.client.send_chat("hello room ✨", false);
    let Event::ChatMessage { text, chat_id, .. } =
        next(&mut a.events, |e| matches!(e, Event::ChatMessage { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(chat_id, None);
    assert_eq!(text.trim(), "RoomieB:  hello room ✨");
    b.client.send_chat("waves", true);
    let Event::ChatMessage { text, .. } =
        next(&mut a.events, |e| matches!(e, Event::ChatMessage { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(text.trim(), "*** RoomieB waves");
    next(&mut b.events, |e| matches!(e, Event::ChatMessage { .. })).await;

    let bid = users.iter().find(|u| u.name == "RoomieB").unwrap().id;
    b.client.disconnect();
    let Event::UserLeft { id } = next(&mut a.events, |e| matches!(e, Event::UserLeft { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(id, bid);
}

#[tokio::test]
async fn tracker_lists_merge_and_sort() {
    use hotline_im::tracker;
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (a1, a2) = (l1.local_addr().unwrap(), l2.local_addr().unwrap());
    let ip = std::net::Ipv4Addr::new(10, 0, 0, 5);
    tokio::spawn(tracker::serve_mock(
        l1,
        vec![
            (ip, 5500, 3, "Quiet".into(), "a small place".into()),
            (
                std::net::Ipv4Addr::new(10, 0, 0, 6),
                5500,
                9,
                "Busy".into(),
                "".into(),
            ),
        ],
    ));
    tokio::spawn(tracker::serve_mock(
        l2,
        vec![(ip, 5500, 4, "Quiet".into(), "".into())],
    ));
    let trackers = vec![a1.to_string(), a2.to_string(), "127.0.0.1:1".to_string()];
    let (list, failed) = tracker::query_all(&trackers, Duration::from_secs(3)).await;
    assert_eq!(failed.len(), 1);
    assert_eq!(
        list.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        vec!["Busy", "Quiet"]
    );
    let quiet = &list[1];
    assert_eq!(
        (
            quiet.users,
            quiet.trackers.len(),
            quiet.description.as_str()
        ),
        (4, 2, "a small place")
    );
}

/// A 1-frame GIF whose screen is `w` x `h`.
fn gif(w: u16, h: u16, color: u8) -> Vec<u8> {
    let mut g = b"GIF89a".to_vec();
    g.extend(w.to_le_bytes());
    g.extend(h.to_le_bytes());
    g.extend([0x80, 0, 0, color, 0, 0, 255, 255, 255]);
    g.extend([0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x4C, 0x01, 0, 0x3B]);
    g
}

#[tokio::test]
async fn buddy_icons() {
    let s = server(MockConfig::default()).await;
    let a = connect(&opts(&s, "alice", Security::Auto)).await.unwrap();
    let a2 = connect(&opts(&s, "alice", Security::Auto)).await.unwrap();
    let mut b = connect(&opts(&s, "bob", Security::Auto)).await.unwrap();
    let c = connect(&opts(&s, "carol", Security::Auto)).await.unwrap();
    let (a, mut a2_ev) = (a.client, a2.events);
    assert_eq!(a.info.limits.max_icon_bytes, Some(16384));
    assert_eq!(a.info.limits.max_icon_dimension, 128);
    b.client.get_roster().await.unwrap();

    // Set: friends get a complete 809 with the hash; our other session gets the 827 echo.
    let pic = gif(48, 48, 1);
    let hash = a.set_buddy_icon(&pic).await.unwrap().unwrap();
    assert_eq!(hash, messaging::hex(&icon::hash(&pic)));
    let Event::Presence { update } = next(
        &mut b.events,
        |e| matches!(e, Event::Presence { update } if update.login == "alice"),
    )
    .await
    else {
        unreachable!()
    };
    assert_eq!(update.icon_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(update.presence, Presence::Online);
    let Event::OwnIconChanged { hash: echoed } =
        next(&mut a2_ev, |e| matches!(e, Event::OwnIconChanged { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(echoed.as_deref(), Some(hash.as_str()));

    // Fetch: friends and ourselves get it; strangers are told NotFriends, no picture.
    let (h, got) = b.client.get_buddy_icon("alice").await.unwrap().unwrap();
    assert_eq!((h.as_str(), got.as_slice()), (hash.as_str(), pic.as_slice()));
    assert!(a2.client.get_buddy_icon("alice").await.unwrap().is_some());
    assert!(c.client.get_buddy_icon("alice").await.unwrap().is_none());

    // The roster and our own 825 carry the hash; a stranger's 825 doesn't.
    let roster = b.client.get_roster().await.unwrap();
    let alice = roster.iter().find(|e| e.login == "alice").unwrap();
    assert_eq!(alice.icon_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(a2.client.get_info("alice").await.unwrap().icon_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(c.client.get_info("alice").await.unwrap().icon_hash, None);

    // Presence changes carry the hash too (809 is complete).
    a.set_presence(Presence::Away, "out", None).await.unwrap();
    let Event::Presence { update } = next(
        &mut b.events,
        |e| matches!(e, Event::Presence { update } if update.status_text.as_deref() == Some("out")),
    )
    .await
    else {
        unreachable!()
    };
    assert_eq!(update.icon_hash.as_deref(), Some(hash.as_str()));

    // Refusals: too many pixels, not a picture, too many bytes.
    let err = |e: Error| match e {
        Error::Server { reason, .. } => reason,
        other => panic!("{other}"),
    };
    assert_eq!(err(a.set_buddy_icon(&gif(200, 200, 1)).await.unwrap_err()), Some(14));
    assert_eq!(err(a.set_buddy_icon(b"not a picture").await.unwrap_err()), Some(14));
    assert_eq!(err(a.set_buddy_icon(&vec![0u8; 20000]).await.unwrap_err()), Some(13));

    // While Invisible, a change reaches our other session but not friends.
    a.set_presence(Presence::Invisible, "", None).await.unwrap();
    a2.client.set_presence(Presence::Invisible, "", None).await.unwrap();
    next(
        &mut b.events,
        |e| matches!(e, Event::Presence { update } if update.presence == Presence::Offline),
    )
    .await;
    let pic2 = gif(32, 32, 2);
    let hash2 = a.set_buddy_icon(&pic2).await.unwrap().unwrap();
    let Event::OwnIconChanged { hash: echoed } =
        next(&mut a2_ev, |e| matches!(e, Event::OwnIconChanged { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(echoed.as_deref(), Some(hash2.as_str()));
    // Coming back announces the new hash.
    a.set_presence(Presence::Online, "", None).await.unwrap();
    let Event::Presence { update } = next(&mut b.events, |e| {
        matches!(e, Event::Presence { update } if update.login == "alice")
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(update.presence, Presence::Online, "no 809 went out while invisible");
    assert_eq!(update.icon_hash.as_deref(), Some(hash2.as_str()));

    // Clearing: the echo has no hash, and friends' 809 drops it.
    assert_eq!(a.set_buddy_icon(&[]).await.unwrap(), None);
    let Event::OwnIconChanged { hash: echoed } =
        next(&mut a2_ev, |e| matches!(e, Event::OwnIconChanged { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(echoed, None);
    let Event::Presence { update } = next(&mut b.events, |e| {
        matches!(e, Event::Presence { update } if update.login == "alice")
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(update.icon_hash, None);
}

#[tokio::test]
async fn no_buddy_icons_without_the_limit() {
    let s = server(MockConfig {
        max_icon_bytes: None,
        ..Default::default()
    })
    .await;
    let a = connect(&opts(&s, "alice", Security::Auto)).await.unwrap();
    assert!(!a.client.has_buddy_icons());
    assert!(a.client.set_buddy_icon(&gif(48, 48, 1)).await.is_err());
}

#[tokio::test]
async fn pictures_and_gif_icons_in_a_room() {
    let s = server(MockConfig::default()).await;
    let with_media = |nick: &str| ConnectOptions { media: true, ..guest(&s, nick) };
    let mut a = hotline_im::connect(&with_media("PicA")).await.unwrap();
    let b = hotline_im::connect(&with_media("PicB")).await.unwrap();
    let mut old = hotline_im::connect(&guest(&s, "OldC")).await.unwrap();
    assert!(a.client.media_limits().is_some());
    assert!(old.client.media_limits().is_none());

    // A picture bigger than one chunk goes up in parts and comes back whole.
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend((0..10_000u32).map(|i| (i * 7) as u8));
    let m = b.client.upload_media(&png).await.unwrap();
    assert_eq!(m.mime, "image/png");
    b.client.send_chat_media("[image]", &m);
    let Event::ChatMessage { text, media, .. } =
        next(&mut a.events, |e| matches!(e, Event::ChatMessage { .. })).await
    else {
        unreachable!()
    };
    assert!(text.ends_with("[image]"));
    let got = media.expect("capable clients get the picture");
    let (bytes, mime) = a.client.download_media(&got.id).await.unwrap();
    assert_eq!((bytes, mime.as_str()), (png.clone(), "image/png"));
    // A client that didn't ask for pictures gets just the text.
    let Event::ChatMessage { media, .. } =
        next(&mut old.events, |e| matches!(e, Event::ChatMessage { .. })).await
    else {
        unreachable!()
    };
    assert!(media.is_none());
    // Not a picture: refused.
    assert!(b.client.upload_media(b"hello, this is not an image at all, really not").await.is_err());

    // GIF icons: set, announced, fetched one at a time and as a list.
    let gif = b"GIF89a\x01\x00\x01\x00tiny".to_vec();
    b.client.set_gif_icon(&gif).await.unwrap();
    let Event::GifIconChanged { user_id } =
        next(&mut a.events, |e| matches!(e, Event::GifIconChanged { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(user_id, b.client.info.user_id);
    assert_eq!(a.client.gif_icon(user_id).await.unwrap(), Some(gif.clone()));
    assert!(a.client.gif_icons().await.unwrap().contains(&(user_id, gif)));
    assert_eq!(a.client.gif_icon(old.client.info.user_id).await.unwrap(), None);
}
