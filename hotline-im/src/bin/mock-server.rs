//! A local Hotline IM server for trying the app: `cargo run -p hotline-im
//! --features mock-server --bin mock-server [port]`. Accounts alice, bob and
//! carol (password "hotline"), plus HotBot, who answers everything.

use hotline_im::mock::{MockConfig, MockServer};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(5500);
    let cfg = MockConfig {
        bind: format!("127.0.0.1:{port}").parse().unwrap(),
        info_port: true,
        server_name: "HIM Test Server".into(),
        agreement: std::env::var("MOCK_AGREEMENT").ok(),
        ..Default::default()
    };
    let srv = MockServer::start(cfg).await?;
    for (login, name) in [
        ("alice", "Alice"),
        ("bob", "Bob"),
        ("carol", "Carol"),
        ("dave", "Dave"),
    ] {
        srv.add_account(login, "hotline", name);
    }
    srv.befriend("alice", "bob");
    srv.befriend("alice", "carol");
    srv.befriend("bob", "carol");
    srv.add_bot("hotbot", "HotBot", "Ask me anything!", |from, body| {
        let b = body.trim().to_lowercase();
        if b.contains("hello") || b.contains("hi") || b == "hey" {
            format!("Hi {from}! I'm HotBot. Say \"time\" or anything else and I'll repeat it back.")
        } else if b.contains("time") {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            format!("By my clock it's {} seconds past 1970. Very precise.", secs)
        } else {
            format!("You said: {body}")
        }
    });
    srv.befriend("alice", "hotbot");
    // MOCK_ICONS="hotbot=/path/robot.gif,bob=/path/b.png" gives accounts Buddy Icons.
    for pair in std::env::var("MOCK_ICONS").unwrap_or_default().split(',') {
        if let Some((login, path)) = pair.split_once('=') {
            match std::fs::read(path) {
                Ok(pic) => srv.set_icon(login.trim(), pic),
                Err(e) => eprintln!("{path}: {e}"),
            }
        }
    }
    srv.befriend("bob", "hotbot");
    println!(
        "mock Hotline IM server on {} (info port {})",
        srv.addr,
        port - 1
    );
    println!("accounts: alice, bob, carol, dave / password hotline; hotbot answers messages");
    std::future::pending::<()>().await;
    Ok(())
}
