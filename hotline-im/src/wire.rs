//! Transactions on the data port: the 20-byte header and the field list
//! (Client-Creation-Guide §5). Every integer is big-endian.

use std::fmt;

/// Transaction types this client sends or handles.
pub mod tx {
    pub const LOGIN: u16 = 107;
    pub const SHOW_AGREEMENT: u16 = 109;
    pub const AGREED: u16 = 121;
    pub const DISCONNECT_MSG: u16 = 111;
    pub const SERVER_MSG: u16 = 104;
    pub const SEND_CHAT: u16 = 105;
    pub const CHAT_MSG: u16 = 106;
    pub const GET_USER_NAME_LIST: u16 = 300;
    pub const NOTIFY_CHANGE_USER: u16 = 301;
    pub const NOTIFY_DELETE_USER: u16 = 302;
    pub const SET_CLIENT_USER_INFO: u16 = 304;

    pub const GET_ROSTER: u16 = 800;
    pub const ROSTER_ENTRY: u16 = 801;
    pub const ADD_FRIEND: u16 = 802;
    pub const REMOVE_FRIEND: u16 = 803;
    pub const FRIEND_REQUEST: u16 = 804;
    pub const FRIEND_RESPONSE: u16 = 805;
    pub const BLOCK_USER: u16 = 806;
    pub const UNBLOCK_USER: u16 = 807;
    pub const SET_PRESENCE: u16 = 808;
    pub const PRESENCE_CHANGED: u16 = 809;
    pub const IM_SEND: u16 = 810;
    pub const IM_DELIVER: u16 = 811;
    pub const IM_ACK: u16 = 812;
    pub const IM_TYPING: u16 = 813;
    pub const FIND_USER: u16 = 822;
    pub const USER_SEARCH: u16 = 823;
    pub const SET_FRIEND_NICKNAME: u16 = 824;
    pub const GET_USER_INFO: u16 = 825;
    pub const SET_USER_INFO: u16 = 826;
    /// Buddy icons (Capabilities-Buddy-Icons.md). 827 also arrives as a notification
    /// when another of our own sessions changes the icon.
    pub const SET_BUDDY_ICON: u16 = 827;
    pub const GET_BUDDY_ICON: u16 = 828;
}

/// Field IDs (guide Appendix B).
pub mod field {
    pub const ERROR: u16 = 100;
    pub const DATA: u16 = 101;
    pub const USER_NAME: u16 = 102;
    pub const USER_ID: u16 = 103;
    pub const USER_ICON_ID: u16 = 104;
    pub const USER_LOGIN: u16 = 105;
    pub const USER_PASSWORD: u16 = 106;
    pub const CHAT_OPTIONS: u16 = 109;
    pub const USER_FLAGS: u16 = 112;
    pub const OPTIONS: u16 = 113;
    pub const CHAT_ID: u16 = 114;
    pub const USER_NAME_WITH_INFO: u16 = 300;
    pub const NO_AGREEMENT: u16 = 154;
    pub const VERSION: u16 = 160;
    pub const SERVER_NAME: u16 = 162;
    pub const CAPABILITIES: u16 = 0x01F0;

    pub const HOPE_APP_ID: u16 = 0x0E01;
    pub const HOPE_APP_STRING: u16 = 0x0E02;
    pub const HOPE_SESSION_KEY: u16 = 0x0E03;
    pub const HOPE_MAC_ALGORITHM: u16 = 0x0E04;
    pub const HOPE_SERVER_CIPHER: u16 = 0x0EC1;
    pub const HOPE_CLIENT_CIPHER: u16 = 0x0EC2;
    pub const HOPE_SERVER_CIPHER_MODE: u16 = 0x0EC3;
    pub const HOPE_CLIENT_CIPHER_MODE: u16 = 0x0EC4;

    pub const FRIEND_LOGIN: u16 = 0x0600;
    pub const FRIEND_NICKNAME: u16 = 0x0601;
    pub const PRESENCE_STATE: u16 = 0x0602;
    pub const PRESENCE_STATUS_TEXT: u16 = 0x0603;
    pub const ROSTER_STATE: u16 = 0x0604;
    pub const MESSAGE_GUID: u16 = 0x0605;
    pub const MESSAGE_BODY: u16 = 0x0606;
    pub const MESSAGE_TIMESTAMP: u16 = 0x0607;
    pub const ACK_TYPE: u16 = 0x0608;
    pub const TYPING_STATE: u16 = 0x0609;
    pub const REASON_CODE: u16 = 0x060F;
    pub const REQUEST_NOTE: u16 = 0x0610;
    pub const DISCOVERABLE: u16 = 0x0611;
    pub const SEARCH_QUERY: u16 = 0x0612;
    pub const FRIEND_CAPABILITIES: u16 = 0x0613;
    pub const PROFILE_NICKNAME: u16 = 0x0614;
    pub const PROFILE_FIRST_NAME: u16 = 0x0615;
    pub const PROFILE_LAST_NAME: u16 = 0x0616;
    pub const PROFILE_EMAIL: u16 = 0x0617;
    pub const PROFILE_GENDER: u16 = 0x0618;
    pub const PROFILE_BIRTHDATE: u16 = 0x0619;
    pub const PROFILE_COUNTRY: u16 = 0x061A;
    pub const PROFILE_POSTCODE: u16 = 0x061B;
    pub const PROFILE_LANGUAGE: u16 = 0x061C;
    pub const MAX_MESSAGE_BYTES: u16 = 0x0620;
    pub const MAX_ROSTER_SIZE: u16 = 0x0621;
    pub const MAX_OFFLINE_QUEUE: u16 = 0x0622;
    pub const BUDDY_ICON: u16 = 0x061D;
    pub const BUDDY_ICON_HASH: u16 = 0x061E;
    pub const MAX_ICON_BYTES: u16 = 0x0623;
    pub const MAX_ICON_DIMENSION: u16 = 0x0624;
}

/// `DATA_CAPABILITIES` bits (guide §7.1).
pub mod cap {
    pub const LARGE_FILES: u16 = 1 << 0;
    pub const TEXT_ENCODING: u16 = 1 << 1;
    pub const VOICE: u16 = 1 << 2;
    pub const MESSAGING: u16 = 1 << 6;
    pub const DIRECT_TRANSFER: u16 = 1 << 7;
    pub const MESSENGER_SESSION: u16 = 1 << 8;
}

pub const HEADER_LEN: usize = 20;

#[derive(Clone, PartialEq, Eq)]
pub struct Field {
    pub id: u16,
    pub data: Vec<u8>,
}

impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#06x}:{:02x?}", self.id, self.data)
    }
}

impl Field {
    pub fn new(id: u16, data: impl Into<Vec<u8>>) -> Self {
        Field {
            id,
            data: data.into(),
        }
    }
    pub fn u16(id: u16, v: u16) -> Self {
        Field::new(id, v.to_be_bytes())
    }
    pub fn u32(id: u16, v: u32) -> Self {
        Field::new(id, v.to_be_bytes())
    }
    /// Legacy integer fields use the minimal width: 2 bytes when it fits, else 4.
    pub fn int(id: u16, v: u32) -> Self {
        if v <= 0xFFFF {
            Field::u16(id, v as u16)
        } else {
            Field::u32(id, v)
        }
    }
    /// Reads the field as a big-endian integer over its whole size (guide §5.3).
    pub fn uint(&self) -> u64 {
        self.data
            .iter()
            .take(8)
            .fold(0u64, |acc, b| (acc << 8) | *b as u64)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transaction {
    pub flags: u8,
    pub is_reply: bool,
    pub ty: u16,
    pub id: u32,
    pub error: u32,
    pub fields: Vec<Field>,
}

impl Transaction {
    pub fn request(ty: u16, fields: Vec<Field>) -> Self {
        Transaction {
            ty,
            fields,
            ..Default::default()
        }
    }

    pub fn reply_to(req: &Transaction, fields: Vec<Field>) -> Self {
        // Janus answers with type 0; the client routes replies by task ID only.
        Transaction {
            is_reply: true,
            id: req.id,
            fields,
            ..Default::default()
        }
    }

    pub fn get(&self, id: u16) -> Option<&Field> {
        self.fields.iter().find(|f| f.id == id)
    }

    pub fn has(&self, id: u16) -> bool {
        self.get(id).is_some()
    }

    pub fn uint(&self, id: u16) -> Option<u64> {
        self.get(id).map(Field::uint)
    }

    pub fn bytes(&self, id: u16) -> Option<&[u8]> {
        self.get(id).map(|f| f.data.as_slice())
    }

    /// Splits the field list into entry groups, each opened by `DATA_FRIEND_LOGIN`
    /// (guide §5.5). Fields before the first delimiter are dropped.
    pub fn entries(&self) -> Vec<&[Field]> {
        let mut out = Vec::new();
        let mut start = None;
        for (i, f) in self.fields.iter().enumerate() {
            if f.id == field::FRIEND_LOGIN {
                if let Some(s) = start {
                    out.push(&self.fields[s..i]);
                }
                start = Some(i);
            }
        }
        if let Some(s) = start {
            out.push(&self.fields[s..]);
        }
        out
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&(self.fields.len() as u16).to_be_bytes());
        for f in &self.fields {
            body.extend_from_slice(&f.id.to_be_bytes());
            body.extend_from_slice(&(f.data.len() as u16).to_be_bytes());
            body.extend_from_slice(&f.data);
        }
        // A transaction with no fields may carry an empty payload; classic servers do both.
        let mut out = Vec::with_capacity(HEADER_LEN + body.len());
        out.push(self.flags);
        out.push(self.is_reply as u8);
        out.extend_from_slice(&self.ty.to_be_bytes());
        out.extend_from_slice(&self.id.to_be_bytes());
        out.extend_from_slice(&self.error.to_be_bytes());
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// Parses a header and its complete payload.
    pub fn decode(header: &[u8; HEADER_LEN], payload: &[u8]) -> Result<Self, WireError> {
        let mut t = Transaction {
            flags: header[0],
            is_reply: header[1] != 0,
            ty: u16::from_be_bytes([header[2], header[3]]),
            id: u32::from_be_bytes(header[4..8].try_into().unwrap()),
            error: u32::from_be_bytes(header[8..12].try_into().unwrap()),
            fields: Vec::new(),
        };
        if payload.len() < 2 {
            return Ok(t);
        }
        let count = u16::from_be_bytes([payload[0], payload[1]]) as usize;
        let mut p = 2;
        for _ in 0..count {
            if p + 4 > payload.len() {
                return Err(WireError::Truncated);
            }
            let id = u16::from_be_bytes([payload[p], payload[p + 1]]);
            let len = u16::from_be_bytes([payload[p + 2], payload[p + 3]]) as usize;
            p += 4;
            if p + len > payload.len() {
                return Err(WireError::Truncated);
            }
            t.fields.push(Field::new(id, &payload[p..p + len]));
            p += len;
        }
        Ok(t)
    }
}

/// Total and part sizes from a header (offsets 12 and 16).
pub fn header_sizes(header: &[u8; HEADER_LEN]) -> (u32, u32) {
    (
        u32::from_be_bytes(header[12..16].try_into().unwrap()),
        u32::from_be_bytes(header[16..20].try_into().unwrap()),
    )
}

#[derive(Debug)]
pub enum WireError {
    Truncated,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::Truncated => write!(f, "truncated transaction"),
        }
    }
}

impl std::error::Error for WireError {}

/// HOPE algorithm list: `u16 count, [u8 len, name]*`.
pub fn encode_name_list(names: &[&str]) -> Vec<u8> {
    let mut out = (names.len() as u16).to_be_bytes().to_vec();
    for n in names {
        out.push(n.len() as u8);
        out.extend_from_slice(n.as_bytes());
    }
    out
}

pub fn decode_name_list(data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    if data.len() < 2 {
        return out;
    }
    let count = u16::from_be_bytes([data[0], data[1]]) as usize;
    let mut p = 2;
    for _ in 0..count {
        let Some(&len) = data.get(p) else { break };
        let len = len as usize;
        let Some(name) = data.get(p + 1..p + 1 + len) else {
            break;
        };
        out.push(String::from_utf8_lossy(name).into_owned());
        p += 1 + len;
    }
    out
}

pub fn invert(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(|b| !b).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Appendix D of the guide, byte for byte.
    #[test]
    fn im_send_matches_worked_example() {
        let guid: Vec<u8> = (0..16)
            .map(|i| [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF][i % 8])
            .collect();
        let mut t = Transaction::request(
            tx::IM_SEND,
            vec![
                Field::new(field::FRIEND_LOGIN, "alice"),
                Field::new(field::MESSAGE_GUID, guid),
                Field::new(field::MESSAGE_BODY, "hi"),
            ],
        );
        t.id = 7;
        let bytes = t.encode();
        assert_eq!(
            &bytes[..20],
            &[0, 0, 0x03, 0x2A, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0x25, 0, 0, 0, 0x25]
        );
        assert_eq!(bytes.len(), 57);
        let back = Transaction::decode(bytes[..20].try_into().unwrap(), &bytes[20..]).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn entries_split_on_friend_login() {
        let t = Transaction::request(
            0,
            vec![
                Field::u16(field::REASON_CODE, 0),
                Field::new(field::FRIEND_LOGIN, "a"),
                Field::new(field::USER_NAME, "Alice"),
                Field::new(field::FRIEND_LOGIN, "b"),
            ],
        );
        let e = t.entries();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].len(), 2);
        assert_eq!(e[1].len(), 1);
    }

    #[test]
    fn name_list_round_trip() {
        let enc = encode_name_list(&["HMAC-SHA256", "HMAC-SHA1", "INVERSE"]);
        assert_eq!(&enc[..3], &[0, 3, 11]);
        assert_eq!(
            decode_name_list(&enc),
            vec!["HMAC-SHA256", "HMAC-SHA1", "INVERSE"]
        );
    }
}
