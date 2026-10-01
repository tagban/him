//! Does a server take pictures in chat and GIF icons? `cargo run --example probe_media -- host [port]`
use hotline_im::{connect, ConnectOptions, Security};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let host = args.next().expect("host");
    let port = args.next().and_then(|p| p.parse().ok()).unwrap_or(5500);
    let s = connect(&ConnectOptions {
        host: host.clone(),
        port,
        login: String::new(),
        password: String::new(),
        nickname: "HIM probe".into(),
        icon: 0,
        security: Security::Auto,
        classic: true,
        media: true,
        history: true,
    })
    .await
    .expect("connect");
    let c = &s.client;
    println!("{host}: {:?} caps {:#06x}", c.info.server_name, c.info.caps);
    println!("  inline media: {:?}", c.info.media);
    println!("  gif icons: {:?}", c.gif_icons().await.map(|l| l.len()));
    println!("  chat history: {} {:?}", c.info.chat_history, c.chat_history(None, None, 3).await.map(|p| (p.entries.len(), p.has_more)));
    c.disconnect();
}
