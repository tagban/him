//! A buddy who takes every file offered and saves it:
//! `cargo run --example take_files -- login:password@host:port folder`
use hotline_im::{connect, ConnectOptions, Event, Presence, Security};

#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let (spec, folder) = (a.next().unwrap(), a.next().unwrap());
    let (cred, addr) = spec.rsplit_once('@').unwrap();
    let (login, password) = cred.split_once(':').unwrap();
    let (host, port) = addr.split_once(':').unwrap();
    let mut s = connect(&ConnectOptions {
        host: host.into(), port: port.parse().unwrap(), login: login.into(), password: password.into(),
        nickname: login.into(), icon: 0, security: Security::Auto, classic: false, media: false, history: false,
    })
    .await
    .expect("sign on");
    println!("{login} signed on via {}", s.client.info.transport);
    s.client.set_presence(Presence::Online, "", None).await.unwrap();
    s.client.get_roster().await.unwrap();
    while let Some(e) = s.events.recv().await {
        match e {
            Event::FileOffer { offer: hotline_im::FileOffer { from, guid, name, size } } => {
                println!("{from} offers {name} ({size} bytes); taking it");
                s.client.accept_file(&guid).await.expect("accept");
            }
            Event::FileReady { relay_ref, .. } => {
                let c = s.client.clone();
                let folder = folder.clone();
                tokio::spawn(async move {
                    let tmp = std::path::Path::new(&folder).join(".incoming");
                    let f = tokio::fs::File::create(&tmp).await.unwrap();
                    match c.receive_file(relay_ref, f, |_, _| {}).await {
                        Ok(name) => {
                            let to = std::path::Path::new(&folder).join(&name);
                            std::fs::rename(&tmp, &to).unwrap();
                            println!("saved {}", to.display());
                        }
                        Err(e) => println!("receive failed: {e}"),
                    }
                });
            }
            Event::FileDeclined { .. } => println!("{e:?}"),
            _ => {}
        }
    }
}
