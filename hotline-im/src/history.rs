//! A server's record of public chat (Capabilities-Chat-History.md): Get Chat History (700)
//! with cursors, for showing what was said before you joined and scrolling back.

use serde::Serialize;

use crate::client::{Client, Error};
use crate::text::TextMode;
use crate::wire::{field, tx, Field};

/// One remembered chat line.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// The server's ID: higher is newer, and it's the cursor for the next page.
    pub id: u64,
    /// Unix seconds.
    pub timestamp: i64,
    pub nick: String,
    pub text: String,
    pub icon: u16,
    /// A `/me` emote.
    pub emote: bool,
    /// From the server (an admin broadcast), not a person.
    pub server: bool,
    /// Removed by an admin; show a placeholder.
    pub deleted: bool,
}

impl HistoryEntry {
    /// `id(8) time(8) flags(2) icon(2) nickLen(2) nick msgLen(2) msg [sub-fields]`
    pub fn parse(d: &[u8], text: TextMode) -> Option<Self> {
        let u16_at = |i: usize| d.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
        if d.len() < 24 {
            return None;
        }
        let id = u64::from_be_bytes(d[0..8].try_into().ok()?);
        let timestamp = i64::from_be_bytes(d[8..16].try_into().ok()?);
        let flags = u16_at(16)?;
        let icon = u16_at(18)?;
        let nick_len = u16_at(20)? as usize;
        let nick = d.get(22..22 + nick_len)?;
        let msg_len = u16_at(22 + nick_len)? as usize;
        let msg = d.get(24 + nick_len..24 + nick_len + msg_len)?;
        Some(HistoryEntry {
            id,
            timestamp,
            nick: text.decode(nick).trim().to_string(),
            text: text.decode(msg),
            icon,
            emote: flags & 1 != 0,
            server: flags & 2 != 0,
            deleted: flags & 4 != 0,
        })
    }

    /// The packed form (the test server writes these).
    pub fn pack(&self, text: TextMode) -> Vec<u8> {
        let nick = text.encode(&self.nick);
        let msg = text.encode(&self.text);
        let flags = self.emote as u16 | (self.server as u16) << 1 | (self.deleted as u16) << 2;
        let mut d = Vec::with_capacity(24 + nick.len() + msg.len());
        d.extend(self.id.to_be_bytes());
        d.extend(self.timestamp.to_be_bytes());
        d.extend(flags.to_be_bytes());
        d.extend(self.icon.to_be_bytes());
        d.extend((nick.len() as u16).to_be_bytes());
        d.extend(nick);
        d.extend((msg.len() as u16).to_be_bytes());
        d.extend(msg);
        d
    }
}

/// A batch of history, oldest first.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    /// More exist in the direction asked (older for `before` or no cursor, newer for `after`).
    pub has_more: bool,
}

impl Client {
    /// Public chat's history: the latest `limit` lines, or those before / after a message ID.
    pub async fn chat_history(&self, before: Option<u64>, after: Option<u64>, limit: u16) -> Result<HistoryPage, Error> {
        if !self.info.chat_history {
            return Err(Error::Server { reason: None, text: "This server doesn't keep chat history.".into() });
        }
        let mut f = vec![Field::u32(field::CHANNEL_ID, 0), Field::u16(field::HISTORY_LIMIT, limit)];
        if let Some(b) = before {
            f.push(Field::new(field::HISTORY_BEFORE, b.to_be_bytes()));
        }
        if let Some(a) = after {
            f.push(Field::new(field::HISTORY_AFTER, a.to_be_bytes()));
        }
        let r = self.request(tx::GET_CHAT_HISTORY, f).await?;
        let mut entries: Vec<HistoryEntry> = r
            .t
            .fields
            .iter()
            .filter(|f| f.id == field::HISTORY_ENTRY)
            .filter_map(|f| HistoryEntry::parse(&f.data, self.text))
            .collect();
        entries.sort_by_key(|e| e.id);
        Ok(HistoryPage { entries, has_more: r.t.uint(field::HISTORY_HAS_MORE).unwrap_or(0) != 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_round_trip_and_sub_fields_skipped() {
        let e = HistoryEntry {
            id: 42, timestamp: 1_700_000_000, nick: "Tagban".into(), text: "hi all".into(),
            icon: 128, emote: true, server: false, deleted: false,
        };
        let mut d = e.pack(TextMode::Utf8);
        d.extend([0x00, 0x01, 0x00, 0x04, 1, 2, 3, 4]); // an unknown sub-field
        assert_eq!(HistoryEntry::parse(&d, TextMode::Utf8), Some(e));
        assert_eq!(HistoryEntry::parse(&d[..10], TextMode::Utf8), None);
    }
}
