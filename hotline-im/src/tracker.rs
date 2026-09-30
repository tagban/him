//! Tracker lists (HTRK, version 1): where the public Hotline servers are.
//! Ported from BigRedH's crawler, which was checked against the live trackers.

use crate::text::TextMode;
use serde::Serialize;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const TRACKER_PORT: u16 = 5498;

/// The trackers BigRedH merges.
pub const DEFAULT_TRACKERS: &[&str] = &[
    "hltracker.com",
    "tracker.preterhuman.net",
    "hotline.kicks-ass.net",
    "saddle.dyndns.org",
];

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListedServer {
    pub host: String,
    pub port: u16,
    pub users: u16,
    pub name: String,
    pub description: String,
    /// Which trackers list it.
    pub trackers: Vec<String>,
}

/// One tracker's list. Version 1: every tracker speaks it, and some hang up on newer ones.
pub async fn query(tracker: &str, timeout: Duration) -> std::io::Result<Vec<ListedServer>> {
    let (host, port) = match tracker.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.parse().unwrap()),
        _ => (tracker.to_string(), TRACKER_PORT),
    };
    tokio::time::timeout(timeout, query_inner(&host, port, tracker))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "tracker didn't answer"))?
}

async fn query_inner(host: &str, port: u16, label: &str) -> std::io::Result<Vec<ListedServer>> {
    let mut s = TcpStream::connect((host, port)).await?;
    s.write_all(b"HTRK\x00\x01").await?;
    let mut magic = [0u8; 6];
    s.read_exact(&mut magic).await?;
    if &magic[..4] != b"HTRK" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a tracker",
        ));
    }
    let mut out = Vec::new();
    // The first batch header's count is the total; each batch header also says how many
    // entries follow it. The tracker keeps the connection open, so stop at the total.
    let mut total: Option<usize> = None;
    for _ in 0..200 {
        if total.is_some_and(|t| out.len() >= t) {
            break;
        }
        let mut h = [0u8; 8];
        s.read_exact(&mut h).await?;
        let t = u16::from_be_bytes([h[4], h[5]]) as usize;
        let total = *total.get_or_insert(t);
        let in_batch = u16::from_be_bytes([h[6], h[7]]) as usize;
        for _ in 0..in_batch {
            let mut fixed = [0u8; 10];
            s.read_exact(&mut fixed).await?;
            let name = read_pstr(&mut s).await?;
            let desc = read_pstr(&mut s).await?;
            out.push(ListedServer {
                host: Ipv4Addr::new(fixed[0], fixed[1], fixed[2], fixed[3]).to_string(),
                port: u16::from_be_bytes([fixed[4], fixed[5]]),
                users: u16::from_be_bytes([fixed[6], fixed[7]]),
                name: TextMode::MacRoman.decode(&name).trim().to_string(),
                description: TextMode::MacRoman.decode(&desc).trim().to_string(),
                trackers: vec![label.to_string()],
            });
        }
        if in_batch == 0 && total > 0 {
            break;
        }
    }
    Ok(out)
}

async fn read_pstr(s: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let len = s.read_u8().await? as usize;
    let mut b = vec![0u8; len];
    s.read_exact(&mut b).await?;
    Ok(b)
}

/// Every tracker at once, merged by address (the biggest user count wins), busiest first.
/// Trackers that fail are reported, not fatal.
pub async fn query_all(trackers: &[String], timeout: Duration) -> (Vec<ListedServer>, Vec<String>) {
    let tasks: Vec<_> = trackers
        .iter()
        .map(|t| {
            let t = t.clone();
            tokio::spawn(async move { (t.clone(), query(&t, timeout).await) })
        })
        .collect();
    let mut merged: HashMap<(String, u16), ListedServer> = HashMap::new();
    let mut failed = Vec::new();
    for task in tasks {
        let Ok((label, result)) = task.await else {
            continue;
        };
        match result {
            Ok(list) => {
                for s in list {
                    // Trackers pad their lists with "-------" dividers; they aren't servers.
                    let divider = !s.name.chars().any(|c| c.is_alphanumeric());
                    if s.port == 0 || s.host == "0.0.0.0" || divider {
                        continue;
                    }
                    merged
                        .entry((s.host.clone(), s.port))
                        .and_modify(|m| {
                            m.users = m.users.max(s.users);
                            if !m.trackers.contains(&label) {
                                m.trackers.push(label.clone());
                            }
                            if m.description.is_empty() {
                                m.description = s.description.clone();
                            }
                        })
                        .or_insert(s);
                }
            }
            Err(e) => failed.push(format!("{label}: {e}")),
        }
    }
    let mut list: Vec<_> = merged.into_values().collect();
    list.sort_by(|a, b| {
        b.users
            .cmp(&a.users)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    (list, failed)
}

/// A tracker that lists fixed entries (tests and the mock server).
#[cfg(any(test, feature = "mock-server"))]
pub async fn serve_mock(
    listener: tokio::net::TcpListener,
    entries: Vec<(Ipv4Addr, u16, u16, String, String)>,
) {
    while let Ok((mut s, _)) = listener.accept().await {
        let entries = entries.clone();
        tokio::spawn(async move {
            let mut req = [0u8; 6];
            if s.read_exact(&mut req).await.is_err() || &req[..4] != b"HTRK" {
                return;
            }
            let mut out = b"HTRK\x00\x01".to_vec();
            let mut body = Vec::new();
            for (ip, port, users, name, desc) in &entries {
                body.extend(ip.octets());
                body.extend(port.to_be_bytes());
                body.extend(users.to_be_bytes());
                body.extend([0, 0]);
                body.push(name.len() as u8);
                body.extend(name.as_bytes());
                body.push(desc.len() as u8);
                body.extend(desc.as_bytes());
            }
            out.extend([0, 1]); // message type
            out.extend(((body.len() + 4) as u16).to_be_bytes());
            out.extend((entries.len() as u16).to_be_bytes());
            out.extend((entries.len() as u16).to_be_bytes());
            out.extend(body);
            let _ = s.write_all(&out).await;
            // like the real ones, keep the connection open for a moment
            tokio::time::sleep(Duration::from_secs(2)).await;
        });
    }
}
