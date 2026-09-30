//! The info port (Docs/Protocol/Hotline-Info-Port.md): an advisory, one-shot
//! "what do you support?" before the real session.

use serde::Deserialize;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_PAYLOAD: usize = 64 * 1024;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Descriptor {
    pub info_version: u32,
    pub data_port: u16,
    pub tls_port: Option<u16>,
    pub transport: Transport,
    pub server: Option<ServerIdent>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ServerIdent {
    pub name: Option<String>,
    pub description: Option<String>,
    pub hostname: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Transport {
    pub hope: Hope,
    pub tls: Tls,
    pub plaintext: Plaintext,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Hope {
    pub supported: Option<bool>,
    pub required: bool,
}

impl Hope {
    pub fn known_unsupported(&self) -> bool {
        self.supported == Some(false)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Tls {
    pub supported: bool,
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Plaintext {
    pub accepted: bool,
}

impl Default for Plaintext {
    fn default() -> Self {
        Plaintext { accepted: true }
    }
}

/// `None` for anything but a clean OK: timeouts, refusals and garbage all mean
/// "no info protocol here", and the caller keeps its own plan.
pub async fn probe(host: &str, port: u16) -> Option<Descriptor> {
    tokio::time::timeout(PROBE_TIMEOUT, probe_inner(host, port))
        .await
        .ok()
        .flatten()
}

async fn probe_inner(host: &str, port: u16) -> Option<Descriptor> {
    let mut s = TcpStream::connect((host, port)).await.ok()?;
    s.write_all(b"HLIP\x00\x01\x00\x00").await.ok()?;
    let mut h = [0u8; 12];
    s.read_exact(&mut h).await.ok()?;
    if &h[..4] != b"HLIP" || u16::from_be_bytes([h[6], h[7]]) != 0 {
        return None;
    }
    let len = u32::from_be_bytes(h[8..12].try_into().unwrap()) as usize;
    if len == 0 || len > MAX_PAYLOAD {
        return None;
    }
    let mut body = vec![0u8; len];
    s.read_exact(&mut body).await.ok()?;
    serde_json::from_slice(&body).ok()
}
