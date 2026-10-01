//! Pictures in chat (Capabilities-Inline-Media.md) and GIF icons (GIF-Icons.md), for
//! classic sessions (chat rooms). Both are optional: a server that doesn't confirm
//! `CAPABILITY_INLINE_MEDIA` gets no media, and one that doesn't know the GIF icon
//! transactions just fails them.

use std::time::Duration;

use serde::Serialize;

use crate::client::{Client, Error};
use crate::messaging::{hex, unhex};
use crate::wire::{field, tx, Field, Transaction};

/// The most one field can carry (16-bit lengths).
const FIELD_MAX: usize = 65_535;
/// The spec's defaults when the server leaves a limit out.
const DEFAULT_MAX_BYTES: u32 = 256 * 1024;
const DEFAULT_MAX_DIMENSION: u32 = 2048;
const DEFAULT_CHUNK: u32 = 32 * 1024;
/// GIF icon calls get a short wait: a server without them may never answer.
const ICON_TIMEOUT: Duration = Duration::from_secs(8);

/// What the server accepts, from its login reply (all advisory; it checks again).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaLimits {
    pub max_bytes: u32,
    pub max_dimension: u32,
    pub max_pixels: u32,
    pub chunk_size: u32,
    pub max_frames: u32,
    pub max_duration_ms: u32,
}

impl MediaLimits {
    pub(crate) fn parse(reply: &Transaction) -> Self {
        let v = |id, d: u32| reply.uint(id).map(|v| v as u32).filter(|v| *v != 0).unwrap_or(d);
        let max_dimension = v(field::MEDIA_MAX_DIMENSION, DEFAULT_MAX_DIMENSION);
        MediaLimits {
            max_bytes: v(field::MEDIA_MAX_BYTES, DEFAULT_MAX_BYTES),
            max_dimension,
            max_pixels: v(field::MEDIA_MAX_PIXELS, max_dimension.saturating_mul(max_dimension)),
            chunk_size: v(field::MEDIA_CHUNK_SIZE, DEFAULT_CHUNK).min(FIELD_MAX as u32),
            max_frames: v(field::MEDIA_MAX_FRAMES, 150),
            max_duration_ms: v(field::MEDIA_MAX_DURATION_MS, 15_000),
        }
    }
}

/// A picture the server holds, as a chat line or upload reply names it.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaRef {
    /// Hex of the server's opaque handle.
    pub id: String,
    /// The canonical MIME type ("image/png", "image/jpeg", "image/gif").
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u32,
}

impl MediaRef {
    /// The handle and type come as a pair; one without the other is ignored.
    pub(crate) fn parse(t: &Transaction) -> Option<Self> {
        let id = t.bytes(field::MEDIA_ID).filter(|b| !b.is_empty())?;
        let mime = t.bytes(field::MEDIA_TYPE)?;
        Some(MediaRef {
            id: hex(id),
            mime: String::from_utf8_lossy(mime).into_owned(),
            width: t.uint(field::MEDIA_WIDTH).unwrap_or(0) as u32,
            height: t.uint(field::MEDIA_HEIGHT).unwrap_or(0) as u32,
            bytes: t.uint(field::MEDIA_BYTES).unwrap_or(0) as u32,
        })
    }

    pub(crate) fn fields(&self) -> Option<[Field; 2]> {
        Some([
            Field::new(field::MEDIA_ID, unhex(&self.id)?),
            Field::new(field::MEDIA_TYPE, self.mime.as_bytes()),
        ])
    }
}

/// "image/png" for PNG bytes, and so on; a hint only (the server sniffs for itself).
pub fn mime_of(b: &[u8]) -> &'static str {
    if b.starts_with(b"\x89PNG") {
        "image/png"
    } else if b.starts_with(b"GIF8") {
        "image/gif"
    } else if b.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}

impl Client {
    /// Pictures can be sent here: the server confirmed inline media.
    pub fn media_limits(&self) -> Option<&MediaLimits> {
        self.info.media.as_ref()
    }

    /// Uploads a picture (Upload Media, 750), in chunks when it's bigger than one field.
    /// The handle goes in a chat line with `send_chat_media` or `send_private_media`.
    pub async fn upload_media(&self, data: &[u8]) -> Result<MediaRef, Error> {
        let limits = self.media_limits().ok_or_else(|| Error::Server {
            reason: None,
            text: "This server doesn't take pictures.".into(),
        })?;
        if data.len() > limits.max_bytes as usize {
            return Err(Error::Server { reason: None, text: "That picture is too big for this server.".into() });
        }
        let chunk = (limits.chunk_size as usize).clamp(1024, FIELD_MAX);
        let mime = mime_of(data);
        if data.len() <= chunk {
            let r = self
                .request(
                    tx::UPLOAD_MEDIA,
                    vec![
                        Field::new(field::MEDIA_PAYLOAD, data),
                        Field::new(field::MEDIA_DECLARED_TYPE, mime),
                        Field::new(field::MEDIA_PART_FINAL, [1u8]),
                    ],
                )
                .await?;
            return uploaded(&r.t);
        }
        let parts: Vec<&[u8]> = data.chunks(chunk).collect();
        let n = parts.len();
        let first = self
            .request(
                tx::UPLOAD_MEDIA,
                vec![
                    Field::new(field::MEDIA_PAYLOAD, parts[0]),
                    Field::new(field::MEDIA_DECLARED_TYPE, mime),
                    Field::u16(field::MEDIA_PART_INDEX, 0),
                    Field::u16(field::MEDIA_PART_COUNT, n as u16),
                ],
            )
            .await?;
        let token = first
            .t
            .bytes(field::MEDIA_UPLOAD_TOKEN)
            .ok_or_else(|| Error::Server { reason: None, text: "The server didn't start the upload.".into() })?
            .to_vec();
        let mut last = first;
        for (i, part) in parts.iter().enumerate().skip(1) {
            let mut f = vec![
                Field::new(field::MEDIA_UPLOAD_TOKEN, token.clone()),
                Field::new(field::MEDIA_PAYLOAD, *part),
                Field::u16(field::MEDIA_PART_INDEX, i as u16),
            ];
            if i == n - 1 {
                f.push(Field::new(field::MEDIA_PART_FINAL, [1u8]));
            }
            last = self.request(tx::UPLOAD_MEDIA, f).await?;
        }
        uploaded(&last.t)
    }

    /// Fetches a picture named in a chat line (Download Media, 751): its bytes and type.
    pub async fn download_media(&self, id: &str) -> Result<(Vec<u8>, String), Error> {
        let handle = unhex(id).ok_or_else(|| Error::Server { reason: None, text: "Bad picture handle.".into() })?;
        let max = self.media_limits().map(|l| l.max_bytes as usize).unwrap_or(DEFAULT_MAX_BYTES as usize);
        let mut out = Vec::new();
        let mut mime = String::new();
        let mut index = 0u16;
        loop {
            let mut f = vec![Field::new(field::MEDIA_ID, handle.clone())];
            if index > 0 {
                f.push(Field::u16(field::MEDIA_PART_INDEX, index));
            }
            let r = self.request(tx::DOWNLOAD_MEDIA, f).await?;
            out.extend_from_slice(r.t.bytes(field::MEDIA_PAYLOAD).unwrap_or_default());
            if let Some(m) = r.t.bytes(field::MEDIA_TYPE) {
                mime = String::from_utf8_lossy(m).into_owned();
            }
            if out.len() > max {
                return Err(Error::Server { reason: None, text: "The picture was bigger than the server allows.".into() });
            }
            let count = r.t.uint(field::MEDIA_PART_COUNT).unwrap_or(1) as u16;
            index += 1;
            if r.t.uint(field::MEDIA_PART_FINAL).unwrap_or(1) != 0 || index >= count {
                break;
            }
        }
        Ok((out, mime))
    }

    /// Public chat with a picture attached; `text` is what clients without pictures see.
    pub fn send_chat_media(&self, text: &str, media: &MediaRef) {
        let mut f = vec![Field::new(field::DATA, self.text.encode(text))];
        f.extend(media.fields().into_iter().flatten());
        self.notify(tx::SEND_CHAT, f);
    }

    /// A classic private message (108) to a user ID, optionally with a picture.
    pub async fn send_private(&self, user_id: u16, text: &str, media: Option<&MediaRef>) -> Result<(), Error> {
        let mut f = vec![
            Field::u16(field::USER_ID, user_id),
            Field::new(field::DATA, self.text.encode(text)),
        ];
        if let Some(m) = media {
            f.extend(m.fields().into_iter().flatten());
        }
        self.request(tx::SEND_INSTANT_MSG, f).await.map(|_| ())
    }

    // ---------- GIF icons ----------

    async fn icon_request(&self, ty: u16, fields: Vec<Field>) -> Result<Transaction, Error> {
        match tokio::time::timeout(ICON_TIMEOUT, self.request(ty, fields)).await {
            Ok(r) => r.map(|r| r.t),
            Err(_) => Err(Error::Timeout),
        }
    }

    /// Everyone's GIF icon (Get Icon List, 1861): (user ID, GIF bytes).
    pub async fn gif_icons(&self) -> Result<Vec<(u16, Vec<u8>)>, Error> {
        let t = self.icon_request(tx::ICON_GET_LIST, vec![]).await?;
        Ok(t.fields
            .iter()
            .filter(|f| f.id == field::ICON_LIST_ENTRY && f.data.len() >= 4)
            .filter_map(|f| {
                let uid = u16::from_be_bytes([f.data[0], f.data[1]]);
                let len = u16::from_be_bytes([f.data[2], f.data[3]]) as usize;
                let gif = f.data.get(4..4 + len)?;
                (!gif.is_empty()).then(|| (uid, gif.to_vec()))
            })
            .collect())
    }

    /// Sets our GIF icon (Set Icon, 1862); empty clears it.
    pub async fn set_gif_icon(&self, gif: &[u8]) -> Result<(), Error> {
        self.icon_request(tx::ICON_SET, vec![Field::new(field::GIF_ICON_DATA, gif)]).await.map(|_| ())
    }

    /// One user's GIF icon (Get Icon, 1863); None when they have none.
    pub async fn gif_icon(&self, user_id: u16) -> Result<Option<Vec<u8>>, Error> {
        let t = self.icon_request(tx::ICON_GET, vec![Field::u16(field::USER_ID, user_id)]).await?;
        Ok(t.bytes(field::GIF_ICON_DATA).filter(|b| b.starts_with(b"GIF8")).map(<[u8]>::to_vec))
    }
}

fn uploaded(t: &Transaction) -> Result<MediaRef, Error> {
    MediaRef::parse(t).ok_or_else(|| Error::Server { reason: None, text: "The server didn't keep the picture.".into() })
}
