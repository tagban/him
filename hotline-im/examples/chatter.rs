//! A scripted guest in a mock chat room: `cargo run -p hotline-im --example chatter -- Nick`
//! joins 127.0.0.1:5500, chats a little, then leaves.

use hotline_im::{connect, ConnectOptions, Event, Security};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let nick = std::env::args().nth(1).unwrap_or_else(|| "Chatty".into());
    let lines: Vec<String> = std::env::args().skip(2).collect();
    let opts = ConnectOptions {
        host: "127.0.0.1".into(),
        port: 5500,
        login: String::new(),
        password: String::new(),
        nickname: nick.clone(),
        icon: 0,
        security: Security::Auto,
        classic: true,
        media: false,
        history: false,
    };
    let mut s = connect(&opts).await.expect("join");
    let c = s.client.clone();
    tokio::spawn(async move {
        while let Some(e) = s.events.recv().await {
            if let Event::ChatMessage { text, .. } = &e {
                println!("chat: {}", text.trim());
            }
        }
    });
    let pause = |ms| tokio::time::sleep(Duration::from_millis(ms));
    pause(1500).await;
    let script = if lines.is_empty() {
        vec![
            "hey everybody".to_string(),
            "/me waves".to_string(),
            "anyone remember the old AIM chat rooms?".to_string(),
        ]
    } else {
        lines
    };
    for l in script {
        match l.strip_prefix("/me ") {
            Some(a) => c.send_chat(a, true),
            None => c.send_chat(&l, false),
        }
        pause(1800).await;
    }
    pause(
        std::env::var("CHATTER_STAY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4000),
    )
    .await;
    c.disconnect();
    pause(300).await;
}
