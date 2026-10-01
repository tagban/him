//! Joins a real server as a silent guest, lists who's there, listens briefly, leaves.
//! `cargo run -p hotline-im --example probe_room -- host [port] [nick]` (sends no chat)

use hotline_im::{connect, ConnectOptions, Event, Security};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let host = a.first().cloned().expect("host");
    let port = a.get(1).and_then(|p| p.parse().ok()).unwrap_or(5500);
    let nick = a.get(2).cloned().unwrap_or_else(|| "HIM test".into());
    let opts = ConnectOptions {
        host,
        port,
        login: String::new(),
        password: String::new(),
        nickname: nick.clone(),
        icon: 0,
        security: match std::env::var("PROBE_SEC").as_deref() {
            Ok("plain") => Security::Plain,
            Ok("tls") => Security::Tls,
            _ => Security::Auto,
        },
        classic: true,
        media: false,
        history: false,
    };
    if let Some(d) = hotline_im::info::probe(&opts.host, opts.port - 1).await {
        println!(
            "info port: data {} tls {:?} tls.supported {} hope {:?} hostname {:?}",
            d.data_port,
            d.tls_port,
            d.transport.tls.supported,
            d.transport.hope.supported,
            d.server.as_ref().and_then(|s| s.hostname.clone())
        );
    } else {
        println!("no info port");
    }
    let mut s = match connect(&opts).await {
        Ok(s) => s,
        Err(e) => return println!("join failed: {e}"),
    };
    let i = &s.client.info;
    println!(
        "server: {:?} v{:?} via {} caps={:#06x} utf8={} warnings={:?}",
        i.server_name, i.server_version, i.transport, i.caps, i.utf8, i.warnings
    );
    let c = s.client.clone();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let mut agreed = false;
    loop {
        tokio::select! {
            e = s.events.recv() => match e {
                Some(Event::Agreement { text }) => { println!("agreement: {} chars", text.len()); c.agree_nowait(&nick, 0); agreed = true; }
                Some(Event::ChatMessage { text, .. }) => println!("chat: {}", text.trim()),
                Some(Event::UserChanged { user }) => println!("user changed: {} flags={}", user.name, user.flags),
                Some(Event::Disconnected { reason }) => { println!("disconnected: {reason}"); break; }
                Some(other) => println!("event: {other:?}"),
                None => break,
            },
            _ = tokio::time::sleep_until(deadline) => break,
        }
        if agreed {
            agreed = false;
            match c.get_users().await {
                Ok(u) => println!(
                    "users ({}): {:?}",
                    u.len(),
                    u.iter().map(|x| x.name.as_str()).collect::<Vec<_>>()
                ),
                Err(e) => println!("user list refused: {e}"),
            }
        }
    }
    if let Ok(u) = c.get_users().await {
        println!("users at end: {}", u.len());
    }
    c.disconnect();
    tokio::time::sleep(Duration::from_millis(300)).await;
}
