//! An in-memory Hotline IM server for tests and for running the app without a
//! real server. It follows the spec's server rules closely enough to exercise
//! every client flow; it is not Janus and is not meant to be deployed.

use crate::frame::{FrameReader, FrameWriter, Sealer, DIR_CLIENT_TO_SERVER, DIR_SERVER_TO_CLIENT};
use crate::hope::{self, MacAlg};
use crate::messaging::{Presence, RosterState};
use crate::wire::{cap, decode_name_list, encode_name_list, field, invert, tx, Field, Transaction};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct MockConfig {
    pub bind: SocketAddr,
    pub hope: bool,
    pub aead: bool,
    /// Ask clients to MAC the login too (the other Step 2 branch).
    pub mac_login: bool,
    pub agreement: Option<String>,
    pub server_name: String,
    /// Also answer the info port (bind port - 1).
    pub info_port: bool,
    /// An empty login signs on as a guest.
    pub allow_guest: bool,
    /// Buddy icons: the largest picture kept (None = not supported).
    pub max_icon_bytes: Option<u32>,
    pub max_icon_dimension: u32,
    /// Pictures in classic chat (inline media), and GIF icons.
    pub media: bool,
}

/// The test server's picture chunk size: small, so tests cross chunk boundaries.
pub const MOCK_MEDIA_CHUNK: usize = 4096;

impl Default for MockConfig {
    fn default() -> Self {
        MockConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            hope: true,
            aead: true,
            mac_login: false,
            agreement: None,
            server_name: "Mock Hotline".into(),
            info_port: false,
            allow_guest: true,
            max_icon_bytes: Some(16384),
            max_icon_dimension: 128,
            media: true,
        }
    }
}

struct Account {
    password: String,
    name: String,
    status_text: Option<String>,
    discoverable: bool,
    profile: Vec<Field>,
    /// (picture, hash)
    icon: Option<(Vec<u8>, [u8; 16])>,
}

struct Rel {
    state: RosterState,
    alias: Option<String>,
}

struct Sess {
    login: String,
    tx: mpsc::UnboundedSender<Transaction>,
    presence: Presence,
    caps: u16,
    /// Classic user-list identity (chat rooms).
    name: String,
    icon: u16,
    flags: u16,
}

impl Sess {
    /// Pure messengers (bit 8) never appear in the classic user list.
    fn visible(&self) -> bool {
        self.caps & cap::MESSENGER_SESSION == 0
    }
}

#[derive(Default)]
struct State {
    accounts: HashMap<String, Account>,
    /// (owner, other) → owner's row about other
    rel: HashMap<(String, String), Rel>,
    sessions: HashMap<u64, Sess>,
    next_sid: u64,
    offline: HashMap<String, Vec<Transaction>>,
    seen: HashSet<(Vec<u8>, String)>,
    /// Read receipts waiting for an offline sender.
    receipts: HashMap<String, Vec<Transaction>>,
    /// Inline media: handle → (bytes, MIME); upload token → bytes so far.
    media: HashMap<Vec<u8>, (Vec<u8>, String)>,
    uploads: HashMap<Vec<u8>, Vec<u8>>,
    /// GIF icons, by session.
    gif_icons: HashMap<u64, Vec<u8>>,
}

#[derive(Clone)]
pub struct MockServer {
    pub addr: SocketAddr,
    state: Arc<Mutex<State>>,
    cfg: Arc<MockConfig>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn notify(ty: u16, fields: Vec<Field>) -> Transaction {
    Transaction::request(ty, fields)
}

fn reason(code: u16) -> Field {
    Field::u16(field::REASON_CODE, code)
}

fn fail(req: &Transaction, code: u16, text: &str) -> Transaction {
    let mut r = Transaction::reply_to(req, vec![reason(code), Field::new(field::ERROR, text)]);
    r.error = 1;
    r
}

impl MockServer {
    pub async fn start(cfg: MockConfig) -> std::io::Result<MockServer> {
        let listener = TcpListener::bind(cfg.bind).await?;
        let addr = listener.local_addr()?;
        let server = MockServer {
            addr,
            state: Arc::default(),
            cfg: Arc::new(cfg),
        };
        if server.cfg.info_port {
            let info = TcpListener::bind((addr.ip(), addr.port() - 1)).await?;
            let port = addr.port();
            let hope = server.cfg.hope;
            tokio::spawn(async move {
                while let Ok((mut s, _)) = info.accept().await {
                    tokio::spawn(async move {
                        let mut req = [0u8; 8];
                        if s.read_exact(&mut req).await.is_err() || &req[..4] != b"HLIP" {
                            return;
                        }
                        let body = serde_json::json!({
                            "infoVersion": 1,
                            "dataPort": port,
                            "software": {"name": "mock", "version": "0"},
                            "transport": {
                                "hope": {"supported": hope, "required": false},
                                "tls": {"supported": false, "required": false},
                                "plaintext": {"accepted": true},
                                "compression": {"supported": false}
                            }
                        })
                        .to_string();
                        let mut out = b"HLIP\x00\x01\x00\x00".to_vec();
                        out.extend((body.len() as u32).to_be_bytes());
                        out.extend(body.as_bytes());
                        let _ = s.write_all(&out).await;
                    });
                }
            });
        }
        let srv = server.clone();
        tokio::spawn(async move {
            while let Ok((s, _)) = listener.accept().await {
                let srv = srv.clone();
                tokio::spawn(async move {
                    let _ = srv.serve(s).await;
                });
            }
        });
        Ok(server)
    }

    pub fn add_account(&self, login: &str, password: &str, name: &str) {
        self.state.lock().unwrap().accounts.insert(
            login.into(),
            Account {
                password: password.into(),
                name: name.into(),
                status_text: None,
                discoverable: true,
                profile: vec![],
                icon: None,
            },
        );
    }

    /// Gives an account a Buddy Icon, as if it had set one.
    pub fn set_icon(&self, login: &str, pic: Vec<u8>) {
        let hash = crate::icon::hash(&pic);
        if let Some(a) = self.state.lock().unwrap().accounts.get_mut(login) {
            a.icon = Some((pic, hash));
        }
    }

    /// Makes two accounts accepted friends.
    pub fn befriend(&self, a: &str, b: &str) {
        let mut st = self.state.lock().unwrap();
        st.rel.insert(
            (a.into(), b.into()),
            Rel {
                state: RosterState::Accepted,
                alias: None,
            },
        );
        st.rel.insert(
            (b.into(), a.into()),
            Rel {
                state: RosterState::Accepted,
                alias: None,
            },
        );
    }

    /// An always-online account that answers every message (for trying the app).
    pub fn add_bot(
        &self,
        login: &str,
        name: &str,
        status: &str,
        reply: impl Fn(&str, &str) -> String + Send + 'static,
    ) {
        self.add_account(login, &format!("{:x}", rand::random::<u64>()), name);
        let (tx_, mut rx) = mpsc::unbounded_channel::<Transaction>();
        {
            let mut st = self.state.lock().unwrap();
            st.accounts.get_mut(login).unwrap().status_text = Some(status.into());
            st.next_sid += 1;
            let sid = st.next_sid;
            st.sessions.insert(
                sid,
                Sess {
                    login: login.into(),
                    tx: tx_,
                    presence: Presence::Online,
                    caps: cap::MESSAGING | cap::MESSENGER_SESSION | cap::TEXT_ENCODING,
                    name: name.into(),
                    icon: 0,
                    flags: 0,
                },
            );
        }
        let srv = self.clone();
        let me = login.to_string();
        tokio::spawn(async move {
            while let Some(t) = rx.recv().await {
                match t.ty {
                    tx::FRIEND_REQUEST => {
                        let from = String::from_utf8_lossy(
                            t.bytes(field::FRIEND_LOGIN).unwrap_or_default(),
                        )
                        .into_owned();
                        let mut req = Transaction::request(
                            tx::FRIEND_RESPONSE,
                            vec![
                                Field::new(field::FRIEND_LOGIN, from),
                                Field::u16(field::ROSTER_STATE, 3),
                            ],
                        );
                        req.id = 1;
                        srv.handle(&me, 0, &req);
                    }
                    tx::IM_DELIVER => {
                        let from = String::from_utf8_lossy(
                            t.bytes(field::FRIEND_LOGIN).unwrap_or_default(),
                        )
                        .into_owned();
                        let body = String::from_utf8_lossy(
                            t.bytes(field::MESSAGE_BODY).unwrap_or_default(),
                        )
                        .into_owned();
                        let guid = t.bytes(field::MESSAGE_GUID).unwrap_or_default().to_vec();
                        let ack = |kind| {
                            let mut a = Transaction::request(
                                tx::IM_ACK,
                                vec![
                                    Field::new(field::MESSAGE_GUID, guid.clone()),
                                    Field::u16(field::ACK_TYPE, kind),
                                    Field::new(field::FRIEND_LOGIN, from.clone()),
                                ],
                            );
                            a.id = 1;
                            a
                        };
                        srv.handle(&me, 0, &ack(1));
                        let typing = |on| {
                            let mut a = Transaction::request(
                                tx::IM_TYPING,
                                vec![
                                    Field::new(field::FRIEND_LOGIN, from.clone()),
                                    Field::u16(field::TYPING_STATE, on),
                                ],
                            );
                            a.id = 1;
                            a
                        };
                        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                        srv.handle(&me, 0, &ack(2));
                        srv.handle(&me, 0, &typing(1));
                        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
                        srv.handle(&me, 0, &typing(0));
                        let text = reply(&from, &body);
                        let mut send = Transaction::request(
                            tx::IM_SEND,
                            vec![
                                Field::new(field::FRIEND_LOGIN, from.clone()),
                                Field::new(
                                    field::MESSAGE_GUID,
                                    crate::messaging::new_guid().to_vec(),
                                ),
                                Field::new(field::MESSAGE_BODY, text),
                            ],
                        );
                        send.id = 1;
                        srv.handle(&me, 0, &send);
                    }
                    _ => {}
                }
            }
        });
    }

    async fn serve(&self, mut s: TcpStream) -> std::io::Result<()> {
        let local = s.local_addr()?;
        let mut hs = [0u8; 12];
        s.read_exact(&mut hs).await?;
        if &hs[..8] != b"TRTPHOTL" {
            return Ok(());
        }
        s.write_all(b"TRTP\x00\x00\x00\x00").await?;
        let (r, w) = s.into_split();
        let mut r = FrameReader::new(r);
        let mut w = FrameWriter::new(w);

        let mut req = r.read().await?;
        if req.ty != tx::LOGIN {
            return Ok(());
        }
        let mut hope_ctx = None;
        if self.cfg.hope
            && req.bytes(field::USER_LOGIN) == Some(&[0][..])
            && req.has(field::HOPE_MAC_ALGORITHM)
        {
            let offered = decode_name_list(req.bytes(field::HOPE_MAC_ALGORITHM).unwrap());
            let alg = offered
                .iter()
                .filter_map(|n| MacAlg::parse(n))
                .find(|a| matches!(a, MacAlg::HmacSha256 | MacAlg::HmacSha1 | MacAlg::Inverse))
                .unwrap_or(MacAlg::Inverse);
            let mut key = Vec::with_capacity(64);
            if let std::net::IpAddr::V4(ip) = local.ip() {
                key.extend(ip.octets());
            } else {
                key.extend([0, 0, 0, 0]);
            }
            key.extend(local.port().to_be_bytes());
            key.extend((0..58).map(|_| rand::random::<u8>()));
            let wants_aead = req
                .bytes(field::HOPE_CLIENT_CIPHER)
                .map(decode_name_list)
                .is_some_and(|l| {
                    l.iter()
                        .any(|c| hope::normalize_cipher(c) == hope::AEAD_CIPHER)
                });
            let aead = self.cfg.aead && wants_aead && alg != MacAlg::Inverse;
            let mut f = vec![
                Field::new(field::HOPE_SESSION_KEY, key.clone()),
                Field::new(field::HOPE_MAC_ALGORITHM, encode_name_list(&[alg.name()])),
                Field::new(
                    field::USER_LOGIN,
                    if self.cfg.mac_login {
                        alg.name().as_bytes().to_vec()
                    } else {
                        vec![]
                    },
                ),
                Field::new(field::HOPE_APP_ID, b"MOCK".to_vec()),
                Field::new(field::HOPE_APP_STRING, "Mock 0"),
            ];
            if aead {
                let l = encode_name_list(&[hope::AEAD_CIPHER]);
                f.push(Field::new(field::HOPE_SERVER_CIPHER, l.clone()));
                f.push(Field::new(field::HOPE_CLIENT_CIPHER, l));
                f.push(Field::new(field::HOPE_SERVER_CIPHER_MODE, "AEAD"));
                f.push(Field::new(field::HOPE_CLIENT_CIPHER_MODE, "AEAD"));
            }
            w.write(&Transaction::reply_to(&req, f)).await?;
            req = r.read().await?;
            hope_ctx = Some((alg, key, aead));
        }

        // Authenticate.
        let login_field = req.bytes(field::USER_LOGIN).unwrap_or_default().to_vec();
        let pw_field = req.bytes(field::USER_PASSWORD).unwrap_or_default().to_vec();
        let found = {
            let st = self.state.lock().unwrap();
            st.accounts
                .iter()
                .find(|(login, acct)| {
                    let (l_ok, p_ok) = match &hope_ctx {
                        Some((alg, key, _)) => (
                            if self.cfg.mac_login {
                                alg.mac(login.as_bytes(), key) == login_field
                            } else {
                                invert(login.as_bytes()) == login_field
                            },
                            alg.mac(acct.password.as_bytes(), key) == pw_field,
                        ),
                        None => (
                            invert(login.as_bytes()) == login_field,
                            invert(acct.password.as_bytes()) == pw_field,
                        ),
                    };
                    l_ok && p_ok
                })
                .map(|(l, a)| (l.clone(), a.password.clone()))
        };
        if let Some((alg, key, true)) = &hope_ctx {
            if let Some((_, pw)) = &found {
                let keys = hope::aead_keys(*alg, pw.as_bytes(), key).unwrap();
                w.enable_aead(Sealer::new(&keys.encode, DIR_SERVER_TO_CLIENT));
                r.enable_aead(Sealer::new(&keys.decode, DIR_CLIENT_TO_SERVER));
            }
        }
        let guest = hope_ctx.is_none() && login_field.is_empty() && self.cfg.allow_guest;
        let found = found.or_else(|| guest.then(|| ("guest".to_string(), String::new())));
        let Some((login, _)) = found else {
            let mut f =
                Transaction::reply_to(&req, vec![Field::new(field::ERROR, "Incorrect login.")]);
            f.error = 1;
            w.write(&f).await?;
            return Ok(());
        };
        let next_uid = (self.state.lock().unwrap().next_sid + 1) as u32;
        let asked = req.uint(field::CAPABILITIES).unwrap_or(0) as u16;
        let mut ours = cap::MESSAGING | cap::MESSENGER_SESSION | cap::TEXT_ENCODING;
        if self.cfg.media {
            ours |= cap::INLINE_MEDIA;
        }
        let confirmed = asked & ours;
        let mut lf = vec![
            Field::int(field::VERSION, 197),
            Field::new(field::SERVER_NAME, self.cfg.server_name.clone()),
            Field::int(field::USER_ID, next_uid),
            Field::u16(field::CAPABILITIES, confirmed),
            Field::u32(field::MAX_MESSAGE_BYTES, 4096),
        ];
        if confirmed & cap::INLINE_MEDIA != 0 {
            lf.push(Field::u32(field::MEDIA_MAX_BYTES, 256 * 1024));
            lf.push(Field::u32(field::MEDIA_MAX_DIMENSION, 2048));
            lf.push(Field::u32(field::MEDIA_CHUNK_SIZE, MOCK_MEDIA_CHUNK as u32));
        }
        if let (Some(max), true) = (self.cfg.max_icon_bytes, confirmed & cap::MESSAGING != 0) {
            lf.push(Field::u32(field::MAX_ICON_BYTES, max));
            lf.push(Field::u32(field::MAX_ICON_DIMENSION, self.cfg.max_icon_dimension));
        }
        w.write(&Transaction::reply_to(&req, lf)).await?;

        // Register the session; friends see us come online.
        let (tx_, mut rx) = mpsc::unbounded_channel::<Transaction>();
        let sid = {
            let mut st = self.state.lock().unwrap();
            st.next_sid += 1;
            let sid = st.next_sid;
            st.sessions.insert(
                sid,
                Sess {
                    login: login.clone(),
                    tx: tx_.clone(),
                    presence: Presence::Online,
                    caps: confirmed,
                    name: req
                        .bytes(field::USER_NAME)
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .unwrap_or_else(|| login.clone()),
                    icon: req.uint(field::USER_ICON_ID).unwrap_or(0) as u16,
                    flags: 0,
                },
            );
            sid
        };
        self.broadcast_presence(&login);
        self.announce_user(sid);
        if let Some(text) = &self.cfg.agreement {
            let _ = tx_.send(notify(
                tx::SHOW_AGREEMENT,
                vec![Field::new(field::DATA, text.clone())],
            ));
        }

        let writer = tokio::spawn(async move {
            while let Some(t) = rx.recv().await {
                if w.write(&t).await.is_err() {
                    break;
                }
            }
        });
        while let Ok(t) = r.read().await {
            if let Some(reply) = self.handle(&login, sid, &t) {
                let _ = tx_.send(reply);
            }
            if t.ty == tx::GET_ROSTER {
                self.after_roster(&login, &tx_);
            }
        }
        let gone = {
            let mut st = self.state.lock().unwrap();
            st.gif_icons.remove(&sid);
            st.sessions.remove(&sid)
        };
        if gone.is_some_and(|g| g.visible()) {
            let st = self.state.lock().unwrap();
            Self::to_visible(
                &st,
                None,
                &notify(
                    tx::NOTIFY_DELETE_USER,
                    vec![Field::int(field::USER_ID, sid as u32)],
                ),
            );
        }
        self.broadcast_presence(&login);
        writer.abort();
        Ok(())
    }

    /// Classic broadcasts go to every visible session (except `skip`).
    fn to_visible(st: &State, skip: Option<u64>, t: &Transaction) {
        for (id, s) in &st.sessions {
            if s.visible() && Some(*id) != skip {
                let _ = s.tx.send(t.clone());
            }
        }
    }

    fn user_fields(sid: u64, s: &Sess) -> Vec<Field> {
        vec![
            Field::int(field::USER_ID, sid as u32),
            Field::int(field::USER_ICON_ID, s.icon as u32),
            Field::int(field::USER_FLAGS, s.flags as u32),
            Field::new(field::USER_NAME, s.name.clone()),
        ]
    }

    fn user_info(sid: u64, s: &Sess) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend((sid as u16).to_be_bytes());
        b.extend(s.icon.to_be_bytes());
        b.extend(s.flags.to_be_bytes());
        b.extend((s.name.len() as u16).to_be_bytes());
        b.extend(s.name.as_bytes());
        b
    }

    /// Notify Change User (301) about `sid` to everyone else in the classic world.
    fn announce_user(&self, sid: u64) {
        let st = self.state.lock().unwrap();
        if let Some(s) = st.sessions.get(&sid).filter(|s| s.visible()) {
            Self::to_visible(
                &st,
                Some(sid),
                &notify(tx::NOTIFY_CHANGE_USER, Self::user_fields(sid, s)),
            );
        }
    }

    fn send_to(st: &State, login: &str, t: &Transaction) -> bool {
        let mut any = false;
        for s in st.sessions.values().filter(|s| s.login == login) {
            any |= s.tx.send(t.clone()).is_ok();
        }
        any
    }

    fn visible(st: &State, login: &str) -> Presence {
        let mut best = Presence::Offline;
        for s in st.sessions.values().filter(|s| s.login == login) {
            best = match (best, s.presence) {
                (_, Presence::Invisible) => best,
                (_, Presence::Online) | (Presence::Online, _) => Presence::Online,
                (_, Presence::Busy) | (Presence::Busy, _) => Presence::Busy,
                (_, Presence::Away) | (Presence::Away, _) => Presence::Away,
                _ => best,
            };
        }
        best
    }

    fn caps_of(st: &State, login: &str) -> Option<u16> {
        st.sessions
            .values()
            .find(|s| s.login == login)
            .map(|s| s.caps)
    }

    fn entry(st: &State, owner: &str, other: &str) -> Vec<Field> {
        let mut f = vec![Field::new(field::FRIEND_LOGIN, other)];
        let Some(rel) = st.rel.get(&(owner.to_string(), other.to_string())) else {
            f.push(Field::u16(field::ROSTER_STATE, 0));
            return f;
        };
        if let Some(a) = &rel.alias {
            f.push(Field::new(field::FRIEND_NICKNAME, a.clone()));
        }
        f.push(Field::u16(field::ROSTER_STATE, rel.state.to_u16()));
        if rel.state == RosterState::Accepted {
            let p = Self::visible(st, other);
            if p != Presence::Offline {
                f.push(Field::u16(field::PRESENCE_STATE, p.to_u16()));
                if let Some(c) = Self::caps_of(st, other) {
                    f.push(Field::u16(field::FRIEND_CAPABILITIES, c));
                }
            }
            if let Some(acct) = st.accounts.get(other) {
                if let Some(s) = &acct.status_text {
                    f.push(Field::new(field::PRESENCE_STATUS_TEXT, s.clone()));
                }
                f.push(Field::new(field::USER_NAME, acct.name.clone()));
                if let Some((_, h)) = &acct.icon {
                    f.push(Field::new(field::BUDDY_ICON_HASH, h.to_vec()));
                }
            }
        }
        f
    }

    fn roster_delta(st: &State, owner: &str, other: &str) {
        Self::send_to(
            st,
            owner,
            &notify(tx::ROSTER_ENTRY, Self::entry(st, owner, other)),
        );
    }

    fn broadcast_presence(&self, login: &str) {
        let st = self.state.lock().unwrap();
        let p = Self::visible(&st, login);
        let mut f = vec![
            Field::new(field::FRIEND_LOGIN, login),
            Field::u16(field::PRESENCE_STATE, p.to_u16()),
        ];
        if let Some(acct) = st.accounts.get(login) {
            if let Some(s) = &acct.status_text {
                f.push(Field::new(field::PRESENCE_STATUS_TEXT, s.clone()));
            }
            f.push(Field::new(field::USER_NAME, acct.name.clone()));
            if let Some((_, h)) = &acct.icon {
                f.push(Field::new(field::BUDDY_ICON_HASH, h.to_vec()));
            }
        }
        if p != Presence::Offline {
            if let Some(c) = Self::caps_of(&st, login) {
                f.push(Field::u16(field::FRIEND_CAPABILITIES, c));
            }
        }
        let t = notify(tx::PRESENCE_CHANGED, f);
        for ((owner, other), rel) in &st.rel {
            if other == login && rel.state == RosterState::Accepted {
                Self::send_to(&st, owner, &t);
            }
        }
    }

    fn after_roster(&self, login: &str, out: &mpsc::UnboundedSender<Transaction>) {
        let mut st = self.state.lock().unwrap();
        let pending: Vec<String> = st
            .rel
            .iter()
            .filter(|((o, _), r)| o == login && r.state == RosterState::PendingIn)
            .map(|((_, other), _)| other.clone())
            .collect();
        for p in pending {
            let _ = out.send(notify(
                tx::FRIEND_REQUEST,
                vec![Field::new(field::FRIEND_LOGIN, p)],
            ));
        }
        for t in st.offline.remove(login).unwrap_or_default() {
            let _ = out.send(t);
        }
        for t in st.receipts.remove(login).unwrap_or_default() {
            let _ = out.send(t);
        }
    }

    fn rel_state(st: &State, a: &str, b: &str) -> Option<RosterState> {
        st.rel.get(&(a.to_string(), b.to_string())).map(|r| r.state)
    }

    /// Handles one request from `me`; returns the reply (if the type has one).
    fn handle(&self, me: &str, sid: u64, t: &Transaction) -> Option<Transaction> {
        let s = |id| t.bytes(id).map(|b| String::from_utf8_lossy(b).into_owned());
        let who = s(field::FRIEND_LOGIN).unwrap_or_default();
        let mut guard = self.state.lock().unwrap();
        let st = &mut *guard;
        let ok = |extra: Vec<Field>| {
            let mut f = vec![reason(0)];
            f.extend(extra);
            Some(Transaction::reply_to(t, f))
        };
        match t.ty {
            tx::AGREED | tx::SET_CLIENT_USER_INFO => {
                if let Some(sess) = st.sessions.get_mut(&sid) {
                    if let Some(n) = s(field::USER_NAME) {
                        sess.name = n;
                    }
                    if let Some(i) = t.uint(field::USER_ICON_ID) {
                        sess.icon = i as u16;
                    }
                }
                drop(guard);
                self.announce_user(sid);
                (t.ty == tx::AGREED).then(|| Transaction::reply_to(t, vec![]))
            }
            tx::SEND_CHAT => {
                let me_s = st.sessions.get(&sid)?;
                let text = s(field::DATA).unwrap_or_default();
                let line = if t.uint(field::CHAT_OPTIONS) == Some(1) {
                    format!("\r *** {} {}", me_s.name, text)
                } else {
                    format!("\r{:>13}:  {}", me_s.name, text)
                };
                let plain = notify(tx::CHAT_MSG, vec![Field::new(field::DATA, line)]);
                // A picture goes only to sessions that can show one (the rest get the text).
                let pic = t
                    .bytes(field::MEDIA_ID)
                    .filter(|_| me_s.caps & cap::INLINE_MEDIA != 0)
                    .and_then(|h| st.media.get(h).map(|(b, m)| (h.to_vec(), b.len(), m.clone())));
                for s in st.sessions.values().filter(|s| s.visible()) {
                    let mut out = plain.clone();
                    if let (Some((h, n, m)), true) = (&pic, s.caps & cap::INLINE_MEDIA != 0) {
                        out.fields.push(Field::new(field::MEDIA_ID, h.clone()));
                        out.fields.push(Field::new(field::MEDIA_TYPE, m.clone()));
                        out.fields.push(Field::u32(field::MEDIA_BYTES, *n as u32));
                    }
                    let _ = s.tx.send(out);
                }
                None
            }
            tx::SEND_INSTANT_MSG => {
                // A classic private message: delivered as Server Message (104) naming the sender.
                let from = st.sessions.get(&sid)?;
                let to = t.uint(field::USER_ID).unwrap_or(0);
                let msg = notify(
                    tx::SERVER_MSG,
                    vec![
                        Field::int(field::USER_ID, sid as u32),
                        Field::new(field::USER_NAME, from.name.clone()),
                        Field::new(field::DATA, t.bytes(field::DATA).unwrap_or_default().to_vec()),
                    ],
                );
                match st.sessions.get(&to).filter(|s| s.visible()) {
                    Some(dest) => {
                        let _ = dest.tx.send(msg);
                        Some(Transaction::reply_to(t, vec![]))
                    }
                    None => Some(fail(t, 0, "That user isn't here.")),
                }
            }
            tx::UPLOAD_MEDIA => {
                if st.sessions.get(&sid).map(|s| s.caps & cap::INLINE_MEDIA) != Some(cap::INLINE_MEDIA) {
                    return Some(fail(t, 0, "Media rejected"));
                }
                let part = t.bytes(field::MEDIA_PAYLOAD).unwrap_or_default().to_vec();
                let fin = t.uint(field::MEDIA_PART_FINAL).unwrap_or(0) != 0;
                let data = match t.bytes(field::MEDIA_UPLOAD_TOKEN) {
                    Some(tok) => match st.uploads.get_mut(tok) {
                        Some(sofar) => {
                            sofar.extend_from_slice(&part);
                            if !fin {
                                return ok(vec![]);
                            }
                            st.uploads.remove(tok).unwrap()
                        }
                        None => return Some(fail(t, 0, "Media rejected")),
                    },
                    None if !fin => {
                        let tok = crate::messaging::new_guid().to_vec();
                        st.uploads.insert(tok.clone(), part);
                        return ok(vec![Field::new(field::MEDIA_UPLOAD_TOKEN, tok)]);
                    }
                    None => part,
                };
                let mime = crate::media::mime_of(&data);
                if !mime.starts_with("image/") {
                    return Some(fail(t, 0, "Unsupported media"));
                }
                let h = crate::messaging::new_guid().to_vec();
                let n = data.len();
                st.media.insert(h.clone(), (data, mime.to_string()));
                ok(vec![
                    Field::new(field::MEDIA_ID, h),
                    Field::new(field::MEDIA_TYPE, mime),
                    Field::u32(field::MEDIA_BYTES, n as u32),
                ])
            }
            tx::DOWNLOAD_MEDIA => {
                let Some((data, mime)) = t.bytes(field::MEDIA_ID).and_then(|h| st.media.get(h)) else {
                    return Some(fail(t, 0, "Media not found"));
                };
                let parts: Vec<&[u8]> = data.chunks(MOCK_MEDIA_CHUNK).collect();
                let i = t.uint(field::MEDIA_PART_INDEX).unwrap_or(0) as usize;
                let Some(part) = parts.get(i) else { return Some(fail(t, 0, "Media not found")) };
                ok(vec![
                    Field::new(field::MEDIA_PAYLOAD, part.to_vec()),
                    Field::new(field::MEDIA_TYPE, mime.clone()),
                    Field::u16(field::MEDIA_PART_COUNT, parts.len() as u16),
                    Field::new(field::MEDIA_PART_FINAL, [(i + 1 == parts.len()) as u8]),
                ])
            }
            tx::ICON_SET if self.cfg.media => {
                let gif = t.bytes(field::GIF_ICON_DATA).unwrap_or_default().to_vec();
                if !gif.is_empty() && !gif.starts_with(b"GIF8") {
                    return Some(fail(t, 0, "Not a GIF."));
                }
                if gif.is_empty() {
                    st.gif_icons.remove(&sid);
                } else {
                    st.gif_icons.insert(sid, gif);
                }
                Self::to_visible(st, None, &notify(tx::ICON_CHANGE, vec![Field::int(field::USER_ID, sid as u32)]));
                ok(vec![])
            }
            tx::ICON_GET if self.cfg.media => {
                let uid = t.uint(field::USER_ID).unwrap_or(0);
                let mut f = vec![Field::int(field::USER_ID, uid as u32)];
                if let Some(g) = st.gif_icons.get(&uid) {
                    f.push(Field::new(field::GIF_ICON_DATA, g.clone()));
                }
                ok(f)
            }
            tx::ICON_GET_LIST if self.cfg.media => ok(st
                .gif_icons
                .iter()
                .map(|(uid, g)| {
                    let mut d = (*uid as u16).to_be_bytes().to_vec();
                    d.extend((g.len() as u16).to_be_bytes());
                    d.extend(g);
                    Field::new(field::ICON_LIST_ENTRY, d)
                })
                .collect()),
            tx::GET_USER_NAME_LIST => {
                let mut ids: Vec<_> = st
                    .sessions
                    .iter()
                    .filter(|(_, s)| s.visible())
                    .map(|(id, _)| *id)
                    .collect();
                ids.sort();
                let f = ids
                    .iter()
                    .map(|id| {
                        Field::new(
                            field::USER_NAME_WITH_INFO,
                            Self::user_info(*id, &st.sessions[id]),
                        )
                    })
                    .collect();
                Some(Transaction::reply_to(t, f))
            }
            tx::SET_PRESENCE => {
                let p = Presence::from_u16(t.uint(field::PRESENCE_STATE).unwrap_or(1) as u16);
                if p == Presence::Offline {
                    return Some(fail(t, 0, "Offline is not a state you can set."));
                }
                if let Some(sess) = st.sessions.get_mut(&sid) {
                    sess.presence = p;
                }
                if let Some(acct) = st.accounts.get_mut(me) {
                    if let Some(text) = s(field::PRESENCE_STATUS_TEXT) {
                        acct.status_text = (!text.trim().is_empty()).then_some(text);
                    }
                    if let Some(d) = t.uint(field::DISCOVERABLE) {
                        acct.discoverable = d == 1;
                    }
                }
                drop(guard);
                self.broadcast_presence(me);
                Some(Transaction::reply_to(t, vec![reason(0)]))
            }
            tx::GET_ROSTER => {
                let mut f = Vec::new();
                let others: Vec<String> = st
                    .rel
                    .keys()
                    .filter(|(o, _)| o == me)
                    .map(|(_, x)| x.clone())
                    .collect();
                for other in others {
                    f.extend(Self::entry(st, me, &other));
                }
                Some(Transaction::reply_to(t, f))
            }
            tx::ADD_FRIEND => {
                if !st.accounts.contains_key(&who)
                    || who == me
                    || Self::rel_state(st, &who, me) == Some(RosterState::Blocked)
                {
                    return Some(fail(t, 1, "No such user."));
                }
                match Self::rel_state(st, me, &who) {
                    Some(RosterState::Accepted) => return Some(fail(t, 4, "Already friends.")),
                    Some(RosterState::PendingOut) => {
                        return Some(fail(t, 5, "Request already sent."))
                    }
                    Some(RosterState::Blocked) => return Some(fail(t, 3, "Unblock them first.")),
                    _ => {}
                }
                if Self::rel_state(st, me, &who) == Some(RosterState::PendingIn) {
                    // They already asked us: adding back accepts.
                    for (a, b) in [(me, who.as_str()), (who.as_str(), me)] {
                        st.rel.insert(
                            (a.into(), b.into()),
                            Rel {
                                state: RosterState::Accepted,
                                alias: None,
                            },
                        );
                    }
                    Self::roster_delta(st, me, &who);
                    Self::roster_delta(st, &who, me);
                    return Some(Transaction::reply_to(
                        t,
                        vec![reason(0), Field::u16(field::ROSTER_STATE, 3)],
                    ));
                }
                st.rel.insert(
                    (me.into(), who.clone()),
                    Rel {
                        state: RosterState::PendingOut,
                        alias: None,
                    },
                );
                st.rel.insert(
                    (who.clone(), me.into()),
                    Rel {
                        state: RosterState::PendingIn,
                        alias: None,
                    },
                );
                Self::roster_delta(st, me, &who);
                Self::roster_delta(st, &who, me);
                let mut rf = vec![Field::new(field::FRIEND_LOGIN, me)];
                if let Some(n) = s(field::REQUEST_NOTE) {
                    rf.push(Field::new(field::REQUEST_NOTE, n));
                }
                Self::send_to(st, &who, &notify(tx::FRIEND_REQUEST, rf));
                Some(Transaction::reply_to(
                    t,
                    vec![reason(0), Field::u16(field::ROSTER_STATE, 1)],
                ))
            }
            tx::FRIEND_RESPONSE => {
                if Self::rel_state(st, me, &who) != Some(RosterState::PendingIn) {
                    return Some(fail(t, 1, "No request from them."));
                }
                if t.uint(field::ROSTER_STATE) == Some(3) {
                    for (a, b) in [(me, who.as_str()), (who.as_str(), me)] {
                        st.rel.insert(
                            (a.into(), b.into()),
                            Rel {
                                state: RosterState::Accepted,
                                alias: None,
                            },
                        );
                    }
                } else {
                    st.rel.remove(&(me.to_string(), who.clone()));
                    st.rel.remove(&(who.clone(), me.to_string()));
                }
                Self::roster_delta(st, me, &who);
                Self::roster_delta(st, &who, me);
                ok(vec![])
            }
            tx::REMOVE_FRIEND => {
                st.rel.remove(&(me.to_string(), who.clone()));
                st.rel.remove(&(who.clone(), me.to_string()));
                Self::roster_delta(st, me, &who);
                Self::roster_delta(st, &who, me);
                ok(vec![])
            }
            tx::BLOCK_USER => {
                let alias = st
                    .rel
                    .get(&(me.to_string(), who.clone()))
                    .and_then(|r| r.alias.clone());
                st.rel.insert(
                    (me.into(), who.clone()),
                    Rel {
                        state: RosterState::Blocked,
                        alias,
                    },
                );
                st.rel.remove(&(who.clone(), me.to_string()));
                Self::roster_delta(st, me, &who);
                Self::roster_delta(st, &who, me);
                ok(vec![])
            }
            tx::UNBLOCK_USER => {
                st.rel.remove(&(me.to_string(), who.clone()));
                Self::roster_delta(st, me, &who);
                ok(vec![])
            }
            tx::SET_FRIEND_NICKNAME => {
                match st.rel.get_mut(&(me.to_string(), who.clone())) {
                    Some(r) if r.state == RosterState::Accepted => {
                        r.alias = s(field::FRIEND_NICKNAME).filter(|a| !a.trim().is_empty());
                    }
                    _ => return Some(fail(t, 6, "Not friends.")),
                }
                Self::roster_delta(st, me, &who);
                ok(vec![])
            }
            tx::IM_SEND => {
                match Self::rel_state(st, me, &who) {
                    Some(RosterState::Accepted) => {}
                    Some(RosterState::Blocked) => return Some(fail(t, 3, "You blocked them.")),
                    _ => return Some(fail(t, 6, "You must be friends to do that.")),
                }
                let guid = t.bytes(field::MESSAGE_GUID).unwrap_or_default().to_vec();
                let body = t.bytes(field::MESSAGE_BODY).unwrap_or_default().to_vec();
                if body.len() > 4096 {
                    return Some(fail(t, 13, "That message is too long."));
                }
                if !st.seen.insert((guid.clone(), who.clone())) {
                    return ok(vec![]); // duplicate: success, not delivered again
                }
                let deliver = notify(
                    tx::IM_DELIVER,
                    vec![
                        Field::new(field::MESSAGE_GUID, guid),
                        Field::new(field::FRIEND_LOGIN, me),
                        Field::new(field::MESSAGE_BODY, body),
                        Field::new(field::MESSAGE_TIMESTAMP, now().to_be_bytes().to_vec()),
                    ],
                );
                if Self::send_to(st, &who, &deliver) {
                    ok(vec![])
                } else {
                    st.offline.entry(who).or_default().push(deliver);
                    Some(Transaction::reply_to(t, vec![reason(7)]))
                }
            }
            tx::IM_ACK => {
                let fwd = notify(
                    tx::IM_ACK,
                    vec![
                        Field::new(
                            field::MESSAGE_GUID,
                            t.bytes(field::MESSAGE_GUID).unwrap_or_default().to_vec(),
                        ),
                        Field::u16(field::ACK_TYPE, t.uint(field::ACK_TYPE).unwrap_or(1) as u16),
                        Field::new(field::FRIEND_LOGIN, me),
                    ],
                );
                if !Self::send_to(st, &who, &fwd) && t.uint(field::ACK_TYPE) == Some(2) {
                    st.receipts.entry(who).or_default().push(fwd);
                }
                ok(vec![])
            }
            tx::IM_TYPING => {
                if Self::rel_state(st, me, &who) == Some(RosterState::Accepted) {
                    Self::send_to(
                        st,
                        &who,
                        &notify(
                            tx::IM_TYPING,
                            vec![
                                Field::new(field::FRIEND_LOGIN, me),
                                Field::u16(
                                    field::TYPING_STATE,
                                    t.uint(field::TYPING_STATE).unwrap_or(0) as u16,
                                ),
                            ],
                        ),
                    );
                }
                None
            }
            tx::FIND_USER => match st.accounts.get(&who) {
                Some(a) if Self::rel_state(st, &who, me) != Some(RosterState::Blocked) => ok(vec![
                    Field::new(field::FRIEND_LOGIN, who.clone()),
                    Field::new(field::USER_NAME, a.name.clone()),
                ]),
                _ => Some(fail(t, 1, "No such user.")),
            },
            tx::USER_SEARCH => {
                let q = s(field::SEARCH_QUERY).unwrap_or_default().to_lowercase();
                let mut f = vec![reason(0)];
                for (login, a) in st
                    .accounts
                    .iter()
                    .filter(|(l, a)| {
                        a.discoverable
                            && *l != me
                            && (l.to_lowercase().contains(&q) || a.name.to_lowercase().contains(&q))
                    })
                    .take(50)
                {
                    f.push(Field::new(field::FRIEND_LOGIN, login.clone()));
                    f.push(Field::new(field::USER_NAME, a.name.clone()));
                }
                Some(Transaction::reply_to(t, f))
            }
            tx::GET_USER_INFO => {
                let Some(a) = st.accounts.get(&who) else {
                    return Some(fail(t, 1, "No such user."));
                };
                let mut f = vec![
                    Field::new(field::FRIEND_LOGIN, who.clone()),
                    Field::new(field::USER_NAME, a.name.clone()),
                ];
                if who == me || Self::rel_state(st, me, &who) == Some(RosterState::Accepted) {
                    f.insert(0, reason(0));
                    f.extend(a.profile.clone());
                    if let Some((_, h)) = &a.icon {
                        f.push(Field::new(field::BUDDY_ICON_HASH, h.to_vec()));
                    }
                } else {
                    f.insert(0, reason(6));
                }
                Some(Transaction::reply_to(t, f))
            }
            tx::SET_USER_INFO => {
                let profile: Vec<Field> = t
                    .fields
                    .iter()
                    .filter(|f| {
                        (field::PROFILE_NICKNAME..=field::PROFILE_LANGUAGE).contains(&f.id)
                            && !f.data.is_empty()
                    })
                    .cloned()
                    .collect();
                if let Some(a) = st.accounts.get_mut(me) {
                    if let Some(n) = profile.iter().find(|f| f.id == field::PROFILE_NICKNAME) {
                        a.name = String::from_utf8_lossy(&n.data).into_owned();
                    }
                    a.profile = profile;
                }
                drop(guard);
                self.broadcast_presence(me);
                Some(Transaction::reply_to(t, vec![reason(0)]))
            }
            tx::SET_BUDDY_ICON if self.cfg.max_icon_bytes.is_some() => {
                let pic = t.bytes(field::BUDDY_ICON).unwrap_or_default().to_vec();
                let max = self.cfg.max_icon_bytes.unwrap_or(0) as usize;
                if pic.len() > max {
                    return Some(fail(t, 13, "That picture is too big."));
                }
                let new = if pic.is_empty() {
                    None
                } else {
                    match crate::icon::inspect(&pic) {
                        Some(h)
                            if h.width.max(h.height) <= self.cfg.max_icon_dimension
                                && h.frames <= 100 =>
                        {
                            let hash = crate::icon::hash(&pic);
                            Some((pic, hash))
                        }
                        Some(_) => return Some(fail(t, 14, "That picture is too large.")),
                        None => return Some(fail(t, 14, "That isn't a GIF, PNG or JPEG picture.")),
                    }
                };
                let hash = new.as_ref().map(|(_, h)| h.to_vec());
                let acct = st.accounts.get_mut(me)?;
                let changed = acct.icon.as_ref().map(|(_, h)| h.to_vec()) != hash;
                acct.icon = new;
                if changed {
                    // Our other sessions hear about it through 827 itself, whatever our presence.
                    let echo = notify(
                        tx::SET_BUDDY_ICON,
                        hash.iter()
                            .map(|h| Field::new(field::BUDDY_ICON_HASH, h.clone()))
                            .collect(),
                    );
                    for (id, sess) in &st.sessions {
                        if sess.login == me && *id != sid {
                            let _ = sess.tx.send(echo.clone());
                        }
                    }
                }
                // Friends see an Invisible account as signed out: no 809 for them.
                let tell = changed && Self::visible(st, me) != Presence::Offline;
                drop(guard);
                if tell {
                    self.broadcast_presence(me);
                }
                let mut f = vec![reason(0)];
                f.extend(hash.map(|h| Field::new(field::BUDDY_ICON_HASH, h)));
                Some(Transaction::reply_to(t, f))
            }
            tx::GET_BUDDY_ICON if self.cfg.max_icon_bytes.is_some() => {
                let mut f = vec![Field::new(field::FRIEND_LOGIN, who.clone())];
                if who == me || Self::rel_state(st, me, &who) == Some(RosterState::Accepted) {
                    f.insert(0, reason(0));
                    if let Some((pic, h)) = st.accounts.get(&who).and_then(|a| a.icon.as_ref()) {
                        f.push(Field::new(field::BUDDY_ICON_HASH, h.to_vec()));
                        f.push(Field::new(field::BUDDY_ICON, pic.clone()));
                    }
                } else {
                    f.insert(0, reason(6));
                }
                Some(Transaction::reply_to(t, f))
            }
            _ => {
                let mut r = Transaction::reply_to(
                    t,
                    vec![Field::new(
                        field::ERROR,
                        "Not supported by the mock server.",
                    )],
                );
                r.error = 1;
                Some(r)
            }
        }
    }
}
