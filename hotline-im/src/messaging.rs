//! The messaging objects of the 800 block, parsed out of field lists
//! (Capabilities-Messaging.md; guide §9–§12).

use crate::text::TextMode;
use crate::wire::{field, Field, Transaction};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Presence {
    Offline,
    Online,
    Away,
    Invisible,
    Busy,
}

impl Presence {
    pub fn from_u16(v: u16) -> Self {
        match v {
            1 => Presence::Online,
            2 => Presence::Away,
            3 => Presence::Invisible,
            4 => Presence::Busy,
            _ => Presence::Offline,
        }
    }
    pub fn to_u16(self) -> u16 {
        match self {
            Presence::Offline => 0,
            Presence::Online => 1,
            Presence::Away => 2,
            Presence::Invisible => 3,
            Presence::Busy => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RosterState {
    Removed,
    PendingOut,
    PendingIn,
    Accepted,
    Blocked,
}

impl RosterState {
    pub fn from_u16(v: u16) -> Self {
        match v {
            1 => RosterState::PendingOut,
            2 => RosterState::PendingIn,
            3 => RosterState::Accepted,
            4 => RosterState::Blocked,
            _ => RosterState::Removed,
        }
    }
    pub fn to_u16(self) -> u16 {
        match self {
            RosterState::Removed => 0,
            RosterState::PendingOut => 1,
            RosterState::PendingIn => 2,
            RosterState::Accepted => 3,
            RosterState::Blocked => 4,
        }
    }
}

/// One friend as the server holds it. An 801 replaces the whole entry (guide §9.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RosterEntry {
    pub login: String,
    /// Our private alias for them (824).
    pub nickname: Option<String>,
    /// The name they go by (field 102); survives them signing off.
    pub display_name: Option<String>,
    pub state: RosterState,
    pub presence: Presence,
    pub status_text: Option<String>,
    pub capabilities: Option<u16>,
    /// Hex of `DATA_BUDDY_ICON_HASH`; None means they have no icon.
    pub icon_hash: Option<String>,
}

impl RosterEntry {
    pub fn parse(group: &[Field], text: TextMode) -> Option<Self> {
        let mut e = RosterEntry {
            login: String::new(),
            nickname: None,
            display_name: None,
            state: RosterState::Accepted,
            presence: Presence::Offline,
            status_text: None,
            capabilities: None,
            icon_hash: None,
        };
        let mut have_login = false;
        for f in group {
            match f.id {
                field::FRIEND_LOGIN if !have_login => {
                    e.login = text.decode(&f.data);
                    have_login = true;
                }
                field::FRIEND_NICKNAME => e.nickname = non_empty(text.decode(&f.data)),
                field::USER_NAME => e.display_name = non_empty(text.decode(&f.data)),
                field::ROSTER_STATE => e.state = RosterState::from_u16(f.uint() as u16),
                field::PRESENCE_STATE => e.presence = Presence::from_u16(f.uint() as u16),
                field::PRESENCE_STATUS_TEXT => e.status_text = non_empty(text.decode(&f.data)),
                field::FRIEND_CAPABILITIES => e.capabilities = Some(f.uint() as u16),
                field::BUDDY_ICON_HASH if !f.data.is_empty() => e.icon_hash = Some(hex(&f.data)),
                _ => {}
            }
        }
        have_login.then_some(e)
    }

    /// Alias, then published name, then the bare Login (guide §9.1).
    pub fn shown_name(&self) -> &str {
        self.nickname
            .as_deref()
            .or(self.display_name.as_deref())
            .unwrap_or(&self.login)
    }
}

/// Presence Changed (809). Status, capabilities and the icon hash are complete:
/// absent means none. The display name is kept when absent (guide §11.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenceUpdate {
    pub login: String,
    pub presence: Presence,
    pub status_text: Option<String>,
    pub capabilities: Option<u16>,
    pub display_name: Option<String>,
    pub icon_hash: Option<String>,
}

impl PresenceUpdate {
    pub fn parse(t: &Transaction, text: TextMode) -> Option<Self> {
        Some(PresenceUpdate {
            login: text.decode(t.bytes(field::FRIEND_LOGIN)?),
            presence: Presence::from_u16(t.uint(field::PRESENCE_STATE).unwrap_or(0) as u16),
            status_text: t
                .bytes(field::PRESENCE_STATUS_TEXT)
                .and_then(|b| non_empty(text.decode(b))),
            capabilities: t.uint(field::FRIEND_CAPABILITIES).map(|v| v as u16),
            display_name: t
                .bytes(field::USER_NAME)
                .and_then(|b| non_empty(text.decode(b))),
            icon_hash: t.bytes(field::BUDDY_ICON_HASH).filter(|b| !b.is_empty()).map(hex),
        })
    }

    pub fn apply(&self, e: &mut RosterEntry) {
        e.presence = self.presence;
        e.status_text = self.status_text.clone();
        e.capabilities = self.capabilities;
        e.icon_hash = self.icon_hash.clone();
        if self.display_name.is_some() {
            e.display_name = self.display_name.clone();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingMessage {
    /// Hex of the 16-byte GUID.
    pub guid: String,
    pub from: String,
    pub body: String,
    /// Unix seconds, server-stamped.
    pub timestamp: u64,
}

impl IncomingMessage {
    pub fn parse(t: &Transaction, text: TextMode) -> Option<Self> {
        Some(IncomingMessage {
            guid: hex(t.bytes(field::MESSAGE_GUID)?),
            from: text.decode(t.bytes(field::FRIEND_LOGIN)?),
            body: text.decode(t.bytes(field::MESSAGE_BODY).unwrap_or_default()),
            timestamp: t.uint(field::MESSAGE_TIMESTAMP).unwrap_or(0),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AckKind {
    Delivered,
    Read,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub nickname: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub gender: u16,
    /// year, month, day; 0 = not given
    pub birth: (u16, u8, u8),
    pub country: Option<String>,
    pub postcode: Option<String>,
    pub languages: Vec<String>,
}

impl Profile {
    pub fn parse(t: &Transaction, text: TextMode) -> Self {
        let s = |id| t.bytes(id).and_then(|b| non_empty(text.decode(b)));
        let birth = t
            .bytes(field::PROFILE_BIRTHDATE)
            .filter(|b| b.len() == 4)
            .map(|b| (u16::from_be_bytes([b[0], b[1]]), b[2], b[3]))
            .unwrap_or((0, 0, 0));
        Profile {
            nickname: s(field::PROFILE_NICKNAME),
            first_name: s(field::PROFILE_FIRST_NAME),
            last_name: s(field::PROFILE_LAST_NAME),
            email: s(field::PROFILE_EMAIL),
            gender: t.uint(field::PROFILE_GENDER).unwrap_or(0) as u16,
            birth,
            country: s(field::PROFILE_COUNTRY),
            postcode: s(field::PROFILE_POSTCODE),
            languages: t
                .fields
                .iter()
                .filter(|f| f.id == field::PROFILE_LANGUAGE)
                .map(|f| text.decode(&f.data))
                .take(3)
                .collect(),
        }
    }

    /// Set User Info replaces the whole profile, so every kept field is sent.
    pub fn to_fields(&self, text: TextMode) -> Vec<Field> {
        let mut out = Vec::new();
        let mut s = |id, v: &Option<String>| {
            if let Some(v) = v {
                out.push(Field::new(id, text.encode(v)));
            }
        };
        s(field::PROFILE_NICKNAME, &self.nickname);
        s(field::PROFILE_FIRST_NAME, &self.first_name);
        s(field::PROFILE_LAST_NAME, &self.last_name);
        s(field::PROFILE_EMAIL, &self.email);
        s(field::PROFILE_COUNTRY, &self.country);
        s(field::PROFILE_POSTCODE, &self.postcode);
        out.push(Field::u16(field::PROFILE_GENDER, self.gender));
        let (y, m, d) = self.birth;
        if (y, m, d) != (0, 0, 0) {
            let mut b = y.to_be_bytes().to_vec();
            b.extend([m, d]);
            out.push(Field::new(field::PROFILE_BIRTHDATE, b));
        }
        for l in self.languages.iter().take(3) {
            out.push(Field::new(field::PROFILE_LANGUAGE, text.encode(l)));
        }
        out
    }
}

/// Someone on a server's user list (field 300, or a Notify Change User 301).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatUser {
    pub id: u16,
    pub icon: u16,
    /// bit 0 away, bit 1 admin, bit 2 refuses PMs, bit 3 refuses private chat
    pub flags: u16,
    pub name: String,
}

impl ChatUser {
    /// `id(2) icon(2) flags(2) name_len(2) name`
    pub fn parse(d: &[u8], text: TextMode) -> Option<Self> {
        if d.len() < 8 {
            return None;
        }
        let u = |i: usize| u16::from_be_bytes([d[i], d[i + 1]]);
        let len = u(6) as usize;
        let name = d.get(8..8 + len.min(d.len() - 8))?;
        Some(ChatUser {
            id: u(0),
            icon: u(2),
            flags: u(4),
            name: text.decode(name).trim().to_string(),
        })
    }

    pub fn from_fields(t: &Transaction, text: TextMode) -> Option<Self> {
        Some(ChatUser {
            id: t.uint(field::USER_ID)? as u16,
            icon: t.uint(field::USER_ICON_ID).unwrap_or(0) as u16,
            flags: t.uint(field::USER_FLAGS).unwrap_or(0) as u16,
            name: t
                .bytes(field::USER_NAME)
                .map(|b| text.decode(b).trim().to_string())
                .unwrap_or_default(),
        })
    }

    pub fn is_away(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn is_admin(&self) -> bool {
        self.flags & 2 != 0
    }
}

/// `DATA_REASON_CODE` (guide §16).
pub fn reason_text(code: u16) -> &'static str {
    match code {
        0 => "OK",
        1 => "No such user.",
        2 => "That account can't use messaging.",
        3 => "You can't message this user.",
        4 => "You're already buddies.",
        5 => "Request already sent.",
        6 => "You must be buddies to do that.",
        7 => "Will deliver when they sign on.",
        8 => "User is offline.",
        9 => "Their inbox is full; try later.",
        10 => "Slow down a little.",
        11 => "No one found.",
        12 => "Your Buddy List is full.",
        13 => "That message (or picture) is too big.",
        14 => "The server couldn't use that picture.",
        _ => "The server refused.",
    }
}

pub mod reason {
    pub const OK: u16 = 0;
    pub const OFFLINE_QUEUED: u16 = 7;
    pub const NOT_FRIENDS: u16 = 6;
    pub const RATE_LIMITED: u16 = 10;
    pub const MESSAGE_TOO_LONG: u16 = 13;
    pub const INVALID_IMAGE: u16 = 14;
}

fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then_some(s)
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// A fresh UUIDv4 from the OS generator (a reused GUID is silently dropped by the server).
pub fn new_guid() -> [u8; 16] {
    let mut g: [u8; 16] = rand::random();
    g[6] = (g[6] & 0x0F) | 0x40;
    g[8] = (g[8] & 0x3F) | 0x80;
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_entry_uses_its_own_group() {
        let t = Transaction::request(
            800,
            vec![
                Field::new(field::FRIEND_LOGIN, "alice"),
                Field::u16(field::ROSTER_STATE, 3),
                Field::new(field::USER_NAME, "Alice"),
                Field::u16(field::PRESENCE_STATE, 2),
                Field::new(field::FRIEND_LOGIN, "bob"),
                Field::u16(field::ROSTER_STATE, 2),
            ],
        );
        let e: Vec<_> = t
            .entries()
            .into_iter()
            .filter_map(|g| RosterEntry::parse(g, TextMode::Utf8))
            .collect();
        assert_eq!(e[0].shown_name(), "Alice");
        assert_eq!(e[0].presence, Presence::Away);
        assert_eq!(e[1].shown_name(), "bob");
        assert_eq!(e[1].state, RosterState::PendingIn);
        assert_eq!(e[1].display_name, None);
    }

    #[test]
    fn guid_is_v4() {
        let g = new_guid();
        assert_eq!(g[6] >> 4, 4);
        assert_ne!(new_guid(), g);
        assert_eq!(unhex(&hex(&g)).unwrap(), g);
    }
}
