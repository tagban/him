//! hotline-im for Swift: the modern macOS/iOS app's view of the protocol.
//!
//! A thin layer. `connect` signs on and hands back a `Session`; everything the
//! server says afterwards arrives through the `EventListener` the app passes in,
//! on a background thread. What to keep (roster, conversations) is the app's.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use hotline_im::client::UserInfo as CoreUserInfo;
use hotline_im::messaging::{new_guid as core_new_guid, reason, unhex};
use hotline_im::{icon, tracker, Client, ConnectOptions, Error as CoreError, Event as CoreEvent};

uniffi::setup_scaffolding!();

/// Every connection's tasks run here, whatever thread Swift calls from.
static RT: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("him-core")
        .enable_all()
        .build()
        .expect("tokio runtime")
});

async fn on_rt<F, T>(f: F) -> T
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    RT.spawn(f).await.expect("him-core task panicked")
}

// ---------- errors ----------

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum HimError {
    #[error("{message}")]
    Connect { message: String },
    /// Wrong screen name or password, or the account is disabled.
    #[error("{message}")]
    LoginFailed { message: String },
    #[error("{message}")]
    Security { message: String },
    /// The server said no; `reason` is its reason code, when it gave one.
    #[error("{message}")]
    Server { message: String, reason: Option<u16> },
    #[error("The server didn't answer in time.")]
    Timeout,
    #[error("You're not signed on.")]
    NotConnected,
}

impl From<CoreError> for HimError {
    fn from(e: CoreError) -> Self {
        let message = e.to_string();
        match e {
            CoreError::LoginFailed(_) => HimError::LoginFailed { message },
            CoreError::Security(_) | CoreError::Tls(_) => HimError::Security { message },
            CoreError::Server { reason, .. } => HimError::Server { message, reason },
            CoreError::Timeout => HimError::Timeout,
            CoreError::Closed => HimError::NotConnected,
            CoreError::Connect(_) | CoreError::Refused(_) => HimError::Connect { message },
        }
    }
}

// ---------- records ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Presence {
    Offline,
    Online,
    Away,
    Invisible,
    Busy,
}

impl From<hotline_im::Presence> for Presence {
    fn from(p: hotline_im::Presence) -> Self {
        use hotline_im::Presence as P;
        match p {
            P::Offline => Presence::Offline,
            P::Online => Presence::Online,
            P::Away => Presence::Away,
            P::Invisible => Presence::Invisible,
            P::Busy => Presence::Busy,
        }
    }
}

impl From<Presence> for hotline_im::Presence {
    fn from(p: Presence) -> Self {
        use hotline_im::Presence as P;
        match p {
            Presence::Offline => P::Offline,
            Presence::Online => P::Online,
            Presence::Away => P::Away,
            Presence::Invisible => P::Invisible,
            Presence::Busy => P::Busy,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RosterState {
    Removed,
    /// We asked; they haven't answered.
    PendingOut,
    /// They asked us.
    PendingIn,
    Accepted,
    Blocked,
}

impl From<hotline_im::RosterState> for RosterState {
    fn from(s: hotline_im::RosterState) -> Self {
        use hotline_im::RosterState as S;
        match s {
            S::Removed => RosterState::Removed,
            S::PendingOut => RosterState::PendingOut,
            S::PendingIn => RosterState::PendingIn,
            S::Accepted => RosterState::Accepted,
            S::Blocked => RosterState::Blocked,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Buddy {
    pub login: String,
    /// Our private alias for them.
    pub nickname: Option<String>,
    /// The name they go by.
    pub display_name: Option<String>,
    pub state: RosterState,
    pub presence: Presence,
    pub status_text: Option<String>,
    /// Hex; None means no Buddy Icon.
    pub icon_hash: Option<String>,
}

impl From<hotline_im::RosterEntry> for Buddy {
    fn from(e: hotline_im::RosterEntry) -> Self {
        Buddy {
            login: e.login,
            nickname: e.nickname,
            display_name: e.display_name,
            state: e.state.into(),
            presence: e.presence.into(),
            status_text: e.status_text,
            icon_hash: e.icon_hash,
        }
    }
}

/// A buddy's presence changed. Status and icon are complete (None = none);
/// a None display name means "unchanged".
#[derive(Clone, Debug, uniffi::Record)]
pub struct PresenceUpdate {
    pub login: String,
    pub presence: Presence,
    pub status_text: Option<String>,
    pub display_name: Option<String>,
    pub icon_hash: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct IncomingMessage {
    pub guid: String,
    pub from: String,
    pub body: String,
    /// Unix seconds from the server; 0 if it didn't say.
    pub timestamp: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AckKind {
    Delivered,
    Read,
}

/// Someone on a classic server (a chat room).
#[derive(Clone, Debug, uniffi::Record)]
pub struct ChatUser {
    pub id: u16,
    /// The classic icon number.
    pub icon: u16,
    pub name: String,
    pub away: bool,
    pub admin: bool,
}

impl From<hotline_im::ChatUser> for ChatUser {
    fn from(u: hotline_im::ChatUser) -> Self {
        ChatUser { id: u.id, icon: u.icon, away: u.is_away(), admin: u.is_admin(), name: u.name }
    }
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct Profile {
    /// The name buddies see.
    pub nickname: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub gender: u16,
    pub birth_year: u16,
    pub birth_month: u8,
    pub birth_day: u8,
    pub country: Option<String>,
    pub postcode: Option<String>,
    pub languages: Vec<String>,
}

impl From<hotline_im::Profile> for Profile {
    fn from(p: hotline_im::Profile) -> Self {
        Profile {
            nickname: p.nickname,
            first_name: p.first_name,
            last_name: p.last_name,
            email: p.email,
            gender: p.gender,
            birth_year: p.birth.0,
            birth_month: p.birth.1,
            birth_day: p.birth.2,
            country: p.country,
            postcode: p.postcode,
            languages: p.languages,
        }
    }
}

impl From<Profile> for hotline_im::Profile {
    fn from(p: Profile) -> Self {
        hotline_im::Profile {
            nickname: p.nickname,
            first_name: p.first_name,
            last_name: p.last_name,
            email: p.email,
            gender: p.gender,
            birth: (p.birth_year, p.birth_month, p.birth_day),
            country: p.country,
            postcode: p.postcode,
            languages: p.languages,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UserInfo {
    /// The server's name for them.
    pub name: Option<String>,
    pub profile: Option<Profile>,
    pub icon_hash: Option<String>,
}

impl From<CoreUserInfo> for UserInfo {
    fn from(i: CoreUserInfo) -> Self {
        UserInfo { name: i.name, profile: i.profile.map(Into::into), icon_hash: i.icon_hash }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct FoundUser {
    pub login: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct BuddyIcon {
    pub hash: String,
    pub data: Vec<u8>,
}

/// What a server takes for pictures in chat (inline media).
#[derive(Clone, Debug, uniffi::Record)]
pub struct MediaLimits {
    pub max_bytes: u32,
    pub max_dimension: u32,
    pub max_pixels: u32,
    pub max_frames: u32,
}

/// A picture the server holds, named in a chat line.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MediaRef {
    pub id: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u32,
}

impl From<hotline_im::MediaRef> for MediaRef {
    fn from(m: hotline_im::MediaRef) -> Self {
        MediaRef { id: m.id, mime: m.mime, width: m.width, height: m.height, bytes: m.bytes }
    }
}

impl From<MediaRef> for hotline_im::MediaRef {
    fn from(m: MediaRef) -> Self {
        hotline_im::MediaRef { id: m.id, mime: m.mime, width: m.width, height: m.height, bytes: m.bytes }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct MediaData {
    pub data: Vec<u8>,
    pub mime: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct GifIcon {
    pub user_id: u16,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ServerInfo {
    pub server_name: Option<String>,
    /// Instant messaging is on for this account.
    pub messaging: bool,
    /// "TLS", "HOPE (ChaCha20-Poly1305)", ...
    pub transport: String,
    pub encrypted: bool,
    pub warnings: Vec<String>,
    pub max_message_bytes: u32,
    /// None: the server has no Buddy Icons.
    pub max_icon_bytes: Option<u32>,
    pub max_icon_dimension: u32,
    /// Text is UTF-8 (emoji go through as they are).
    pub utf8: bool,
    /// Pictures in chat, when the server takes them.
    pub media: Option<MediaLimits>,
    /// Our user ID (chat rooms).
    pub user_id: u16,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ListedServer {
    pub host: String,
    pub port: u16,
    pub users: u16,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct IconInfo {
    /// "gif", "png" or "jpg"
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Security {
    /// The strongest the server offers.
    Auto,
    Tls,
    HopeEncrypted,
    Plain,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SignOn {
    pub host: String,
    pub port: u16,
    /// Empty for a guest (chat rooms).
    pub login: String,
    pub password: String,
    pub nickname: String,
    /// The classic icon number (chat rooms).
    pub icon: u16,
    pub security: Security,
    /// A chat room: a classic Hotline session with no messaging.
    pub classic: bool,
    /// Ask a chat room's server for pictures in chat.
    pub media: bool,
}

// ---------- events ----------

#[derive(Clone, Debug, uniffi::Enum)]
pub enum HimEvent {
    /// A roster entry, whole (a Removed state means it's gone).
    Buddy { buddy: Buddy },
    FriendRequest { login: String, note: Option<String> },
    Presence { update: PresenceUpdate },
    Message { message: IncomingMessage },
    Ack { guid: String, login: String, kind: AckKind },
    Typing { login: String, typing: bool },
    /// The server's agreement; accept it with `agree`.
    Agreement { text: String },
    /// Public chat; `chat_id` is set for a private chat room.
    Chat { chat_id: Option<u32>, text: String, media: Option<MediaRef> },
    UserChanged { user: ChatUser },
    UserLeft { id: u16 },
    /// A classic private message.
    PrivateMessage { from_id: u16, from_name: String, text: String, media: Option<MediaRef> },
    /// Someone's GIF icon changed; fetch it with `gif_icon`.
    GifIconChanged { user_id: u16 },
    ServerMessage { text: String },
    /// Another of our sessions changed our Buddy Icon; None = cleared.
    OwnIconChanged { hash: Option<String> },
    Disconnected { reason: String },
}

impl From<CoreEvent> for HimEvent {
    fn from(e: CoreEvent) -> Self {
        match e {
            CoreEvent::RosterEntry { entry } => HimEvent::Buddy { buddy: entry.into() },
            CoreEvent::FriendRequest { login, note } => HimEvent::FriendRequest { login, note },
            CoreEvent::Presence { update: u } => HimEvent::Presence {
                update: PresenceUpdate {
                    login: u.login,
                    presence: u.presence.into(),
                    status_text: u.status_text,
                    display_name: u.display_name,
                    icon_hash: u.icon_hash,
                },
            },
            CoreEvent::Message { message: m } => HimEvent::Message {
                message: IncomingMessage { guid: m.guid, from: m.from, body: m.body, timestamp: m.timestamp },
            },
            CoreEvent::Ack { guid, login, ack } => HimEvent::Ack {
                guid,
                login,
                kind: match ack {
                    hotline_im::AckKind::Delivered => AckKind::Delivered,
                    hotline_im::AckKind::Read => AckKind::Read,
                },
            },
            CoreEvent::Typing { login, typing } => HimEvent::Typing { login, typing },
            CoreEvent::Agreement { text } => HimEvent::Agreement { text },
            CoreEvent::ChatMessage { chat_id, text, media } => HimEvent::Chat { chat_id, text, media: media.map(Into::into) },
            CoreEvent::UserChanged { user } => HimEvent::UserChanged { user: user.into() },
            CoreEvent::UserLeft { id } => HimEvent::UserLeft { id },
            CoreEvent::PrivateMessage { from_id, from_name, text, media } => {
                HimEvent::PrivateMessage { from_id, from_name, text, media: media.map(Into::into) }
            }
            CoreEvent::GifIconChanged { user_id } => HimEvent::GifIconChanged { user_id },
            CoreEvent::ServerMessage { text } => HimEvent::ServerMessage { text },
            CoreEvent::OwnIconChanged { hash } => HimEvent::OwnIconChanged { hash },
            CoreEvent::Disconnected { reason } => HimEvent::Disconnected { reason },
        }
    }
}

/// The app's side: called on a background thread, in order, for everything the server says.
#[uniffi::export(with_foreign)]
pub trait EventListener: Send + Sync {
    fn on_event(&self, event: HimEvent);
}

// ---------- signing on ----------

/// Signs on. Events start flowing to `listener` straight away (the roster comes with `get_roster`).
#[uniffi::export]
pub async fn connect(sign_on: SignOn, listener: Arc<dyn EventListener>) -> Result<Arc<Session>, HimError> {
    on_rt(async move {
        let opts = ConnectOptions {
            host: sign_on.host,
            port: sign_on.port,
            login: sign_on.login,
            password: sign_on.password,
            nickname: sign_on.nickname,
            icon: sign_on.icon,
            security: match sign_on.security {
                Security::Auto => hotline_im::Security::Auto,
                Security::Tls => hotline_im::Security::Tls,
                Security::HopeEncrypted => hotline_im::Security::HopeEncrypted,
                Security::Plain => hotline_im::Security::Plain,
            },
            classic: sign_on.classic,
            media: sign_on.media,
        };
        let s = hotline_im::connect(&opts).await?;
        let mut events = s.events;
        tokio::spawn(async move {
            while let Some(e) = events.recv().await {
                listener.on_event(e.into());
            }
        });
        Ok(Arc::new(Session { client: s.client }))
    })
    .await
}

#[derive(uniffi::Object)]
pub struct Session {
    client: Client,
}

fn ok<T>(r: Result<T, CoreError>) -> Result<(), HimError> {
    r.map(|_| ()).map_err(Into::into)
}

#[uniffi::export]
impl Session {
    pub fn info(&self) -> ServerInfo {
        let i = &self.client.info;
        ServerInfo {
            server_name: i.server_name.clone(),
            messaging: i.messaging,
            transport: i.transport.clone(),
            encrypted: i.encrypted,
            warnings: i.warnings.clone(),
            max_message_bytes: i.limits.max_message_bytes,
            max_icon_bytes: i.limits.max_icon_bytes,
            max_icon_dimension: i.limits.max_icon_dimension,
            utf8: i.utf8,
            media: i.media.as_ref().map(|m| MediaLimits {
                max_bytes: m.max_bytes,
                max_dimension: m.max_dimension,
                max_pixels: m.max_pixels,
                max_frames: m.max_frames,
            }),
            user_id: i.user_id,
        }
    }

    pub fn is_open(&self) -> bool {
        self.client.is_open()
    }

    pub fn disconnect(&self) {
        self.client.disconnect();
    }

    // ----- instant messaging -----

    /// `discoverable`: None leaves it as it is.
    pub async fn set_presence(&self, presence: Presence, status: String, discoverable: Option<bool>) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { ok(c.set_presence(presence.into(), &status, discoverable).await) }).await
    }

    /// The whole roster; also the signal to send the offline backlog.
    pub async fn get_roster(&self) -> Result<Vec<Buddy>, HimError> {
        let c = self.client.clone();
        on_rt(async move { Ok(c.get_roster().await?.into_iter().map(Into::into).collect()) }).await
    }

    pub async fn add_friend(&self, login: String, note: String) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { ok(c.add_friend(&login, &note).await) }).await
    }

    pub async fn respond(&self, login: String, accept: bool) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { ok(c.respond(&login, accept).await) }).await
    }

    pub async fn remove_friend(&self, login: String) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { ok(c.remove_friend(&login).await) }).await
    }

    pub async fn block(&self, login: String) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { ok(c.block(&login).await) }).await
    }

    pub async fn unblock(&self, login: String) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { ok(c.unblock(&login).await) }).await
    }

    /// Our private name for them; empty clears it.
    pub async fn set_alias(&self, login: String, alias: String) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { ok(c.set_alias(&login, &alias).await) }).await
    }

    /// Sends an IM under `guid` (from `new_guid`, so the app can show it first).
    /// True when the server queued it for someone offline. A timeout is retried
    /// once with the same GUID, which the server de-duplicates.
    pub async fn send_im(&self, to: String, guid: String, body: String) -> Result<bool, HimError> {
        let c = self.client.clone();
        on_rt(async move {
            let g: [u8; 16] = unhex(&guid)
                .and_then(|v| v.try_into().ok())
                .ok_or(HimError::Server { message: "Bad message ID.".into(), reason: None })?;
            let mut r = c.send_im(&to, &g, &body).await;
            if matches!(r, Err(CoreError::Timeout)) {
                r = c.send_im(&to, &g, &body).await;
            }
            Ok(r? == reason::OFFLINE_QUEUED)
        })
        .await
    }

    pub fn ack(&self, guid: String, from: String, kind: AckKind) {
        let k = match kind {
            AckKind::Delivered => hotline_im::AckKind::Delivered,
            AckKind::Read => hotline_im::AckKind::Read,
        };
        let c = self.client.clone();
        RT.spawn(async move { c.ack_nowait(&guid, &from, k) });
    }

    pub fn typing(&self, to: String, typing: bool) {
        let c = self.client.clone();
        RT.spawn(async move { c.typing(&to, typing) });
    }

    pub async fn find_user(&self, login: String) -> Result<FoundUser, HimError> {
        let c = self.client.clone();
        on_rt(async move {
            let (login, name) = c.find_user(&login).await?;
            Ok(FoundUser { login, name })
        })
        .await
    }

    pub async fn search(&self, query: String) -> Result<Vec<FoundUser>, HimError> {
        let c = self.client.clone();
        on_rt(async move {
            Ok(c.search(&query).await?.into_iter().map(|(login, name)| FoundUser { login, name }).collect())
        })
        .await
    }

    pub async fn get_info(&self, login: String) -> Result<UserInfo, HimError> {
        let c = self.client.clone();
        on_rt(async move { Ok(c.get_info(&login).await?.into()) }).await
    }

    /// Replaces the whole profile.
    pub async fn set_info(&self, profile: Profile) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { ok(c.set_info(&profile.into()).await) }).await
    }

    pub fn has_buddy_icons(&self) -> bool {
        self.client.has_buddy_icons()
    }

    /// Uploads a Buddy Icon (empty clears it); the server's hash for what it stored.
    pub async fn set_buddy_icon(&self, picture: Vec<u8>) -> Result<Option<String>, HimError> {
        let c = self.client.clone();
        on_rt(async move { Ok(c.set_buddy_icon(&picture).await?) }).await
    }

    pub async fn get_buddy_icon(&self, login: String) -> Result<Option<BuddyIcon>, HimError> {
        let c = self.client.clone();
        on_rt(async move { Ok(c.get_buddy_icon(&login).await?.map(|(hash, data)| BuddyIcon { hash, data })) }).await
    }

    // ----- classic Hotline (chat rooms) -----

    pub async fn agree(&self, nickname: String, icon: u16) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { ok(c.agree(&nickname, icon).await) }).await
    }

    /// For servers that never answer the agreement.
    pub fn agree_nowait(&self, nickname: String, icon: u16) {
        let c = self.client.clone();
        RT.spawn(async move { c.agree_nowait(&nickname, icon) });
    }

    pub fn send_chat(&self, text: String, emote: bool) {
        let c = self.client.clone();
        RT.spawn(async move { c.send_chat(&text, emote) });
    }

    pub async fn get_users(&self) -> Result<Vec<ChatUser>, HimError> {
        let c = self.client.clone();
        on_rt(async move { Ok(c.get_users().await?.into_iter().map(Into::into).collect()) }).await
    }

    /// A classic private message to someone in a room, optionally with a picture.
    pub async fn send_private(&self, user_id: u16, text: String, media: Option<MediaRef>) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { Ok(c.send_private(user_id, &text, media.map(Into::into).as_ref()).await?) }).await
    }

    // ----- pictures in chat (inline media) -----

    /// Uploads a picture; put the handle in a chat line with `send_chat_media`.
    pub async fn upload_media(&self, data: Vec<u8>) -> Result<MediaRef, HimError> {
        let c = self.client.clone();
        on_rt(async move { Ok(c.upload_media(&data).await?.into()) }).await
    }

    pub async fn download_media(&self, id: String) -> Result<MediaData, HimError> {
        let c = self.client.clone();
        on_rt(async move {
            let (data, mime) = c.download_media(&id).await?;
            Ok(MediaData { data, mime })
        })
        .await
    }

    /// Chat with a picture; `text` is what clients without pictures see.
    pub fn send_chat_media(&self, text: String, media: MediaRef) {
        let c = self.client.clone();
        RT.spawn(async move { c.send_chat_media(&text, &media.into()) });
    }

    // ----- GIF icons -----

    pub async fn gif_icons(&self) -> Result<Vec<GifIcon>, HimError> {
        let c = self.client.clone();
        on_rt(async move {
            Ok(c.gif_icons().await?.into_iter().map(|(user_id, data)| GifIcon { user_id, data }).collect())
        })
        .await
    }

    /// Sets our GIF icon; empty clears it.
    pub async fn set_gif_icon(&self, gif: Vec<u8>) -> Result<(), HimError> {
        let c = self.client.clone();
        on_rt(async move { Ok(c.set_gif_icon(&gif).await?) }).await
    }

    pub async fn gif_icon(&self, user_id: u16) -> Result<Option<Vec<u8>>, HimError> {
        let c = self.client.clone();
        on_rt(async move { Ok(c.gif_icon(user_id).await?) }).await
    }
}

// ---------- no connection needed ----------

/// Emoji as the text faces classic clients show (😀 → :D).
#[uniffi::export]
pub fn emoji_to_faces(text: String) -> String {
    hotline_im::emoticons::to_faces(&text)
}

/// Text faces as emoji, for showing (:) → 🙂).
#[uniffi::export]
pub fn faces_to_emoji(text: String) -> String {
    hotline_im::emoticons::to_emoji(&text)
}

/// A fresh message ID, as hex.
#[uniffi::export]
pub fn new_guid() -> String {
    hotline_im::messaging::hex(&core_new_guid())
}

/// The public trackers HIM asks by default.
#[uniffi::export]
pub fn default_trackers() -> Vec<String> {
    tracker::DEFAULT_TRACKERS.iter().map(|s| s.to_string()).collect()
}

/// Every server the trackers list, merged, busiest first.
#[uniffi::export]
pub async fn list_servers(trackers: Vec<String>, timeout_secs: u32) -> Vec<ListedServer> {
    on_rt(async move {
        let (mut servers, _failed) = tracker::query_all(&trackers, Duration::from_secs(timeout_secs as u64)).await;
        servers.sort_by(|a, b| b.users.cmp(&a.users));
        servers
            .into_iter()
            .map(|s| ListedServer { host: s.host, port: s.port, users: s.users, name: s.name, description: s.description })
            .collect()
    })
    .await
}

/// The hash the server uses for a Buddy Icon (hex).
#[uniffi::export]
pub fn icon_hash(data: Vec<u8>) -> String {
    hotline_im::messaging::hex(&icon::hash(&data))
}

/// Format, size and frame count of a picture; None if it isn't one we can use.
#[uniffi::export]
pub fn inspect_icon(data: Vec<u8>) -> Option<IconInfo> {
    let h = icon::inspect(&data)?;
    Some(IconInfo { format: h.format.ext().to_string(), width: h.width, height: h.height, frames: h.frames as u32 })
}
