//! A scripted second person for trying the app against the mock server:
//! `cargo run -p hotline-im --example buddy -- bob hotline alice`
//! signs on, says hello to the third argument, goes away, then signs off.

use hotline_im::{connect, ConnectOptions, Presence, Security};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (login, password, to) = (
        a.first().cloned().unwrap_or("bob".into()),
        a.get(1).cloned().unwrap_or("hotline".into()),
        a.get(2).cloned().unwrap_or("alice".into()),
    );
    let port = a.get(3).and_then(|p| p.parse().ok()).unwrap_or(5500);
    let opts = ConnectOptions {
        host: "127.0.0.1".into(),
        port,
        login: login.clone(),
        password,
        nickname: login.clone(),
        icon: 0,
        security: Security::Auto,
        classic: false,
        media: false,
    };
    let mut s = connect(&opts).await.expect("sign on");
    println!("{login} signed on via {}", s.client.info.transport);
    s.client
        .set_presence(Presence::Online, "", None)
        .await
        .unwrap();
    s.client.get_roster().await.unwrap();
    let c = s.client.clone();
    let c2 = s.client.clone();
    tokio::spawn(async move {
        while let Some(e) = s.events.recv().await {
            println!("event: {e:?}");
            // accept every buddy request, like a friendly person would
            if let hotline_im::Event::FriendRequest { login, .. } = &e {
                println!(
                    "accepting {login}: {:?}",
                    c2.respond(login, true).await.map(|_| ())
                );
            }
        }
    });
    let pause = |secs| tokio::time::sleep(Duration::from_secs(secs));
    let wait: u64 = std::env::var("BUDDY_WAIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    pause(wait).await;
    c.typing(&to, true);
    pause(2).await;
    let code = c
        .send_im(
            &to,
            &hotline_im::messaging::new_guid(),
            "hey! long time no see :-)",
        )
        .await;
    println!("sent: {code:?}");
    pause(4).await;
    c.set_presence(Presence::Away, "At lunch, back in 20", None)
        .await
        .unwrap();
    println!("away");
    pause(6).await;
    c.disconnect();
    println!("signed off");
    pause(1).await;
}
