//! Offers a file to a buddy and sends it once they accept:
//! `cargo run --example send_file -- login:password@host:port to path`
use hotline_im::{connect, ConnectOptions, Event, Security};

#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let (spec, to, path) = (a.next().unwrap(), a.next().unwrap(), a.next().unwrap());
    let (cred, addr) = spec.rsplit_once('@').unwrap();
    let (login, password) = cred.split_once(':').unwrap();
    let (host, port) = addr.split_once(':').unwrap();
    let mut s = connect(&ConnectOptions {
        host: host.into(), port: port.parse().unwrap(), login: login.into(), password: password.into(),
        nickname: login.into(), icon: 0, security: Security::Auto, classic: false, media: false, history: false,
    })
    .await
    .expect("sign on");
    s.client.get_roster().await.unwrap();
    let data = std::fs::read(&path).unwrap();
    let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().into_owned();
    let guid = s.client.offer_file(&to, &name, data.len() as u64).await.expect("offer");
    println!("offered {name} ({} bytes) to {to}; waiting...", data.len());
    while let Some(e) = s.events.recv().await {
        match e {
            Event::FileReady { guid: g, relay_ref } if g == guid => {
                s.client.send_file(relay_ref, &name, data.len() as u64, &data[..], |_, _| {}).await.expect("send");
                println!("sent");
                break;
            }
            Event::FileDeclined { guid: g, .. } if g == guid => { println!("declined"); break; }
            _ => {}
        }
    }
}
