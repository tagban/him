//! Buddy icon pictures (Capabilities-Buddy-Icons.md): the hash that identifies one,
//! and what its header says about its size, read without decoding the image.

use sha2::{Digest, Sha256};

/// The default and the ceiling for `DATA_MAX_ICON_BYTES`: one field holds the picture.
pub const DEFAULT_MAX_BYTES: u32 = 16384;
pub const CEILING_BYTES: u32 = 65535;
/// What a client assumes when the server doesn't advertise `DATA_MAX_ICON_DIMENSION`;
/// every server accepts at least this.
pub const FLOOR_DIMENSION: u32 = 64;
/// Every server accepts at least this many frames.
pub const FLOOR_FRAMES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Gif,
    Png,
    Jpeg,
}

impl Format {
    pub fn sniff(b: &[u8]) -> Option<Format> {
        if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
            Some(Format::Gif)
        } else if b.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Format::Png)
        } else if b.starts_with(b"\xFF\xD8\xFF") {
            Some(Format::Jpeg)
        } else {
            None
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Format::Gif => "gif",
            Format::Png => "png",
            Format::Jpeg => "jpg",
        }
    }
}

/// `DATA_BUDDY_ICON_HASH`: the first 16 bytes of the SHA-256 of the picture.
pub fn hash(b: &[u8]) -> [u8; 16] {
    let d = Sha256::digest(b);
    let mut out = [0u8; 16];
    out.copy_from_slice(&d[..16]);
    out
}

/// What the header declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    /// 1 for a still picture.
    pub frames: usize,
}

/// Reads the format, dimensions and frame count; None if the header is truncated or unreadable.
pub fn inspect(b: &[u8]) -> Option<Header> {
    let format = Format::sniff(b)?;
    let (width, height, frames) = match format {
        Format::Gif => gif(b)?,
        Format::Png => png(b)?,
        Format::Jpeg => {
            let (w, h) = jpeg(b)?;
            (w, h, 1)
        }
    };
    (width > 0 && height > 0).then_some(Header {
        format,
        width,
        height,
        frames,
    })
}

fn u16le(b: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32)
}

fn u16be(b: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32)
}

fn u32be(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// Skips a run of GIF sub-blocks (length byte, data) ending in a zero length.
fn gif_skip_blocks(b: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let n = *b.get(i)? as usize;
        i += 1 + n;
        if n == 0 {
            return Some(i);
        }
    }
}

/// The logical screen descriptor, then one image descriptor per frame.
fn gif(b: &[u8]) -> Option<(u32, u32, usize)> {
    let (w, h) = (u16le(b, 6)?, u16le(b, 8)?);
    let packed = *b.get(10)?;
    let mut i = 13;
    if packed & 0x80 != 0 {
        i += 3 << ((packed & 7) + 1);
    }
    let mut frames = 0;
    loop {
        match *b.get(i)? {
            0x21 => i = gif_skip_blocks(b, i + 2)?,
            0x2C => {
                let p = *b.get(i + 9)?;
                i += 10;
                if p & 0x80 != 0 {
                    i += 3 << ((p & 7) + 1);
                }
                i = gif_skip_blocks(b, i + 1)?;
                frames += 1;
            }
            0x3B => break,
            _ => return None,
        }
    }
    (frames > 0).then_some((w, h, frames))
}

/// IHDR, and acTL's frame count for an animated PNG.
fn png(b: &[u8]) -> Option<(u32, u32, usize)> {
    if b.get(12..16)? != b"IHDR" {
        return None;
    }
    let (w, h) = (u32be(b, 16)?, u32be(b, 20)?);
    let mut frames = 1;
    let mut i = 8;
    while i + 8 <= b.len() {
        let len = u32be(b, i)? as usize;
        let kind = b.get(i + 4..i + 8)?;
        if kind == b"acTL" {
            frames = u32be(b, i + 8)? as usize;
        }
        if kind == b"IDAT" || kind == b"IEND" {
            break;
        }
        i = i.checked_add(12 + len)?;
    }
    Some((w, h, frames.max(1)))
}

/// The first SOFn marker's height and width.
fn jpeg(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    loop {
        while *b.get(i)? != 0xFF {
            i += 1;
        }
        while *b.get(i)? == 0xFF {
            i += 1;
        }
        let m = *b.get(i)?;
        i += 1;
        match m {
            0xD8 | 0x01 | 0xD0..=0xD7 => continue,
            0xD9 | 0xDA => return None,
            0xC0..=0xCF if !matches!(m, 0xC4 | 0xC8 | 0xCC) => {
                return Some((u16be(b, i + 5)?, u16be(b, i + 3)?));
            }
            _ => i += u16be(b, i)? as usize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2-frame 3x2 GIF with a global color table.
    fn tiny_gif() -> Vec<u8> {
        let mut g = b"GIF89a".to_vec();
        g.extend([3, 0, 2, 0, 0x80, 0, 0]); // 3x2, GCT of 2 colors
        g.extend([0, 0, 0, 255, 255, 255]);
        g.extend([0x21, 0xFF, 11]);
        g.extend(b"NETSCAPE2.0");
        g.extend([3, 1, 0, 0, 0]);
        for _ in 0..2 {
            g.extend([0x21, 0xF9, 4, 0, 10, 0, 0, 0]);
            g.extend([0x2C, 0, 0, 0, 0, 3, 0, 2, 0, 0]);
            g.extend([2, 2, 0x4C, 0x01, 0]);
        }
        g.push(0x3B);
        g
    }

    #[test]
    fn reads_gif() {
        let h = inspect(&tiny_gif()).unwrap();
        assert_eq!((h.format, h.width, h.height, h.frames), (Format::Gif, 3, 2, 2));
        let mut cut = tiny_gif();
        cut.truncate(40);
        assert_eq!(inspect(&cut), None);
    }

    #[test]
    fn reads_png_and_jpeg() {
        let mut p = b"\x89PNG\r\n\x1a\n".to_vec();
        p.extend(13u32.to_be_bytes());
        p.extend(b"IHDR");
        p.extend(48u32.to_be_bytes());
        p.extend(40u32.to_be_bytes());
        p.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        p.extend(8u32.to_be_bytes());
        p.extend(b"acTL");
        p.extend(5u32.to_be_bytes());
        p.extend([0; 8]);
        let h = inspect(&p).unwrap();
        assert_eq!((h.width, h.height, h.frames), (48, 40, 5));

        let mut j = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0];
        j.extend([0xFF, 0xC0, 0, 11, 8, 0, 20, 0, 30, 1, 1, 0x11, 0]);
        let h = inspect(&j).unwrap();
        assert_eq!((h.format, h.width, h.height), (Format::Jpeg, 30, 20));
    }

    #[test]
    fn hash_is_16_bytes_of_sha256() {
        assert_eq!(hex_of(&hash(b"abc")), "ba7816bf8f01cfea414140de5dae2223");
    }

    fn hex_of(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}
