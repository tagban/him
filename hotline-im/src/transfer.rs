//! Sending files to a buddy (guide §14): an offer, their answer, then both sides meet
//! on the server's file-transfer port (HTXF) and the server pipes one to the other.
//!
//! Only the relay path is spoken here (the direct, hole-punched one is optional). The
//! payload is a flattened file object: a header, an INFO fork with the name, and the
//! DATA fork with the bytes. On a HOPE ChaCha20-Poly1305 session the transfer
//! connection is sealed too, with a key of its own (HOPE-ChaCha20-Poly1305.md).

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::client::{Client, Error};
use crate::frame::{Sealer, DIR_CLIENT_TO_SERVER, DIR_SERVER_TO_CLIENT, MAX_READ};
use crate::hope::hkdf32;
use crate::messaging::{hex, new_guid, unhex};
use crate::wire::{field, tx, Field};

/// Files bigger than this need the large-file extension, which isn't spoken here.
pub const MAX_FILE: u64 = u32::MAX as u64 - 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long the far side may go quiet mid-transfer.
const IDLE: Duration = Duration::from_secs(60);
const CHUNK: usize = 64 * 1024;

/// Where this session's transfers go, and how they're protected.
#[derive(Clone, Debug, Default)]
pub struct TransferRoute {
    /// The server's address with the file-transfer port (the data port + 1).
    pub addr: Option<SocketAddr>,
    /// Wrap transfers in TLS, checked against this name (TLS sessions).
    pub tls_name: Option<String>,
    /// HOPE AEAD sessions: the base key every transfer key comes from.
    pub aead_base: Option<[u8; 32]>,
}

impl TransferRoute {
    /// `ft_base_key = HKDF-SHA256(encode ‖ decode, salt = session key, "hope-file-transfer")`
    pub fn aead_base(encode: &[u8; 32], decode: &[u8; 32], session_key: &[u8]) -> [u8; 32] {
        let mut ikm = encode.to_vec();
        ikm.extend_from_slice(decode);
        hkdf32(&ikm, session_key, b"hope-file-transfer")
    }

    /// One transfer's key: `HKDF-SHA256(ft_base_key, salt = ref (4 bytes), "hope-ft-ref")`
    pub fn transfer_key(base: &[u8; 32], relay_ref: u32) -> [u8; 32] {
        hkdf32(base, &relay_ref.to_be_bytes(), b"hope-ft-ref")
    }
}

/// A file someone wants to send us (File Offer, 814).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileOffer {
    pub from: String,
    /// Hex of the transfer's GUID; accept or decline with it.
    pub guid: String,
    pub name: String,
    pub size: u64,
}

impl FileOffer {
    pub(crate) fn parse(t: &crate::wire::Transaction, text: crate::text::TextMode) -> Option<Self> {
        Some(FileOffer {
            from: text.decode(t.bytes(field::FRIEND_LOGIN)?),
            guid: hex(t.bytes(field::FILE_TRANSFER_GUID)?),
            name: text.decode(t.bytes(field::FILE_NAME).unwrap_or_default()),
            size: t.uint(field::FILE_SIZE64).or_else(|| t.uint(field::FILE_SIZE)).unwrap_or(0),
        })
    }
}

/// Keeps file names to something every system can save: no paths, no control characters.
pub fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\', ':']).next().unwrap_or(name);
    let clean: String = base.chars().filter(|c| !c.is_control()).collect::<String>().trim().trim_start_matches('.').to_string();
    let clean: String = clean.chars().take(120).collect();
    if clean.is_empty() { "file".into() } else { clean }
}

impl Client {
    /// Offers `to` a file (File Offer, 814). The GUID (hex) names the transfer from here on;
    /// File Ready (817) follows if they accept.
    pub async fn offer_file(&self, to: &str, name: &str, size: u64) -> Result<String, Error> {
        if size > MAX_FILE {
            return Err(Error::Server { reason: None, text: "That file is too big to send (4 GB at most).".into() });
        }
        let guid = new_guid();
        self.request(
            tx::FILE_OFFER,
            vec![
                Field::new(field::FRIEND_LOGIN, self.text.encode(to)),
                Field::new(field::FILE_NAME, self.text.encode(&safe_name(name))),
                Field::u32(field::FILE_SIZE, size as u32),
                Field::new(field::FILE_TRANSFER_GUID, guid.to_vec()),
            ],
        )
        .await?;
        Ok(hex(&guid))
    }

    /// Takes a file someone offered (File Accept, 815); File Ready (817) follows.
    pub async fn accept_file(&self, guid: &str) -> Result<(), Error> {
        let g = unhex(guid).ok_or_else(|| bad("Bad transfer ID."))?;
        self.request(tx::FILE_ACCEPT, vec![Field::new(field::FILE_TRANSFER_GUID, g)]).await.map(|_| ())
    }

    /// Turns down an offer, or calls off one of ours (File Decline, 816).
    pub async fn decline_file(&self, guid: &str) -> Result<(), Error> {
        let g = unhex(guid).ok_or_else(|| bad("Bad transfer ID."))?;
        self.request(tx::FILE_DECLINE, vec![Field::new(field::FILE_TRANSFER_GUID, g)]).await.map(|_| ())
    }

    /// The sending side, once File Ready arrives: streams `size` bytes from `data` as `name`.
    /// `progress(done, total)` is called as it goes.
    pub async fn send_file<R: AsyncRead + Unpin>(
        &self,
        relay_ref: u32,
        name: &str,
        size: u64,
        mut data: R,
        mut progress: impl FnMut(u64, u64) + Send,
    ) -> Result<(), Error> {
        let info = info_fork(&safe_name(name), self.text);
        let total = 24 + 16 + info.len() as u64 + 16 + size;
        let mut x = self.open_transfer(relay_ref, total as u32).await?;
        let mut head = Vec::with_capacity(56 + info.len());
        head.extend_from_slice(b"FILP");
        head.extend(1u16.to_be_bytes());
        head.extend([0u8; 16]);
        head.extend(2u16.to_be_bytes());
        head.extend(fork_header(b"INFO", info.len() as u32));
        head.extend(&info);
        head.extend(fork_header(b"DATA", size as u32));
        x.write(&head).await?;
        let mut sent = 0u64;
        let mut buf = vec![0u8; CHUNK];
        progress(0, size);
        while sent < size {
            let want = CHUNK.min((size - sent) as usize);
            let n = data.read(&mut buf[..want]).await.map_err(|e| Error::Connect(e.to_string()))?;
            if n == 0 {
                return Err(Error::Connect("The file got shorter while it was being sent.".into()));
            }
            x.write(&buf[..n]).await?;
            sent += n as u64;
            progress(sent, size);
        }
        x.finish().await;
        Ok(())
    }

    /// The receiving side, once File Ready arrives: writes the file's bytes to `out` and
    /// returns its name. `progress(done, total)` is called as it goes.
    pub async fn receive_file<W: AsyncWrite + Unpin>(
        &self,
        relay_ref: u32,
        mut out: W,
        mut progress: impl FnMut(u64, u64) + Send,
    ) -> Result<String, Error> {
        let mut x = self.open_transfer(relay_ref, 0).await?;
        let head = x.read(24).await?;
        if &head[..4] != b"FILP" {
            return Err(bad("That wasn't a file."));
        }
        let forks = u16::from_be_bytes([head[22], head[23]]);
        let mut name = String::from("file");
        let mut got_data = false;
        for _ in 0..forks.min(8) {
            let fh = x.read(16).await?;
            let kind = [fh[0], fh[1], fh[2], fh[3]];
            let len = u32::from_be_bytes([fh[12], fh[13], fh[14], fh[15]]) as u64;
            match &kind {
                b"INFO" => {
                    let info = x.read(len.min(4096) as usize).await?;
                    x.skip(len.saturating_sub(4096)).await?;
                    if info.len() >= 72 {
                        let n = u16::from_be_bytes([info[70], info[71]]) as usize;
                        if let Some(raw) = info.get(72..72 + n) {
                            name = safe_name(&self.text.decode(raw));
                        }
                    }
                }
                b"DATA" => {
                    let mut done = 0u64;
                    progress(0, len);
                    while done < len {
                        let part = x.read(CHUNK.min((len - done) as usize)).await?;
                        out.write_all(&part).await.map_err(|e| Error::Connect(e.to_string()))?;
                        done += part.len() as u64;
                        progress(done, len);
                    }
                    got_data = true;
                }
                _ => x.skip(len).await?, // a resource fork, say: not needed off a Mac
            }
        }
        out.flush().await.map_err(|e| Error::Connect(e.to_string()))?;
        if !got_data {
            return Err(bad("The file arrived empty."));
        }
        Ok(name)
    }

    /// Opens the transfer port and says who we are (HTXF): `size` is what we'll send (0 to receive).
    async fn open_transfer(&self, relay_ref: u32, size: u32) -> Result<Transfer, Error> {
        let route = &self.route;
        let addr = route.addr.ok_or_else(|| bad("This server can't take file transfers."))?;
        let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr))
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|e| Error::Connect(format!("the file-transfer port: {e}")))?;
        let _ = tcp.set_nodelay(true);
        let mut stream: Box<dyn crate::client::Stream> = match &route.tls_name {
            Some(name) => Box::new(crate::tls::connect(name, tcp).await.map_err(|e| Error::Tls(e.to_string()))?),
            None => Box::new(tcp),
        };
        let mut hs = Vec::with_capacity(16);
        hs.extend_from_slice(b"HTXF");
        hs.extend(relay_ref.to_be_bytes());
        hs.extend(size.to_be_bytes());
        hs.extend(0u32.to_be_bytes());
        stream.write_all(&hs).await.map_err(|e| Error::Connect(e.to_string()))?;
        stream.flush().await.map_err(|e| Error::Connect(e.to_string()))?;
        let seal = route.aead_base.map(|base| {
            let key = TransferRoute::transfer_key(&base, relay_ref);
            (Sealer::new(&key, DIR_CLIENT_TO_SERVER), Sealer::new(&key, DIR_SERVER_TO_CLIENT))
        });
        Ok(Transfer::new(stream, seal))
    }
}

fn bad(s: &str) -> Error {
    Error::Server { reason: None, text: s.into() }
}

fn fork_header(kind: &[u8; 4], len: u32) -> Vec<u8> {
    let mut h = kind.to_vec();
    h.extend([0u8; 8]); // compression, reserved
    h.extend(len.to_be_bytes());
    h
}

/// The INFO fork: platform, type, creator, flags, dates, then the name (and an empty comment).
fn info_fork(name: &str, text: crate::text::TextMode) -> Vec<u8> {
    let name = text.encode(name);
    let now = hotline_date(SystemTime::now());
    let mut f = Vec::with_capacity(74 + name.len());
    f.extend_from_slice(b"AMAC");
    f.extend_from_slice(b"????");
    f.extend_from_slice(b"????");
    f.extend([0u8; 8]); // flags, platform flags
    f.extend([0u8; 32]);
    f.extend(now);
    f.extend(now);
    f.extend(0u16.to_be_bytes()); // name script
    f.extend((name.len() as u16).to_be_bytes());
    f.extend(name);
    f.extend(0u16.to_be_bytes()); // comment length
    f
}

/// Hotline's date: year (2), milliseconds (2), seconds since the start of that year (4).
fn hotline_date(t: SystemTime) -> [u8; 8] {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (year, _, _) = civil(days);
    let year_start = days_from_civil(year, 1, 1);
    let into = secs - (year_start as u64) * 86_400;
    let mut d = [0u8; 8];
    d[..2].copy_from_slice(&(year as u16).to_be_bytes());
    d[4..].copy_from_slice(&(into as u32).to_be_bytes());
    d
}

/// Days since 1970 → (year, month, day), Howard Hinnant's algorithm.
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A transfer connection: plain bytes, or AEAD frames (`u32 length ‖ sealed`).
pub struct Transfer {
    s: Box<dyn crate::client::Stream>,
    /// (ours to seal, theirs to open)
    seal: Option<(Sealer, Sealer)>,
    buf: Vec<u8>,
}

impl Transfer {
    pub fn new(s: Box<dyn crate::client::Stream>, seal: Option<(Sealer, Sealer)>) -> Self {
        Transfer { s, seal, buf: Vec::new() }
    }

    pub async fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let io = |e: std::io::Error| Error::Connect(format!("the transfer: {e}"));
        match &mut self.seal {
            None => self.s.write_all(bytes).await.map_err(io),
            Some((ours, _)) => {
                for part in bytes.chunks(CHUNK) {
                    let sealed = ours.seal(part);
                    self.s.write_all(&(sealed.len() as u32).to_be_bytes()).await.map_err(io)?;
                    self.s.write_all(&sealed).await.map_err(io)?;
                }
                Ok(())
            }
        }
    }

    /// Exactly `n` bytes (fewer only if the transfer ends).
    pub async fn read(&mut self, n: usize) -> Result<Vec<u8>, Error> {
        while self.buf.len() < n {
            let more = self.next().await?;
            if more.is_empty() {
                return Err(Error::Connect("The transfer stopped partway.".into()));
            }
            self.buf.extend(more);
        }
        let rest = self.buf.split_off(n);
        Ok(std::mem::replace(&mut self.buf, rest))
    }

    async fn skip(&mut self, mut n: u64) -> Result<(), Error> {
        while n > 0 {
            let k = n.min(CHUNK as u64) as usize;
            self.read(k).await?;
            n -= k as u64;
        }
        Ok(())
    }

    /// The next piece off the wire: up to a chunk in the clear, or one opened frame.
    async fn next(&mut self) -> Result<Vec<u8>, Error> {
        let io = |e: std::io::Error| Error::Connect(format!("the transfer: {e}"));
        let fut = async {
            match &mut self.seal {
                None => {
                    let mut b = vec![0u8; CHUNK];
                    let n = self.s.read(&mut b).await.map_err(io)?;
                    b.truncate(n);
                    Ok(b)
                }
                Some((_, theirs)) => {
                    let mut len = [0u8; 4];
                    if self.s.read_exact(&mut len).await.is_err() {
                        return Ok(Vec::new());
                    }
                    let len = u32::from_be_bytes(len) as usize;
                    if len > MAX_READ {
                        return Err(Error::Connect("The transfer sent a frame too big to be real.".into()));
                    }
                    let mut sealed = vec![0u8; len];
                    self.s.read_exact(&mut sealed).await.map_err(io)?;
                    theirs.open(&sealed).map_err(io)
                }
            }
        };
        tokio::time::timeout(IDLE, fut).await.map_err(|_| Error::Timeout)?
    }

    pub async fn finish(&mut self) {
        let _ = self.s.flush().await;
        let _ = self.s.shutdown().await;
    }

    /// For the test server's relay: what the far side sends, as it comes.
    pub async fn next_piece(&mut self) -> Result<Vec<u8>, Error> {
        if !self.buf.is_empty() {
            return Ok(std::mem::take(&mut self.buf));
        }
        self.next().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_names() {
        // 2026-09-30 00:00:00 UTC is day 272 of 2026 (0-based 272).
        let t = UNIX_EPOCH + Duration::from_secs(1_790_726_400);
        let d = hotline_date(t);
        assert_eq!(u16::from_be_bytes([d[0], d[1]]), 2026);
        assert_eq!(u32::from_be_bytes([d[4], d[5], d[6], d[7]]), 272 * 86_400);
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\pics\\cat.png"), "cat.png");
        assert_eq!(safe_name(".hidden"), "hidden");
        assert_eq!(safe_name("   "), "file");
    }
}
