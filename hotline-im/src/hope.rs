//! HOPE secure login (Docs/Protocol/HOPE-Secure-Login.md) and the keys of its
//! ChaCha20-Poly1305 transport (HOPE-ChaCha20-Poly1305.md).

use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// What we offer, strongest first. `INVERSE` must always be accepted.
pub const OFFERED_MACS: &[&str] = &["HMAC-SHA256", "HMAC-SHA1", "INVERSE"];
pub const AEAD_CIPHER: &str = "CHACHA20-POLY1305";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacAlg {
    HmacSha256,
    HmacSha1,
    Sha1,
    HmacMd5,
    Md5,
    Inverse,
}

impl MacAlg {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.to_ascii_uppercase().as_str() {
            "HMAC-SHA256" => MacAlg::HmacSha256,
            "HMAC-SHA1" => MacAlg::HmacSha1,
            "SHA1" => MacAlg::Sha1,
            "HMAC-MD5" => MacAlg::HmacMd5,
            "MD5" => MacAlg::Md5,
            "INVERSE" => MacAlg::Inverse,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            MacAlg::HmacSha256 => "HMAC-SHA256",
            MacAlg::HmacSha1 => "HMAC-SHA1",
            MacAlg::Sha1 => "SHA1",
            MacAlg::HmacMd5 => "HMAC-MD5",
            MacAlg::Md5 => "MD5",
            MacAlg::Inverse => "INVERSE",
        }
    }

    /// `mac(key, msg)`: HMAC variants key with `key`; the plain hashes are
    /// `hash(key ‖ msg)`; `INVERSE` is the legacy obfuscation of `key`.
    pub fn mac(self, key: &[u8], msg: &[u8]) -> Vec<u8> {
        match self {
            MacAlg::HmacSha256 => hmac::<Hmac<Sha256>>(key, msg),
            MacAlg::HmacSha1 => hmac::<Hmac<Sha1>>(key, msg),
            MacAlg::HmacMd5 => hmac::<Hmac<Md5>>(key, msg),
            MacAlg::Sha1 => Sha1::new()
                .chain_update(key)
                .chain_update(msg)
                .finalize()
                .to_vec(),
            MacAlg::Md5 => Md5::new()
                .chain_update(key)
                .chain_update(msg)
                .finalize()
                .to_vec(),
            MacAlg::Inverse => crate::wire::invert(key),
        }
    }
}

fn hmac<M: Mac + hmac::digest::KeyInit>(key: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut m =
        <M as hmac::digest::KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    m.update(msg);
    m.finalize().into_bytes().to_vec()
}

/// The two 256-bit transport keys, named from the server's side: the server
/// writes with `encode` and reads with `decode`; a client does the reverse.
#[derive(Clone)]
pub struct AeadKeys {
    pub encode: [u8; 32],
    pub decode: [u8; 32],
}

pub fn aead_keys(alg: MacAlg, password: &[u8], session_key: &[u8]) -> Option<AeadKeys> {
    if alg == MacAlg::Inverse {
        return None;
    }
    let password_mac = alg.mac(password, session_key);
    let encode_key = alg.mac(password, &password_mac);
    let decode_key = alg.mac(password, &encode_key);
    Some(AeadKeys {
        encode: hkdf32(&encode_key, session_key, b"hope-chacha-encode"),
        decode: hkdf32(&decode_key, session_key, b"hope-chacha-decode"),
    })
}

pub(crate) fn hkdf32(ikm: &[u8], salt: &[u8], info: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, &mut out)
        .expect("32 bytes is a valid HKDF length");
    out
}

/// Normalizes the cipher name variants the spec lists.
pub fn normalize_cipher(name: &str) -> String {
    match name.to_ascii_uppercase().as_str() {
        "CHACHA20" | "CHACHA20POLY1305" | "CHACHA20-POLY1305" => AEAD_CIPHER.to_string(),
        "RC4-128" | "ARCFOUR" => "RC4".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_sha256_known_answer() {
        // RFC 4231 test case 2
        let mac = MacAlg::HmacSha256.mac(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            mac.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn keys_differ_by_direction() {
        let k = aead_keys(MacAlg::HmacSha256, b"secret", &[7u8; 64]).unwrap();
        assert_ne!(k.encode, k.decode);
        assert!(aead_keys(MacAlg::Inverse, b"secret", &[7u8; 64]).is_none());
    }
}
