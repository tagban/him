//! String fields travel as UTF-8 once `CAPABILITY_TEXT_ENCODING` is confirmed,
//! otherwise as Mac Roman (guide §6).

use encoding_rs::MACINTOSH;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextMode {
    MacRoman,
    Utf8,
}

impl TextMode {
    pub fn decode(self, bytes: &[u8]) -> String {
        let s = match self {
            TextMode::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            // A server that didn't confirm UTF-8 may still relay it from modern clients;
            // Mac Roman with accents is almost never valid UTF-8, so a clean decode means it was.
            TextMode::MacRoman => match std::str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
                Err(_) => MACINTOSH.decode_without_bom_handling(bytes).0.into_owned(),
            },
        };
        // Mac Roman peers end lines with CR; show them as LF.
        s.replace("\r\n", "\n").replace('\r', "\n")
    }

    pub fn encode(self, s: &str) -> Vec<u8> {
        match self {
            TextMode::Utf8 => s.as_bytes().to_vec(),
            TextMode::MacRoman => {
                let s = s.replace('\n', "\r");
                // Unmappable characters become '?' rather than HTML entities.
                let mut out = Vec::with_capacity(s.len());
                let mut buf = [0u8; 4];
                for ch in s.chars() {
                    let (bytes, _, bad) = MACINTOSH.encode(ch.encode_utf8(&mut buf));
                    if bad {
                        out.push(b'?');
                    } else {
                        out.extend_from_slice(&bytes);
                    }
                }
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_roman_round_trip_and_fallback() {
        let m = TextMode::MacRoman;
        assert_eq!(m.encode("café"), vec![b'c', b'a', b'f', 0x8E]);
        assert_eq!(m.decode(&[b'c', b'a', b'f', 0x8E]), "café");
        assert_eq!(m.encode("日"), b"?");
        assert_eq!(m.encode("a\nb"), b"a\rb");
    }
}
