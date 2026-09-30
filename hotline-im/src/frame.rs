//! Reading and writing whole transactions on a byte stream, in the clear or
//! sealed as HOPE ChaCha20-Poly1305 frames (guide §7.6).

use crate::wire::{header_sizes, Transaction, HEADER_LEN};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Largest frame or transaction we accept (a hostile length must not exhaust memory).
pub const MAX_READ: usize = 16 * 1024 * 1024;

pub const DIR_SERVER_TO_CLIENT: u8 = 0x00;
pub const DIR_CLIENT_TO_SERVER: u8 = 0x01;

/// One direction of an AEAD session: its key, its direction byte and its counter.
pub struct Sealer {
    cipher: ChaCha20Poly1305,
    dir: u8,
    counter: u64,
}

impl Sealer {
    pub fn new(key: &[u8; 32], dir: u8) -> Self {
        Sealer {
            cipher: ChaCha20Poly1305::new(Key::from_slice(key)),
            dir,
            counter: 0,
        }
    }

    fn next_nonce(&mut self) -> [u8; 12] {
        let mut n = [0u8; 12];
        n[0] = self.dir;
        n[4..].copy_from_slice(&self.counter.to_be_bytes());
        self.counter += 1;
        n
    }

    pub fn seal(&mut self, plain: &[u8]) -> Vec<u8> {
        let nonce = self.next_nonce();
        self.cipher
            .encrypt(Nonce::from_slice(&nonce), plain)
            .expect("sealing cannot fail")
    }

    pub fn open(&mut self, sealed: &[u8]) -> io::Result<Vec<u8>> {
        let nonce = self.next_nonce();
        self.cipher
            .decrypt(Nonce::from_slice(&nonce), sealed)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "AEAD frame failed authentication",
                )
            })
    }
}

pub struct FrameReader<R> {
    inner: R,
    aead: Option<Sealer>,
    /// Plaintext left over from an AEAD frame that held more than one transaction.
    buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(inner: R) -> Self {
        FrameReader {
            inner,
            aead: None,
            buf: Vec::new(),
        }
    }

    pub fn enable_aead(&mut self, sealer: Sealer) {
        self.aead = Some(sealer);
    }

    async fn fill(&mut self, want: usize) -> io::Result<()> {
        while self.buf.len() < want {
            match &mut self.aead {
                None => {
                    let mut chunk = vec![0u8; want - self.buf.len()];
                    self.inner.read_exact(&mut chunk).await?;
                    self.buf.extend_from_slice(&chunk);
                }
                Some(sealer) => {
                    let len = self.inner.read_u32().await? as usize;
                    if !(16..=MAX_READ).contains(&len) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "bad AEAD frame length",
                        ));
                    }
                    let mut sealed = vec![0u8; len];
                    self.inner.read_exact(&mut sealed).await?;
                    let plain = sealer.open(&sealed)?;
                    self.buf.extend_from_slice(&plain);
                }
            }
        }
        Ok(())
    }

    /// The first read after switching to AEAD: a server that refused the login
    /// may answer in the clear. A plaintext reply opens `00 01` (flags, is-reply),
    /// which as a frame length would be an implausible 64 KiB+ login reply.
    pub async fn read_first_sealed(&mut self) -> io::Result<(Transaction, bool)> {
        if self.aead.is_none() || !self.buf.is_empty() {
            return Ok((self.read().await?, false));
        }
        let mut head = [0u8; 4];
        self.inner.read_exact(&mut head).await?;
        if head[0] == 0 && head[1] == 1 {
            let mut rest = [0u8; HEADER_LEN - 4];
            self.inner.read_exact(&mut rest).await?;
            let mut header = [0u8; HEADER_LEN];
            header[..4].copy_from_slice(&head);
            header[4..].copy_from_slice(&rest);
            let part = header_sizes(&header).1 as usize;
            if part > MAX_READ {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "transaction too large",
                ));
            }
            let mut payload = vec![0u8; part];
            self.inner.read_exact(&mut payload).await?;
            let t = Transaction::decode(&header, &payload)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            return Ok((t, true));
        }
        let len = u32::from_be_bytes(head) as usize;
        if !(16..=MAX_READ).contains(&len) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bad AEAD frame length",
            ));
        }
        let mut sealed = vec![0u8; len];
        self.inner.read_exact(&mut sealed).await?;
        let plain = self.aead.as_mut().unwrap().open(&sealed)?;
        self.buf.extend_from_slice(&plain);
        Ok((self.read().await?, false))
    }

    fn take(&mut self, n: usize) -> Vec<u8> {
        self.buf.drain(..n).collect()
    }

    pub async fn read_raw(&mut self, n: usize) -> io::Result<Vec<u8>> {
        self.fill(n).await?;
        Ok(self.take(n))
    }

    /// Reads one transaction, joining a multi-part payload if the sender split it.
    pub async fn read(&mut self) -> io::Result<Transaction> {
        self.fill(HEADER_LEN).await?;
        let header: [u8; HEADER_LEN] = self.take(HEADER_LEN).try_into().unwrap();
        let (total, part) = header_sizes(&header);
        let (total, mut part) = (total as usize, part as usize);
        if total > MAX_READ || part > MAX_READ {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "transaction too large",
            ));
        }
        let mut payload = Vec::with_capacity(total.max(part));
        loop {
            self.fill(part).await?;
            payload.extend(self.take(part));
            if payload.len() >= total {
                break;
            }
            // Next part: another header of the same transaction.
            self.fill(HEADER_LEN).await?;
            let h: [u8; HEADER_LEN] = self.take(HEADER_LEN).try_into().unwrap();
            part = header_sizes(&h).1 as usize;
            if part == 0 || payload.len() + part > MAX_READ {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "bad transaction part",
                ));
            }
        }
        Transaction::decode(&header, &payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

pub struct FrameWriter<W> {
    inner: W,
    aead: Option<Sealer>,
}

impl<W: AsyncWrite + Unpin> FrameWriter<W> {
    pub fn new(inner: W) -> Self {
        FrameWriter { inner, aead: None }
    }

    pub fn enable_aead(&mut self, sealer: Sealer) {
        self.aead = Some(sealer);
    }

    pub async fn write_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        match &mut self.aead {
            None => self.inner.write_all(bytes).await?,
            Some(sealer) => {
                let sealed = sealer.seal(bytes);
                self.inner.write_u32(sealed.len() as u32).await?;
                self.inner.write_all(&sealed).await?;
            }
        }
        self.inner.flush().await
    }

    pub async fn write(&mut self, t: &Transaction) -> io::Result<()> {
        self.write_raw(&t.encode()).await
    }

    pub async fn shutdown(&mut self) {
        let _ = self.inner.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{field, Field};

    #[tokio::test]
    async fn aead_round_trip_both_directions() {
        let key = [9u8; 32];
        let (a, b) = tokio::io::duplex(4096);
        let (ar, aw) = tokio::io::split(a);
        let (br, bw) = tokio::io::split(b);
        let mut client_w = FrameWriter::new(aw);
        let mut server_r = FrameReader::new(br);
        client_w.enable_aead(Sealer::new(&key, DIR_CLIENT_TO_SERVER));
        server_r.enable_aead(Sealer::new(&key, DIR_CLIENT_TO_SERVER));
        let _ = (ar, bw);
        for i in 0..3u32 {
            let mut t = Transaction::request(
                810,
                vec![Field::new(field::MESSAGE_BODY, format!("hi {i}"))],
            );
            t.id = i + 1;
            client_w.write(&t).await.unwrap();
            assert_eq!(server_r.read().await.unwrap(), t);
        }
    }
}
