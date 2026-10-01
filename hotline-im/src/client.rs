//! A messenger session: connect, sign on (HOPE or legacy), then requests and
//! notifications multiplexed by task ID (guide §4, §5.4, §7).

use crate::frame::{FrameReader, FrameWriter, Sealer, DIR_CLIENT_TO_SERVER, DIR_SERVER_TO_CLIENT};
use crate::hope::{self, MacAlg};
use crate::icon;
use crate::info::{self, Descriptor};
use crate::media::{MediaLimits, MediaRef};
use crate::messaging::*;
use crate::text::TextMode;
use crate::wire::{cap, decode_name_list, encode_name_list, field, invert, tx, Field, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, watch};

/// Our Version (160). 1997 was AIM's year; not taken in the Known Identifiers registry.
pub const CLIENT_VERSION: u16 = 1997;
pub const APP_ID: &[u8; 4] = b"HIMc";
pub const APP_STRING: &str = concat!("HIM ", env!("CARGO_PKG_VERSION"));

/// Messaging + messenger session + UTF-8. No voice or files yet, so we don't claim them.
pub const OUR_CAPS: u16 = cap::MESSAGING | cap::MESSENGER_SESSION | cap::TEXT_ENCODING;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Security {
    /// Strongest the server offers: TLS, else HOPE with ChaCha20-Poly1305, else
    /// HOPE authentication alone, else the legacy login.
    #[default]
    Auto,
    Tls,
    HopeEncrypted,
    /// Legacy login on a plaintext socket (the password is only inverted).
    Plain,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOptions {
    pub host: String,
    pub port: u16,
    pub login: String,
    pub password: String,
    pub nickname: String,
    pub icon: u16,
    pub security: Security,
    /// A classic Hotline session (chat rooms): no messaging bits, a 1.9 version.
    #[serde(default)]
    pub classic: bool,
    /// Ask a classic session's server for pictures in chat (inline media).
    #[serde(default)]
    pub media: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub max_message_bytes: u32,
    pub max_roster_size: u32,
    pub max_offline_queue: u32,
    /// None: the server has no buddy icons (its absence is the signal).
    pub max_icon_bytes: Option<u32>,
    pub max_icon_dimension: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginInfo {
    pub server_name: Option<String>,
    pub server_version: Option<u64>,
    pub user_id: u16,
    pub caps: u16,
    pub messaging: bool,
    pub utf8: bool,
    pub limits: Limits,
    /// "TLS", "HOPE (ChaCha20-Poly1305)", "HOPE (HMAC-SHA256 sign-in only)", "Plaintext"
    pub transport: String,
    pub encrypted: bool,
    /// Raised during sign-on (address mismatch, downgrades); show once there is a window.
    pub warnings: Vec<String>,
    /// Pictures in chat, when the server confirmed inline media.
    pub media: Option<MediaLimits>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    RosterEntry {
        entry: RosterEntry,
    },
    FriendRequest {
        login: String,
        note: Option<String>,
    },
    Presence {
        update: PresenceUpdate,
    },
    Message {
        message: IncomingMessage,
    },
    Ack {
        guid: String,
        login: String,
        ack: AckKind,
    },
    Typing {
        login: String,
        typing: bool,
    },
    Agreement {
        text: String,
    },
    /// Public chat (106); `chat_id` is set for a private chat room.
    ChatMessage {
        chat_id: Option<u32>,
        text: String,
        /// A picture attached (inline media).
        media: Option<MediaRef>,
    },
    /// Someone joined, or changed name or flags (301).
    UserChanged {
        user: ChatUser,
    },
    /// Someone left the server (302).
    UserLeft {
        id: u16,
    },
    /// A classic private message (104 with a sender).
    PrivateMessage {
        from_id: u16,
        from_name: String,
        text: String,
        media: Option<MediaRef>,
    },
    /// Someone set or cleared their GIF icon (1864); fetch it with `gif_icon`.
    GifIconChanged {
        user_id: u16,
    },
    ServerMessage {
        text: String,
    },
    /// Another of our own sessions changed our Buddy Icon (827 as a notification);
    /// None when it was cleared.
    OwnIconChanged {
        hash: Option<String>,
    },
    Disconnected {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub enum Error {
    Connect(String),
    Refused(String),
    Security(String),
    LoginFailed(String),
    /// A reply with a non-zero error code.
    Server {
        reason: Option<u16>,
        text: String,
    },
    Timeout,
    Closed,
    /// The TLS handshake or certificate check failed.
    Tls(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Connect(s) => write!(f, "Couldn't connect: {s}"),
            Error::Refused(s) => write!(f, "The server refused the connection: {s}"),
            Error::Security(s) => write!(f, "{s}"),
            Error::LoginFailed(s) => write!(f, "{s}"),
            Error::Server { text, .. } => write!(f, "{text}"),
            Error::Timeout => write!(f, "The server didn't answer in time."),
            Error::Closed => write!(f, "You're not signed on."),
            Error::Tls(s) => write!(f, "Couldn't make a secure (TLS) connection: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Connect(e.to_string())
    }
}

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
type BoxStream = Box<dyn Stream>;

type Pending = Arc<Mutex<HashMap<u32, oneshot::Sender<Transaction>>>>;

/// A signed-on session. Cheap to clone; every clone talks on the same connection.
#[derive(Clone)]
pub struct Client {
    out: mpsc::UnboundedSender<Transaction>,
    pending: Pending,
    next_id: Arc<AtomicU32>,
    stop: Arc<watch::Sender<bool>>,
    pub text: TextMode,
    pub info: Arc<LoginInfo>,
}

/// What a successful sign-on hands back.
pub struct Session {
    pub client: Client,
    pub events: mpsc::UnboundedReceiver<Event>,
}

enum Plan {
    Tls { port: u16 },
    Hope { encrypt: bool },
    Plain,
}

pub async fn connect(opts: &ConnectOptions) -> Result<Session, Error> {
    let desc = info::probe(&opts.host, opts.port.saturating_sub(1)).await;
    let data_port = desc
        .as_ref()
        .map(|d| d.data_port)
        .filter(|p| *p != 0)
        .unwrap_or(opts.port);
    // A guest has no password to protect: HOPE would add nothing, and some servers refuse it.
    // Public chat gains nothing from encryption, so a guest connects the way every classic
    // client does, unless the server insists on something else.
    let plan = if opts.classic && opts.password.is_empty() && opts.security == Security::Auto {
        let d = desc.as_ref();
        let must_tls =
            d.is_some_and(|d| d.transport.tls.required || !d.transport.plaintext.accepted);
        match d
            .and_then(|d| d.tls_port.filter(|_| d.transport.tls.supported))
            .filter(|_| must_tls)
        {
            Some(port) => Plan::Tls { port },
            None => Plan::Plain,
        }
    } else {
        choose_plan(opts.security, desc.as_ref())?
    };
    let mut warnings = Vec::new();

    match plan {
        Plan::Tls { port } => {
            // Verify against the name the server gives for itself, not the address we dialed.
            let name = desc
                .as_ref()
                .and_then(|d| d.server.as_ref())
                .and_then(|s| s.hostname.clone())
                .filter(|h| !h.trim().is_empty())
                .unwrap_or_else(|| opts.host.clone());
            let tried = match open(&opts.host, port, Some(&name)).await {
                Ok((stream, peer)) => Ok(sign_on(
                    stream,
                    peer,
                    port,
                    opts,
                    true,
                    false,
                    "TLS",
                    warnings.clone(),
                )
                .await),
                Err(e) => Err(e),
            };
            match tried {
                Ok(result) => result,
                // Automatic only asked for the best on offer: a TLS listener whose certificate
                // can't be verified falls back to what a classic client would get, with a warning.
                Err(Error::Tls(why)) if opts.security == Security::Auto => {
                    warnings.push(format!("This server offers TLS, but its certificate couldn't be verified ({why}), so HIM connected without TLS."));
                    if opts.classic && opts.password.is_empty() {
                        let (stream, peer) = open(&opts.host, data_port, None).await?;
                        sign_on_plain(stream, peer, opts, warnings).await
                    } else {
                        let (stream, peer) = open(&opts.host, data_port, None).await?;
                        match sign_on(
                            stream,
                            peer,
                            data_port,
                            opts,
                            false,
                            true,
                            "",
                            warnings.clone(),
                        )
                        .await
                        {
                            Err(Error::Security(s)) if s == HOPE_UNSUPPORTED => {
                                warnings.push("This server doesn't support secure sign-on either, so your password was sent the old Hotline way (obfuscated, not encrypted).".into());
                                let (stream, peer) = open(&opts.host, data_port, None).await?;
                                sign_on_plain(stream, peer, opts, warnings).await
                            }
                            other => other,
                        }
                    }
                }
                Err(e) => Err(e),
            }
        }
        Plan::Hope { encrypt } => {
            let (stream, peer) = open(&opts.host, data_port, None).await?;
            match sign_on(stream, peer, data_port, opts, false, encrypt, "", warnings.clone()).await {
                Err(Error::Security(s)) if s == HOPE_UNSUPPORTED && opts.security == Security::Auto => {
                    // A server without HOPE: fall back to the legacy login, and say so.
                    warnings.push("This server doesn't support secure sign-on, so your password was sent the old Hotline way (obfuscated, not encrypted).".into());
                    let (stream, peer) = open(&opts.host, data_port, None).await?;
                    sign_on_plain(stream, peer, opts, warnings).await
                }
                Err(Error::Security(s)) if s == HOPE_UNSUPPORTED => Err(Error::Security(
                    "This server doesn't support secure sign-on (HOPE); change Security in Setup to connect anyway.".into(),
                )),
                other => other,
            }
        }
        Plan::Plain => {
            let (stream, peer) = open(&opts.host, data_port, None).await?;
            sign_on_plain(stream, peer, opts, warnings).await
        }
    }
}

const HOPE_UNSUPPORTED: &str = "HOPE unsupported";

fn choose_plan(sec: Security, desc: Option<&Descriptor>) -> Result<Plan, Error> {
    let tls_port = desc.and_then(|d| d.tls_port.filter(|_| d.transport.tls.supported));
    match sec {
        Security::Tls => tls_port.map(|port| Plan::Tls { port }).ok_or_else(|| {
            Error::Security("This server doesn't offer TLS, and your settings require it.".into())
        }),
        Security::HopeEncrypted => {
            if desc.is_some_and(|d| d.transport.hope.known_unsupported()) {
                return Err(Error::Security(
                    "This server doesn't offer HOPE encryption, and your settings require it."
                        .into(),
                ));
            }
            Ok(Plan::Hope { encrypt: true })
        }
        Security::Plain => {
            if desc.is_some_and(|d| {
                d.transport.hope.required
                    || d.transport.tls.required
                    || !d.transport.plaintext.accepted
            }) {
                return Err(Error::Security(
                    "This server requires an encrypted connection; change Security in Setup."
                        .into(),
                ));
            }
            Ok(Plan::Plain)
        }
        // HOPE first: it encrypts on the normal port with no certificate to trust. A TLS
        // attempt that fails its certificate check hangs up mid-handshake, which hardened
        // servers (Janus) can treat as a hostile probe and ban the address for. So TLS only
        // when the server has no HOPE, or requires TLS.
        Security::Auto => {
            let tls_required = desc.is_some_and(|d| d.transport.tls.required);
            let hope_missing = desc.is_some_and(|d| d.transport.hope.known_unsupported());
            match tls_port {
                Some(port) if tls_required || hope_missing => Ok(Plan::Tls { port }),
                _ => Ok(Plan::Hope { encrypt: true }),
            }
        }
    }
}

/// `tls`: the name to verify the certificate against, when wrapping in TLS.
async fn open(host: &str, port: u16, tls: Option<&str>) -> Result<(BoxStream, SocketAddr), Error> {
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| Error::Connect(format!("{host}:{port} didn't answer")))??;
    tcp.set_nodelay(true).ok();
    let _ = socket2::SockRef::from(&tcp).set_keepalive(true);
    let peer = tcp.peer_addr()?;
    let mut stream: BoxStream = if let Some(name) = tls {
        let tls = crate::tls::connect(name, tcp)
            .await
            .map_err(|e| Error::Tls(e.to_string()))?;
        Box::new(tls)
    } else {
        Box::new(tcp)
    };
    // TRTP handshake (guide §4.3): the sub-protocol must be HOTL.
    stream.write_all(b"TRTPHOTL\x00\x01\x00\x00").await?;
    let mut reply = [0u8; 8];
    tokio::time::timeout(CONNECT_TIMEOUT, stream.read_exact(&mut reply))
        .await
        .map_err(|_| Error::Connect("no handshake reply".into()))??;
    if &reply[..4] != b"TRTP" {
        return Err(Error::Refused("not a Hotline server".into()));
    }
    let code = u32::from_be_bytes(reply[4..].try_into().unwrap());
    if code != 0 {
        return Err(Error::Refused(format!("handshake error {code}")));
    }
    Ok((stream, peer))
}

struct Io {
    r: FrameReader<tokio::io::ReadHalf<BoxStream>>,
    w: FrameWriter<tokio::io::WriteHalf<BoxStream>>,
    next_id: u32,
    /// Notifications that arrived while we waited for a sign-on reply.
    early: Vec<Transaction>,
}

impl Io {
    fn new(s: BoxStream) -> Self {
        let (r, w) = tokio::io::split(s);
        Io {
            r: FrameReader::new(r),
            w: FrameWriter::new(w),
            next_id: 1,
            early: Vec::new(),
        }
    }

    async fn call(&mut self, mut t: Transaction) -> Result<Transaction, Error> {
        t.id = self.next_id;
        self.next_id += 1;
        self.w.write(&t).await?;
        self.wait(t.id).await
    }

    async fn wait(&mut self, id: u32) -> Result<Transaction, Error> {
        loop {
            let r = tokio::time::timeout(REQUEST_TIMEOUT, self.r.read())
                .await
                .map_err(|_| Error::Timeout)??;
            if r.is_reply && r.id == id {
                return Ok(r);
            }
            if !r.is_reply {
                self.early.push(r);
            }
        }
    }
}

fn login_fields(opts: &ConnectOptions, text: TextMode) -> Vec<Field> {
    let (version, caps) = if opts.classic {
        let media = if opts.media { cap::INLINE_MEDIA } else { 0 };
        (CLASSIC_VERSION, cap::TEXT_ENCODING | media)
    } else {
        (CLIENT_VERSION, OUR_CAPS)
    };
    vec![
        Field::new(field::USER_NAME, text.encode(&opts.nickname)),
        Field::int(field::USER_ICON_ID, opts.icon as u32),
        Field::int(field::VERSION, version as u32),
        Field::u16(field::CAPABILITIES, caps),
    ]
}

/// What a classic session claims to be: a 1.9 client, which every server knows
/// and which is still offered the agreement.
pub const CLASSIC_VERSION: u16 = 190;

#[allow(clippy::too_many_arguments)]
async fn sign_on(
    stream: BoxStream,
    peer: SocketAddr,
    dialed_port: u16,
    opts: &ConnectOptions,
    in_tls: bool,
    encrypt: bool,
    label: &str,
    mut warnings: Vec<String>,
) -> Result<Session, Error> {
    let mut io = Io::new(stream);
    // Our own strings go out as UTF-8 before negotiation: logins should be ASCII.
    let pre = TextMode::Utf8;

    // Step 1: identification.
    let mut ident = vec![
        Field::new(field::USER_LOGIN, [0u8]),
        Field::new(field::USER_PASSWORD, [0u8]),
        Field::new(
            field::HOPE_MAC_ALGORITHM,
            encode_name_list(hope::OFFERED_MACS),
        ),
        Field::new(field::HOPE_APP_ID, APP_ID.to_vec()),
        Field::new(field::HOPE_APP_STRING, APP_STRING),
        Field::new(field::HOPE_SESSION_KEY, Vec::new()),
    ];
    if encrypt && !in_tls {
        ident.push(Field::new(
            field::HOPE_CLIENT_CIPHER,
            encode_name_list(&[hope::AEAD_CIPHER]),
        ));
    }
    let r = io.call(Transaction::request(tx::LOGIN, ident)).await?;
    let session_key = match r.bytes(field::HOPE_SESSION_KEY) {
        Some(k) if k.len() == 64 && r.error == 0 => k.to_vec(),
        _ => return Err(Error::Security(HOPE_UNSUPPORTED.into())),
    };
    let alg = r
        .bytes(field::HOPE_MAC_ALGORITHM)
        .and_then(|b| decode_name_list(b).into_iter().next())
        .and_then(|n| MacAlg::parse(&n))
        .unwrap_or(MacAlg::Inverse);
    let mac_login = r.bytes(field::USER_LOGIN).is_some_and(|b| !b.is_empty());

    // The key opens with the address the server saw us dial.
    if let IpAddr::V4(ip) = peer.ip() {
        let key_ip = &session_key[..4];
        let key_port = u16::from_be_bytes([session_key[4], session_key[5]]);
        if key_ip != ip.octets() || key_port != dialed_port {
            warnings.push(format!(
                "The server's session key names {}.{}.{}.{}:{key_port}, but we connected to {ip}:{dialed_port}. That's normal behind NAT, but it can also mean someone is in the middle.",
                key_ip[0], key_ip[1], key_ip[2], key_ip[3]
            ));
        }
    }

    let server_cipher = r
        .bytes(field::HOPE_SERVER_CIPHER)
        .map(decode_name_list)
        .unwrap_or_default();
    let client_cipher = r
        .bytes(field::HOPE_CLIENT_CIPHER)
        .map(decode_name_list)
        .unwrap_or_default();
    let aead = !in_tls
        && alg != MacAlg::Inverse
        && server_cipher.first().map(|c| hope::normalize_cipher(c))
            == Some(hope::AEAD_CIPHER.into())
        && client_cipher.first().map(|c| hope::normalize_cipher(c))
            == Some(hope::AEAD_CIPHER.into());
    if encrypt && !in_tls && !aead {
        if opts.security == Security::HopeEncrypted {
            return Err(Error::Security(
                "The server didn't agree to encrypt the connection, and your settings require it."
                    .into(),
            ));
        }
        warnings.push(format!(
            "Your password was protected with {}, but this server doesn't encrypt messages.",
            alg.name()
        ));
    }

    // Step 3: authenticated login.
    let pw = opts.password.as_bytes();
    let login_bytes = pre.encode(&opts.login);
    let mut fields = vec![
        Field::new(
            field::USER_LOGIN,
            if mac_login {
                alg.mac(&login_bytes, &session_key)
            } else {
                invert(&login_bytes)
            },
        ),
        Field::new(field::USER_PASSWORD, alg.mac(pw, &session_key)),
    ];
    fields.extend(login_fields(opts, pre));
    if aead {
        fields.push(Field::new(
            field::HOPE_SERVER_CIPHER,
            encode_name_list(&[hope::AEAD_CIPHER]),
        ));
    }
    let mut t = Transaction::request(tx::LOGIN, fields);
    t.id = io.next_id;
    io.next_id += 1;
    io.w.write(&t).await?;
    if aead {
        let keys = hope::aead_keys(alg, pw, &session_key).expect("AEAD needs a real MAC");
        io.w.enable_aead(Sealer::new(&keys.decode, DIR_CLIENT_TO_SERVER));
        io.r.enable_aead(Sealer::new(&keys.encode, DIR_SERVER_TO_CLIENT));
    }
    let reply = if aead {
        let (first, plain) = tokio::time::timeout(REQUEST_TIMEOUT, io.r.read_first_sealed())
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::LoginFailed("Incorrect login.".into()))?;
        if plain {
            // Refused before encryption started.
            return finish(io, first, String::new(), false, warnings);
        }
        if first.is_reply && first.id == t.id {
            first
        } else {
            if !first.is_reply {
                io.early.push(first);
            }
            io.wait(t.id).await?
        }
    } else {
        io.wait(t.id).await?
    };

    let transport = if in_tls {
        label.to_string()
    } else if aead {
        "HOPE (ChaCha20-Poly1305)".to_string()
    } else if alg == MacAlg::Inverse {
        "Plaintext".to_string()
    } else {
        format!("HOPE ({} sign-in only)", alg.name())
    };
    let encrypted = in_tls || aead;
    finish(io, reply, transport, encrypted, warnings)
}

async fn sign_on_plain(
    stream: BoxStream,
    _peer: SocketAddr,
    opts: &ConnectOptions,
    warnings: Vec<String>,
) -> Result<Session, Error> {
    let mut io = Io::new(stream);
    let pre = TextMode::Utf8;
    let mut fields = vec![
        Field::new(field::USER_LOGIN, invert(&pre.encode(&opts.login))),
        Field::new(field::USER_PASSWORD, invert(opts.password.as_bytes())),
    ];
    fields.extend(login_fields(opts, pre));
    let reply = io.call(Transaction::request(tx::LOGIN, fields)).await?;
    finish(io, reply, "Plaintext".into(), false, warnings)
}

fn finish(
    io: Io,
    reply: Transaction,
    transport: String,
    encrypted: bool,
    warnings: Vec<String>,
) -> Result<Session, Error> {
    if reply.error != 0 {
        let text = reply
            .bytes(field::ERROR)
            .map(|b| TextMode::MacRoman.decode(b))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Incorrect login.".into());
        return Err(Error::LoginFailed(text));
    }
    let caps = reply.uint(field::CAPABILITIES).unwrap_or(0) as u16;
    let utf8 = caps & cap::TEXT_ENCODING != 0;
    let text = if utf8 {
        TextMode::Utf8
    } else {
        TextMode::MacRoman
    };
    let limit = |id, default| {
        reply
            .uint(id)
            .map(|v| v as u32)
            .filter(|v| *v != 0)
            .unwrap_or(default)
    };
    let info = LoginInfo {
        server_name: reply.bytes(field::SERVER_NAME).map(|b| text.decode(b)),
        server_version: reply.uint(field::VERSION),
        user_id: reply.uint(field::USER_ID).unwrap_or(0) as u16,
        caps,
        messaging: caps & cap::MESSAGING != 0,
        utf8,
        limits: Limits {
            max_message_bytes: limit(field::MAX_MESSAGE_BYTES, 4096),
            max_roster_size: limit(field::MAX_ROSTER_SIZE, 500),
            max_offline_queue: limit(field::MAX_OFFLINE_QUEUE, 500),
            max_icon_bytes: reply.uint(field::MAX_ICON_BYTES).map(|v| match v {
                0 => icon::DEFAULT_MAX_BYTES,
                v => v.min(icon::CEILING_BYTES as u64) as u32,
            }),
            max_icon_dimension: limit(field::MAX_ICON_DIMENSION, icon::FLOOR_DIMENSION),
        },
        transport,
        encrypted,
        warnings,
        media: (caps & cap::INLINE_MEDIA != 0).then(|| MediaLimits::parse(&reply)),
    };
    Ok(start(io, text, info))
}

fn start(io: Io, text: TextMode, info: LoginInfo) -> Session {
    let Io {
        mut r,
        mut w,
        next_id,
        early,
    } = io;
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Transaction>();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel::<Event>();
    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let (stop_tx, mut stop_w) = watch::channel(false);
    let mut stop_r = stop_tx.subscribe();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                m = out_rx.recv() => match m {
                    Some(t) => if w.write(&t).await.is_err() { break },
                    None => break,
                },
                _ = stop_w.changed() => break,
            }
        }
        w.shutdown().await;
    });

    let client = Client {
        out: out_tx,
        pending: pending.clone(),
        next_id: Arc::new(AtomicU32::new(next_id)),
        stop: Arc::new(stop_tx),
        text,
        info: Arc::new(info),
    };

    for t in early {
        dispatch(&t, text, &ev_tx);
    }
    let auto = client.clone();
    tokio::spawn(async move {
        let reason = loop {
            let next = tokio::select! {
                t = r.read() => t,
                _ = stop_r.changed() => break "Signed off.".to_string(),
            };
            match next {
                Ok(t) if t.is_reply => {
                    let waiter = pending.lock().unwrap().remove(&t.id);
                    if let Some(w) = waiter {
                        let _ = w.send(t);
                    }
                }
                Ok(t) => {
                    // Delivered receipts go out on their own, as soon as a message lands.
                    if t.ty == tx::IM_DELIVER {
                        if let Some(m) = IncomingMessage::parse(&t, text) {
                            auto.ack_nowait(&m.guid, &m.from, AckKind::Delivered);
                        }
                    }
                    if t.ty == tx::DISCONNECT_MSG {
                        let msg = t
                            .bytes(field::DATA)
                            .map(|b| text.decode(b))
                            .unwrap_or_default();
                        break if msg.is_empty() {
                            "The server disconnected you.".to_string()
                        } else {
                            msg
                        };
                    }
                    dispatch(&t, text, &ev_tx);
                }
                Err(e) => {
                    break if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        "The server closed the connection.".to_string()
                    } else {
                        format!("Connection lost: {e}")
                    }
                }
            }
        };
        pending.lock().unwrap().clear();
        let _ = ev_tx.send(Event::Disconnected { reason });
    });

    Session {
        client,
        events: ev_rx,
    }
}

fn dispatch(t: &Transaction, text: TextMode, ev: &mpsc::UnboundedSender<Event>) {
    let s = |id| t.bytes(id).map(|b| text.decode(b));
    let e = match t.ty {
        tx::ROSTER_ENTRY => t
            .entries()
            .into_iter()
            .filter_map(|g| RosterEntry::parse(g, text))
            .map(|entry| Event::RosterEntry { entry })
            .collect(),
        tx::FRIEND_REQUEST => s(field::FRIEND_LOGIN)
            .map(|login| Event::FriendRequest {
                login,
                note: s(field::REQUEST_NOTE).filter(|n| !n.trim().is_empty()),
            })
            .into_iter()
            .collect(),
        tx::PRESENCE_CHANGED => PresenceUpdate::parse(t, text)
            .map(|update| Event::Presence { update })
            .into_iter()
            .collect(),
        tx::IM_DELIVER => IncomingMessage::parse(t, text)
            .map(|message| Event::Message { message })
            .into_iter()
            .collect(),
        tx::IM_ACK => match (t.bytes(field::MESSAGE_GUID), s(field::FRIEND_LOGIN)) {
            (Some(g), Some(login)) => vec![Event::Ack {
                guid: hex(g),
                login,
                ack: if t.uint(field::ACK_TYPE) == Some(2) {
                    AckKind::Read
                } else {
                    AckKind::Delivered
                },
            }],
            _ => vec![],
        },
        tx::IM_TYPING => s(field::FRIEND_LOGIN)
            .map(|login| Event::Typing {
                login,
                typing: t.uint(field::TYPING_STATE) == Some(1),
            })
            .into_iter()
            .collect(),
        tx::SHOW_AGREEMENT => {
            if t.uint(field::NO_AGREEMENT) == Some(1) {
                vec![]
            } else {
                vec![Event::Agreement {
                    text: s(field::DATA).unwrap_or_default(),
                }]
            }
        }
        tx::SERVER_MSG => match t.uint(field::USER_ID) {
            Some(id) => vec![Event::PrivateMessage {
                from_id: id as u16,
                from_name: s(field::USER_NAME).unwrap_or_default(),
                text: s(field::DATA).unwrap_or_default(),
                media: MediaRef::parse(t),
            }],
            None => s(field::DATA)
                .map(|text| Event::ServerMessage { text })
                .into_iter()
                .collect(),
        },
        tx::CHAT_MSG => s(field::DATA)
            .map(|text| Event::ChatMessage {
                chat_id: t.uint(field::CHAT_ID).map(|v| v as u32),
                text,
                media: MediaRef::parse(t),
            })
            .into_iter()
            .collect(),
        tx::NOTIFY_CHANGE_USER => ChatUser::from_fields(t, text)
            .map(|user| Event::UserChanged { user })
            .into_iter()
            .collect(),
        tx::NOTIFY_DELETE_USER => t
            .uint(field::USER_ID)
            .map(|id| Event::UserLeft { id: id as u16 })
            .into_iter()
            .collect(),
        tx::SET_BUDDY_ICON => vec![Event::OwnIconChanged {
            hash: t
                .bytes(field::BUDDY_ICON_HASH)
                .filter(|b| !b.is_empty())
                .map(hex),
        }],
        tx::ICON_CHANGE => t
            .uint(field::USER_ID)
            .map(|id| Event::GifIconChanged { user_id: id as u16 })
            .into_iter()
            .collect(),
        _ => vec![], // unknown notifications are ignored (guide §5.2)
    };
    for x in e {
        let _ = ev.send(x);
    }
}

/// Get User Info (825).
#[derive(Clone, Debug)]
pub struct UserInfo {
    pub name: Option<String>,
    pub profile: Option<Profile>,
    pub icon_hash: Option<String>,
}

/// Outcome of a successful request: its reason code (if any) and the reply.
pub struct Reply {
    pub reason: Option<u16>,
    pub t: Transaction,
}

impl Client {
    fn next_id(&self) -> u32 {
        loop {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            if id != 0 {
                return id;
            }
        }
    }

    /// Sends a request and waits for its reply; a non-zero error code is an `Err`
    /// carrying the server's own text (guide §16.1).
    pub async fn request(&self, ty: u16, fields: Vec<Field>) -> Result<Reply, Error> {
        let mut t = Transaction::request(ty, fields);
        t.id = self.next_id();
        let (tx_, rx_) = oneshot::channel();
        self.pending.lock().unwrap().insert(t.id, tx_);
        self.out.send(t.clone()).map_err(|_| Error::Closed)?;
        let r = match tokio::time::timeout(REQUEST_TIMEOUT, rx_).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => return Err(Error::Closed),
            Err(_) => {
                self.pending.lock().unwrap().remove(&t.id);
                return Err(Error::Timeout);
            }
        };
        let reason = r.uint(field::REASON_CODE).map(|v| v as u16);
        if r.error != 0 {
            let text = r
                .bytes(field::ERROR)
                .map(|b| self.text.decode(b))
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| reason_text(reason.unwrap_or(0xFFFF)).to_string());
            return Err(Error::Server { reason, text });
        }
        Ok(Reply { reason, t: r })
    }

    /// Fire-and-forget (typing, receipts): no one waits for the reply.
    pub fn notify(&self, ty: u16, fields: Vec<Field>) {
        let mut t = Transaction::request(ty, fields);
        t.id = self.next_id();
        let _ = self.out.send(t);
    }

    pub fn is_open(&self) -> bool {
        !self.out.is_closed()
    }

    fn s(&self, id: u16, v: &str) -> Field {
        Field::new(id, self.text.encode(v))
    }

    pub async fn agree(&self, nickname: &str, icon: u16) -> Result<Reply, Error> {
        self.request(
            tx::AGREED,
            vec![
                self.s(field::USER_NAME, nickname),
                Field::int(field::USER_ICON_ID, icon as u32),
                Field::int(field::OPTIONS, 0),
            ],
        )
        .await
    }

    pub async fn set_presence(
        &self,
        p: Presence,
        status: &str,
        discoverable: Option<bool>,
    ) -> Result<Reply, Error> {
        // Always send the status text: empty clears it, absent keeps it (guide §11.1).
        let mut f = vec![
            Field::u16(field::PRESENCE_STATE, p.to_u16()),
            self.s(field::PRESENCE_STATUS_TEXT, status),
        ];
        if let Some(d) = discoverable {
            f.push(Field::u16(field::DISCOVERABLE, d as u16));
        }
        self.request(tx::SET_PRESENCE, f).await
    }

    /// The readiness signal: roster, pending requests, then the offline backlog.
    pub async fn get_roster(&self) -> Result<Vec<RosterEntry>, Error> {
        let r = self.request(tx::GET_ROSTER, vec![]).await?;
        Ok(r.t
            .entries()
            .into_iter()
            .filter_map(|g| RosterEntry::parse(g, self.text))
            .collect())
    }

    pub async fn add_friend(&self, login: &str, note: &str) -> Result<Reply, Error> {
        let mut f = vec![self.s(field::FRIEND_LOGIN, login)];
        if !note.trim().is_empty() {
            f.push(self.s(field::REQUEST_NOTE, note));
        }
        self.request(tx::ADD_FRIEND, f).await
    }

    pub async fn respond(&self, login: &str, accept: bool) -> Result<Reply, Error> {
        let mut f = vec![self.s(field::FRIEND_LOGIN, login)];
        if accept {
            f.push(Field::u16(
                field::ROSTER_STATE,
                RosterState::Accepted.to_u16(),
            ));
        }
        self.request(tx::FRIEND_RESPONSE, f).await
    }

    pub async fn remove_friend(&self, login: &str) -> Result<Reply, Error> {
        self.request(tx::REMOVE_FRIEND, vec![self.s(field::FRIEND_LOGIN, login)])
            .await
    }

    pub async fn block(&self, login: &str) -> Result<Reply, Error> {
        self.request(tx::BLOCK_USER, vec![self.s(field::FRIEND_LOGIN, login)])
            .await
    }

    pub async fn unblock(&self, login: &str) -> Result<Reply, Error> {
        self.request(tx::UNBLOCK_USER, vec![self.s(field::FRIEND_LOGIN, login)])
            .await
    }

    pub async fn set_alias(&self, login: &str, alias: &str) -> Result<Reply, Error> {
        self.request(
            tx::SET_FRIEND_NICKNAME,
            vec![
                self.s(field::FRIEND_LOGIN, login),
                self.s(field::FRIEND_NICKNAME, alias),
            ],
        )
        .await
    }

    /// Returns the reason code: `OK`, or `OfflineQueued` for a friend who is away from the keyboard.
    pub async fn send_im(&self, to: &str, guid: &[u8; 16], body: &str) -> Result<u16, Error> {
        let bytes = self.text.encode(body);
        if bytes.len() > self.info.limits.max_message_bytes as usize {
            return Err(Error::Server {
                reason: Some(13),
                text: reason_text(13).into(),
            });
        }
        let r = self
            .request(
                tx::IM_SEND,
                vec![
                    self.s(field::FRIEND_LOGIN, to),
                    Field::new(field::MESSAGE_GUID, guid.to_vec()),
                    Field::new(field::MESSAGE_BODY, bytes),
                ],
            )
            .await?;
        Ok(r.reason.unwrap_or(reason::OK))
    }

    pub fn ack_nowait(&self, guid_hex: &str, from: &str, kind: AckKind) {
        let Some(g) = unhex(guid_hex) else { return };
        self.notify(
            tx::IM_ACK,
            vec![
                Field::new(field::MESSAGE_GUID, g),
                Field::u16(field::ACK_TYPE, if kind == AckKind::Read { 2 } else { 1 }),
                self.s(field::FRIEND_LOGIN, from),
            ],
        );
    }

    pub fn typing(&self, to: &str, typing: bool) {
        self.notify(
            tx::IM_TYPING,
            vec![
                self.s(field::FRIEND_LOGIN, to),
                Field::u16(field::TYPING_STATE, typing as u16),
            ],
        );
    }

    /// Exact lookup: (login, display name).
    pub async fn find_user(&self, login: &str) -> Result<(String, Option<String>), Error> {
        let r = self
            .request(tx::FIND_USER, vec![self.s(field::FRIEND_LOGIN, login)])
            .await?;
        if let Some(code) = r.reason.filter(|c| *c != reason::OK) {
            return Err(Error::Server {
                reason: Some(code),
                text: reason_text(code).into(),
            });
        }
        let l =
            r.t.bytes(field::FRIEND_LOGIN)
                .map(|b| self.text.decode(b))
                .unwrap_or_else(|| login.to_string());
        Ok((
            l,
            r.t.bytes(field::USER_NAME)
                .map(|b| self.text.decode(b))
                .filter(|s| !s.is_empty()),
        ))
    }

    pub async fn search(&self, query: &str) -> Result<Vec<(String, Option<String>)>, Error> {
        let r = self
            .request(tx::USER_SEARCH, vec![self.s(field::SEARCH_QUERY, query)])
            .await?;
        Ok(r.t
            .entries()
            .into_iter()
            .map(|g| {
                let get = |id| {
                    g.iter()
                        .find(|f| f.id == id)
                        .map(|f| self.text.decode(&f.data))
                };
                (
                    get(field::FRIEND_LOGIN).unwrap_or_default(),
                    get(field::USER_NAME).filter(|s| !s.is_empty()),
                )
            })
            .collect())
    }

    /// Their card: the profile and icon hash come only for friends and ourselves.
    pub async fn get_info(&self, login: &str) -> Result<UserInfo, Error> {
        let r = self
            .request(tx::GET_USER_INFO, vec![self.s(field::FRIEND_LOGIN, login)])
            .await?;
        let name =
            r.t.bytes(field::USER_NAME)
                .map(|b| self.text.decode(b))
                .filter(|s| !s.is_empty());
        let friend = r.reason != Some(reason::NOT_FRIENDS);
        Ok(UserInfo {
            name,
            profile: friend.then(|| Profile::parse(&r.t, self.text)),
            icon_hash: r
                .t
                .bytes(field::BUDDY_ICON_HASH)
                .filter(|b| friend && !b.is_empty())
                .map(hex),
        })
    }

    /// Whether this server keeps Buddy Icons.
    pub fn has_buddy_icons(&self) -> bool {
        self.info.limits.max_icon_bytes.is_some()
    }

    /// Set Buddy Icon (827); empty clears it. Returns the stored hash.
    pub async fn set_buddy_icon(&self, picture: &[u8]) -> Result<Option<String>, Error> {
        let max = self.info.limits.max_icon_bytes.unwrap_or(0) as usize;
        if !self.has_buddy_icons() {
            return Err(Error::Server {
                reason: None,
                text: "This server doesn't keep Buddy Icons.".into(),
            });
        }
        if picture.len() > max {
            return Err(Error::Server {
                reason: Some(reason::MESSAGE_TOO_LONG),
                text: format!("That picture is too big for this server ({} KB at most).", max / 1024),
            });
        }
        let r = self
            .request(tx::SET_BUDDY_ICON, vec![Field::new(field::BUDDY_ICON, picture.to_vec())])
            .await?;
        Ok(r.t.bytes(field::BUDDY_ICON_HASH).filter(|b| !b.is_empty()).map(hex))
    }

    /// Get Buddy Icon (828): (hash, picture) for a friend or ourselves; None when they
    /// have no icon, or aren't a friend (the server doesn't say which).
    pub async fn get_buddy_icon(&self, login: &str) -> Result<Option<(String, Vec<u8>)>, Error> {
        let r = self
            .request(tx::GET_BUDDY_ICON, vec![self.s(field::FRIEND_LOGIN, login)])
            .await?;
        Ok(match (r.t.bytes(field::BUDDY_ICON_HASH), r.t.bytes(field::BUDDY_ICON)) {
            (Some(h), Some(p)) if !p.is_empty() => Some((hex(h), p.to_vec())),
            _ => None,
        })
    }

    pub async fn set_info(&self, p: &Profile) -> Result<Reply, Error> {
        self.request(tx::SET_USER_INFO, p.to_fields(self.text))
            .await
    }

    /// Public chat (105); `emote` is the "alternate" style (/me).
    pub fn send_chat(&self, text: &str, emote: bool) {
        let mut f = vec![Field::new(field::DATA, self.text.encode(text))];
        if emote {
            f.push(Field::int(field::CHAT_OPTIONS, 1));
        }
        self.notify(tx::SEND_CHAT, f);
    }

    /// Everyone on the server (300).
    pub async fn get_users(&self) -> Result<Vec<ChatUser>, Error> {
        let r = self.request(tx::GET_USER_NAME_LIST, vec![]).await?;
        Ok(r.t
            .fields
            .iter()
            .filter(|f| f.id == field::USER_NAME_WITH_INFO)
            .filter_map(|f| ChatUser::parse(&f.data, self.text))
            .collect())
    }

    /// Agreed (121) as a classic client sends it; some servers never reply.
    pub fn agree_nowait(&self, nickname: &str, icon: u16) {
        self.notify(
            tx::AGREED,
            vec![
                self.s(field::USER_NAME, nickname),
                Field::int(field::USER_ICON_ID, icon as u32),
                Field::int(field::OPTIONS, 0),
            ],
        );
    }

    /// Signs off: the writer closes its side, the reader stops, pending requests fail.
    pub fn disconnect(&self) {
        let _ = self.stop.send(true);
        self.pending.lock().unwrap().clear();
    }
}
