//! The signed-on session: the roster, conversations, presence and the event
//! pump that turns protocol events into window updates and sounds.

use crate::settings::{self, AwayMessage, Group, Prefs, SavedAccount, Settings};
use crate::windows;
use hotline_im::client::Reply;
use hotline_im::messaging::{hex, new_guid, reason, Profile};
use hotline_im::{
    AckKind, Client, ConnectOptions, Error, Event, Presence, RosterEntry, RosterState, Security,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

type AppState<'a> = State<'a, Mutex<App>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub guid: String,
    /// "in", "out", "auto" (our auto-response) or "system"
    pub dir: String,
    pub from: String,
    pub body: String,
    pub ts: u64,
    /// "sending", "sent", "queued", "delivered", "read", "failed", "unread" (incoming)
    pub state: String,
}

pub struct Live {
    pub client: Client,
    pub opts: ConnectOptions,
    pub account: SavedAccount,
    pub roster: BTreeMap<String, RosterEntry>,
    pub convos: HashMap<String, Vec<Line>>,
    pub presence: Presence,
    pub status: String,
    /// Buddies who already got this away period's auto-response.
    pub auto_answered: HashSet<String>,
    /// Bumped on every (re)connect, so a stale pump stops quietly.
    pub epoch: u64,
    pub signing_off: bool,
    pub connected: bool,
    /// The name our buddies see (the profile's nickname, stored by the server).
    pub my_name: Option<String>,
}

pub struct App {
    pub settings: Settings,
    pub path: PathBuf,
    pub live: Option<Live>,
    epoch: u64,
}

impl App {
    pub fn new(path: PathBuf) -> Self {
        App {
            settings: Settings::load(&path),
            path,
            live: None,
            epoch: 0,
        }
    }

    fn save(&self) {
        self.settings.save(&self.path);
    }

    pub fn save_settings(&self) {
        self.save();
    }

    /// Appends one line to him.log beside the settings (him-dev.log for test runs).
    /// For diagnosing what the server sent; never passwords or message text.
    pub fn log(&self, line: &str) {
        use std::io::Write;
        let path = self.data_dir("him").with_extension("log");
        // Keep it small: start over past 1 MB.
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1 << 20) {
            let _ = std::fs::remove_file(&path);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{} {line}", now());
        }
    }

    /// A folder beside the settings file. Test runs (settings.dev.json) get their
    /// own, so they never touch the user's icons or sounds.
    pub fn data_dir(&self, name: &str) -> PathBuf {
        let dev = self.path.file_name().is_some_and(|f| f == "settings.dev.json");
        let dir = self.path.parent().map(PathBuf::from).unwrap_or_default();
        dir.join(if dev { format!("{name}-dev") } else { name.to_string() })
    }

    pub fn shutdown(&mut self) {
        if let Some(l) = &mut self.live {
            l.signing_off = true;
            l.client.disconnect();
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn err(e: Error) -> String {
    e.to_string()
}

fn live_client(state: &AppState) -> Result<Client, String> {
    state
        .lock()
        .unwrap()
        .live
        .as_ref()
        .filter(|l| l.connected)
        .map(|l| l.client.clone())
        .ok_or_else(|| "You're not signed on.".to_string())
}

// ---------- settings ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    accounts: Vec<AccountView>,
    last_account: Option<String>,
    prefs: Prefs,
    away_messages: Vec<AwayMessage>,
    recent_rooms: Vec<crate::settings::RecentRoom>,
    version: &'static str,
    default_host: &'static str,
    default_port: u16,
    /// Hosts with a "Get a Screen Name" page.
    signup_hosts: Vec<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountView {
    key: String,
    #[serde(flatten)]
    acct: SavedAccount,
    has_password: bool,
}

#[tauri::command]
pub fn get_settings(state: AppState) -> SettingsView {
    let app = state.lock().unwrap();
    SettingsView {
        accounts: app
            .settings
            .accounts
            .iter()
            .map(|a| AccountView {
                key: a.key(),
                acct: a.clone(),
                has_password: a.save_password && settings::keychain_get(&a.key()).is_some(),
            })
            .collect(),
        last_account: app.settings.last_account.clone(),
        prefs: app.settings.prefs.clone(),
        away_messages: app.settings.away_messages.clone(),
        recent_rooms: app.settings.recent_rooms.clone(),
        version: env!("CARGO_PKG_VERSION"),
        default_host: settings::DEFAULT_HOST,
        default_port: settings::DEFAULT_PORT,
        signup_hosts: settings::SIGNUP_PAGES.iter().map(|(h, _)| *h).collect(),
    }
}

#[tauri::command]
pub async fn save_prefs(app: AppHandle, state: AppState<'_>, prefs: Prefs) -> Result<(), String> {
    let (client, discoverable_changed, presence, status) = {
        let mut a = state.lock().unwrap();
        let changed = a.settings.prefs.discoverable != prefs.discoverable;
        a.settings.prefs = prefs.clone();
        a.save();
        let l = a.live.as_ref();
        (
            l.map(|l| l.client.clone()),
            changed,
            l.map(|l| l.presence),
            l.map(|l| l.status.clone()).unwrap_or_default(),
        )
    };
    let _ = app.emit("prefs", &prefs);
    if let (Some(c), true, Some(p)) = (client, discoverable_changed, presence) {
        c.set_presence(p, &status, Some(prefs.discoverable))
            .await
            .map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub fn forget_account(state: AppState, key: String) {
    let mut a = state.lock().unwrap();
    a.settings.accounts.retain(|x| x.key() != key);
    settings::keychain_forget(&key);
    a.save();
}

// ---------- sign on / off ----------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignOnRequest {
    pub login: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub save_password: bool,
    pub auto_login: bool,
}

#[tauri::command]
pub async fn sign_on(app: AppHandle, req: SignOnRequest) -> Result<(), String> {
    do_sign_on(&app, req).await
}

pub async fn do_sign_on(app: &AppHandle, req: SignOnRequest) -> Result<(), String> {
    let state = app.state::<Mutex<App>>();
    let login = req.login.trim().to_string();
    let host = req.host.trim().to_string();
    if login.is_empty() {
        return Err("Please enter a screen name.".into());
    }
    if host.is_empty() {
        return Err("Please choose a server in Setup.".into());
    }
    let account = SavedAccount {
        login: login.clone(),
        host: host.clone(),
        port: req.port,
        security: req.security,
        save_password: req.save_password,
        auto_login: req.auto_login,
    };
    let password = if req.password.is_empty() {
        settings::keychain_get(&account.key()).unwrap_or_default()
    } else {
        req.password.clone()
    };
    let opts = ConnectOptions {
        host,
        port: req.port,
        login: login.clone(),
        password: password.clone(),
        nickname: login.clone(),
        icon: 0,
        security: req.security,
        classic: false,
    };

    let _ = app.emit("signon-step", "Connecting...");
    let session = hotline_im::connect(&opts).await.map_err(err)?;
    let _ = app.emit("signon-step", "Starting services...");
    if !session.client.info.messaging {
        session.client.disconnect();
        return Err(
            "Instant messaging isn't available on this server (or for this account).".into(),
        );
    }

    let epoch = {
        let mut a = state.lock().unwrap();
        a.settings.remember(account.clone());
        a.settings.groups_for(&account.key());
        a.save();
        if req.save_password && !password.is_empty() {
            settings::keychain_set(&account.key(), &password);
        } else if !req.save_password {
            settings::keychain_forget(&account.key());
        }
        if let Some(old) = &mut a.live {
            old.signing_off = true;
            old.client.disconnect();
        }
        a.epoch += 1;
        let epoch = a.epoch;
        a.live = Some(Live {
            client: session.client.clone(),
            opts,
            account,
            roster: BTreeMap::new(),
            convos: HashMap::new(),
            presence: Presence::Online,
            status: String::new(),
            auto_answered: HashSet::new(),
            epoch,
            signing_off: false,
            connected: true,
            my_name: None,
        });
        epoch
    };
    tauri::async_runtime::spawn(pump(app.clone(), session.events, epoch));
    ready(app, &session.client, epoch).await;
    windows::open_buddy_list(app);
    if let Some(w) = app.get_webview_window("signon") {
        let _ = w.close();
    }
    Ok(())
}

/// Presence first, then Get Roster (which also drains the offline backlog).
async fn ready(app: &AppHandle, client: &Client, epoch: u64) {
    let (presence, status, discoverable) = {
        let st = app.state::<Mutex<App>>();
        let a = st.lock().unwrap();
        let l = a.live.as_ref();
        (
            l.map(|l| l.presence).unwrap_or(Presence::Online),
            l.map(|l| l.status.clone()).unwrap_or_default(),
            a.settings.prefs.discoverable,
        )
    };
    let _ = client
        .set_presence(presence, &status, Some(discoverable))
        .await;
    match client.get_roster().await {
        Ok(entries) => {
            let st = app.state::<Mutex<App>>();
            let mut a = st.lock().unwrap();
            if let Some(l) = a.live.as_mut().filter(|l| l.epoch == epoch) {
                // Replace wholesale, keeping nothing stale from before a reconnect.
                l.roster = entries
                    .into_iter()
                    .filter(|e| e.state != RosterState::Removed)
                    .map(|e| (e.login.clone(), e))
                    .collect();
                let with: Vec<String> = l
                    .roster
                    .values()
                    .filter_map(|e| e.icon_hash.as_ref().map(|h| format!("{}={h}", e.login)))
                    .collect();
                let line = format!("roster: {} entries, icon hashes: {with:?}", l.roster.len());
                a.log(&line);
                let Some(l) = a.live.as_mut() else { return };
                let pending: Vec<String> = l
                    .roster
                    .values()
                    .filter(|e| e.state == RosterState::PendingIn)
                    .map(|e| e.login.clone())
                    .collect();
                drop(a);
                for p in pending {
                    let _ = app.emit("request", serde_json::json!({ "login": p }));
                }
            }
        }
        Err(e) => {
            let _ = app.emit("notice", format!("Your Buddy List couldn't be loaded: {e}"));
        }
    }
    load_my_name(app, client).await;
    emit_state(app);
    tauri::async_runtime::spawn(crate::icons::sync(app.clone()));
}

/// The name buddies see us as: our own profile's nickname, else the server's name for us.
async fn load_my_name(app: &AppHandle, client: &Client) {
    let login = client_login(app);
    if let Ok(info) = client.get_info(&login).await {
        let name = info.profile.and_then(|p| p.nickname).or(info.name);
        set_my_name(app, name);
    }
}

fn set_my_name(app: &AppHandle, name: Option<String>) {
    let st = app.state::<Mutex<App>>();
    let mut a = st.lock().unwrap();
    if let Some(l) = a.live.as_mut() {
        l.my_name = name.filter(|n| !n.trim().is_empty());
    }
}

#[tauri::command]
pub async fn sign_off(app: AppHandle, state: AppState<'_>) -> Result<(), String> {
    {
        let mut a = state.lock().unwrap();
        a.shutdown();
        a.live = None;
    }
    app.state::<Mutex<crate::rooms::Rooms>>()
        .lock()
        .unwrap()
        .leave_all();
    for (label, w) in app.webview_windows() {
        if label != "signon" {
            let _ = w.close();
        }
    }
    windows::open(
        &app,
        "signon",
        "signon.html?manual=1",
        "Sign On",
        212.0,
        404.0,
        false,
        true,
    );
    Ok(())
}

async fn pump(app: AppHandle, mut events: tokio::sync::mpsc::UnboundedReceiver<Event>, epoch: u64) {
    while let Some(ev) = events.recv().await {
        let st = app.state::<Mutex<App>>();
        if st.lock().unwrap().live.as_ref().map(|l| l.epoch) != Some(epoch) {
            return;
        }
        match ev {
            Event::RosterEntry { entry } => on_roster_entry(&app, entry),
            Event::Presence { update } => {
                let mut door = None;
                {
                    let mut a = st.lock().unwrap();
                    let Some(l) = a.live.as_mut() else { return };
                    if let Some(e) = l.roster.get_mut(&update.login) {
                        if e.icon_hash != update.icon_hash {
                            let line = format!("809 {}: icon {:?} -> {:?}", update.login, e.icon_hash, update.icon_hash);
                            a.log(&line);
                        }
                        let Some(e) = a.live.as_mut().and_then(|l| l.roster.get_mut(&update.login)) else { return };
                        let was = e.presence;
                        update.apply(e);
                        if (was == Presence::Offline) != (e.presence == Presence::Offline) {
                            door = Some(e.presence != Presence::Offline);
                        }
                    }
                }
                if let Some(open) = door {
                    let _ = app.emit(
                        "door",
                        serde_json::json!({ "login": update.login, "open": open }),
                    );
                }
                emit_state(&app);
            }
            Event::FriendRequest { login, note } => {
                if let Some(n) = &note {
                    REQUEST_NOTES
                        .lock()
                        .unwrap()
                        .insert(login.clone(), n.clone());
                }
                let _ = app.emit(
                    "request",
                    serde_json::json!({ "login": login, "note": note }),
                );
                tauri::async_runtime::spawn(open_request(app.clone(), login));
            }
            Event::Message { message } => on_message(&app, message),
            Event::Ack { guid, login, ack } => {
                let state = if ack == AckKind::Read {
                    "read"
                } else {
                    "delivered"
                };
                let mut a = st.lock().unwrap();
                if let Some(line) = a
                    .live
                    .as_mut()
                    .and_then(|l| l.convos.get_mut(&login))
                    .and_then(|c| c.iter_mut().find(|x| x.guid == guid))
                {
                    // Never step back from read to delivered.
                    if line.state != "read" {
                        line.state = state.into();
                    }
                }
                drop(a);
                let _ = app.emit(
                    "ack",
                    serde_json::json!({ "login": login, "guid": guid, "state": state }),
                );
            }
            Event::Typing { login, typing } => {
                let _ = app.emit(
                    "typing",
                    serde_json::json!({ "login": login, "typing": typing }),
                );
            }
            // Server agreements are accepted automatically and never shown (the user's choice).
            Event::Agreement { .. } => {
                let (client, login) = {
                    let a = st.lock().unwrap();
                    match a.live.as_ref() {
                        Some(l) => (l.client.clone(), l.account.login.clone()),
                        None => return,
                    }
                };
                tauri::async_runtime::spawn(async move {
                    let _ = client.agree(&login, 0).await;
                });
            }
            Event::ServerMessage { text } => {
                let _ = app.emit("notice", text);
            }
            Event::OwnIconChanged { hash } => {
                tauri::async_runtime::spawn(crate::icons::own_icon_changed(app.clone(), hash));
            }
            // Classic chat traffic never reaches a pure messenger session.
            Event::ChatMessage { .. }
            | Event::UserChanged { .. }
            | Event::UserLeft { .. }
            | Event::PrivateMessage { .. } => {}
            Event::Disconnected { reason } => {
                let reconnect = {
                    let mut a = st.lock().unwrap();
                    match a.live.as_mut() {
                        Some(l) if !l.signing_off => {
                            l.connected = false;
                            for e in l.roster.values_mut() {
                                e.presence = Presence::Offline;
                            }
                            true
                        }
                        _ => false,
                    }
                };
                if reconnect {
                    let _ = app.emit(
                        "connection",
                        serde_json::json!({ "state": "lost", "reason": reason }),
                    );
                    emit_state(&app);
                    tauri::async_runtime::spawn(reconnect_loop(app.clone(), epoch));
                }
                return;
            }
        }
    }
}

static REQUEST_NOTES: std::sync::LazyLock<Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(Default::default);

#[tauri::command]
pub fn get_request_note(login: String) -> Option<String> {
    REQUEST_NOTES.lock().unwrap().get(&login).cloned()
}

async fn open_request(app: AppHandle, login: String) {
    windows::open_dialog(app, "request".into(), Some(login)).await;
}

/// Tries again with growing pauses until it works or the user signs off.
fn reconnect_loop(
    app: AppHandle,
    old_epoch: u64,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    // Boxed: the pump spawns this and this spawns a new pump.
    Box::pin(async move {
        let mut wait = 2u64;
        loop {
            tokio::time::sleep(Duration::from_secs(wait)).await;
            let st = app.state::<Mutex<App>>();
            let opts = {
                let a = st.lock().unwrap();
                match a.live.as_ref() {
                    Some(l) if l.epoch == old_epoch && !l.signing_off => l.opts.clone(),
                    _ => return,
                }
            };
            let _ = app.emit("connection", serde_json::json!({ "state": "reconnecting" }));
            match hotline_im::connect(&opts).await {
                Ok(session) => {
                    let epoch = {
                        let mut a = st.lock().unwrap();
                        a.epoch += 1;
                        let epoch = a.epoch;
                        match a.live.as_mut() {
                            Some(l) if l.epoch == old_epoch && !l.signing_off => {
                                l.client = session.client.clone();
                                l.epoch = epoch;
                                l.connected = true;
                            }
                            _ => {
                                session.client.disconnect();
                                return;
                            }
                        }
                        epoch
                    };
                    tauri::async_runtime::spawn(pump(app.clone(), session.events, epoch));
                    ready(&app, &session.client, epoch).await;
                    let _ = app.emit("connection", serde_json::json!({ "state": "online" }));
                    return;
                }
                Err(Error::LoginFailed(reason)) => {
                    let _ = app.emit(
                        "connection",
                        serde_json::json!({ "state": "failed", "reason": reason }),
                    );
                    return;
                }
                Err(e) => {
                    let _ = app.emit(
                        "connection",
                        serde_json::json!({ "state": "lost", "reason": e.to_string() }),
                    );
                    wait = (wait * 2).min(60);
                }
            }
        }
    })
}

fn on_roster_entry(app: &AppHandle, entry: RosterEntry) {
    let st = app.state::<Mutex<App>>();
    let mut door = None;
    {
        let mut a = st.lock().unwrap();
        let key = a.live.as_ref().map(|l| l.account.key());
        let Some(l) = a.live.as_mut() else { return };
        let login = entry.login.clone();
        if entry.state == RosterState::Removed {
            l.roster.remove(&login);
            if let Some(k) = key {
                for g in a.settings.groups_for(&k) {
                    g.members.retain(|m| *m != login);
                }
                a.save();
            }
        } else {
            let was = l
                .roster
                .get(&login)
                .map(|e| e.presence)
                .unwrap_or(Presence::Offline);
            if entry.state == RosterState::Accepted
                && (was == Presence::Offline) != (entry.presence == Presence::Offline)
            {
                door = Some(entry.presence != Presence::Offline);
            }
            l.roster.insert(login, entry.clone());
        }
    }
    if let Some(open) = door {
        let _ = app.emit(
            "door",
            serde_json::json!({ "login": entry.login, "open": open }),
        );
    }
    emit_state(app);
}

fn on_message(app: &AppHandle, m: hotline_im::IncomingMessage) {
    let st = app.state::<Mutex<App>>();
    let (auto, client) = {
        let mut a = st.lock().unwrap();
        let auto_on = a.settings.prefs.auto_response;
        let Some(l) = a.live.as_mut() else { return };
        let convo = l.convos.entry(m.from.clone()).or_default();
        if convo.iter().any(|x| x.guid == m.guid) {
            return; // the server may redeliver across a reconnect
        }
        let line = Line {
            guid: m.guid.clone(),
            dir: "in".into(),
            from: m.from.clone(),
            body: m.body.clone(),
            ts: if m.timestamp > 0 { m.timestamp } else { now() },
            state: "unread".into(),
        };
        convo.push(line.clone());
        let _ = app.emit("im", serde_json::json!({ "login": m.from, "line": line }));
        let auto = (auto_on
            && l.presence == Presence::Away
            && !l.status.trim().is_empty()
            && l.auto_answered.insert(m.from.clone()))
        .then(|| l.status.clone());
        (auto, l.client.clone())
    };
    windows::open_im_window(app, &m.from, false);
    if let Some(text) = auto {
        let app = app.clone();
        let to = m.from.clone();
        tauri::async_runtime::spawn(async move {
            let _ = send_line(
                &app,
                &client,
                &to,
                &format!("Auto-response: {text}"),
                "auto",
            )
            .await;
        });
    }
}

// ---------- the Buddy List's view ----------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuddyView {
    #[serde(flatten)]
    entry: RosterEntry,
    name: String,
    unread: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateView {
    signed_on: bool,
    connected: bool,
    login: String,
    server: String,
    server_name: Option<String>,
    transport: String,
    encrypted: bool,
    warnings: Vec<String>,
    presence: Presence,
    status: String,
    buddies: Vec<BuddyView>,
    groups: Vec<Group>,
    max_message_bytes: u32,
    /// What buddies see us as, when it isn't just the screen name.
    my_name: Option<String>,
}

fn state_view(a: &mut App) -> StateView {
    let Some(l) = a.live.as_ref() else {
        return StateView {
            signed_on: false,
            connected: false,
            login: String::new(),
            server: String::new(),
            server_name: None,
            transport: String::new(),
            encrypted: false,
            warnings: vec![],
            presence: Presence::Offline,
            status: String::new(),
            buddies: vec![],
            groups: vec![],
            max_message_bytes: 4096,
            my_name: None,
        };
    };
    let key = l.account.key();
    let buddies: Vec<BuddyView> = l
        .roster
        .values()
        .map(|e| BuddyView {
            name: e.shown_name().to_string(),
            unread: l
                .convos
                .get(&e.login)
                .map(|c| c.iter().filter(|x| x.state == "unread").count())
                .unwrap_or(0),
            entry: e.clone(),
        })
        .collect();
    let info = l.client.info.clone();
    let view = StateView {
        signed_on: true,
        connected: l.connected,
        login: l.account.login.clone(),
        server: format!("{}:{}", l.account.host, l.account.port),
        server_name: info.server_name.clone(),
        transport: info.transport.clone(),
        encrypted: info.encrypted,
        warnings: info.warnings.clone(),
        presence: l.presence,
        status: l.status.clone(),
        buddies,
        groups: vec![],
        max_message_bytes: info.limits.max_message_bytes,
        my_name: l.my_name.clone(),
    };
    let groups = a.settings.groups_for(&key).clone();
    StateView { groups, ..view }
}

fn emit_state(app: &AppHandle) {
    let st = app.state::<Mutex<App>>();
    let v = state_view(&mut st.lock().unwrap());
    let _ = app.emit("state", v);
}

#[tauri::command]
pub fn get_state(state: AppState) -> StateView {
    state_view(&mut state.lock().unwrap())
}

// ---------- conversations ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationView {
    login: String,
    name: String,
    me: String,
    buddy: Option<RosterEntry>,
    lines: Vec<Line>,
    max_message_bytes: u32,
    prefs: Prefs,
}

#[tauri::command]
pub fn get_conversation(state: AppState, login: String) -> ConversationView {
    let a = state.lock().unwrap();
    let l = a.live.as_ref();
    let buddy = l.and_then(|l| l.roster.get(&login)).cloned();
    ConversationView {
        name: buddy
            .as_ref()
            .map(|b| b.shown_name().to_string())
            .unwrap_or_else(|| login.clone()),
        // Our lines carry the name buddies see us as (My HIM > Display Name).
        me: l
            .map(|l| l.my_name.clone().unwrap_or_else(|| l.account.login.clone()))
            .unwrap_or_default(),
        lines: l
            .and_then(|l| l.convos.get(&login))
            .cloned()
            .unwrap_or_default(),
        max_message_bytes: l
            .map(|l| l.client.info.limits.max_message_bytes)
            .unwrap_or(4096),
        prefs: a.settings.prefs.clone(),
        buddy,
        login,
    }
}

async fn send_line(
    app: &AppHandle,
    client: &Client,
    to: &str,
    body: &str,
    dir: &str,
) -> Result<Line, String> {
    let guid = new_guid();
    let me = client_login(app);
    let mut line = Line {
        guid: hex(&guid),
        dir: dir.into(),
        from: me,
        body: body.to_string(),
        ts: now(),
        state: "sending".into(),
    };
    {
        let st = app.state::<Mutex<App>>();
        let mut a = st.lock().unwrap();
        if let Some(l) = a.live.as_mut() {
            l.convos
                .entry(to.to_string())
                .or_default()
                .push(line.clone());
        }
    }
    let _ = app.emit("im", serde_json::json!({ "login": to, "line": line }));
    // One retry with the same GUID covers a timeout; the server de-duplicates.
    let mut result = client.send_im(to, &guid, body).await;
    if matches!(result, Err(Error::Timeout)) {
        result = client.send_im(to, &guid, body).await;
    }
    let (state, error) = match &result {
        Ok(code) if *code == reason::OFFLINE_QUEUED => ("queued", None),
        Ok(_) => ("sent", None),
        Err(e) => ("failed", Some(e.to_string())),
    };
    line.state = state.into();
    {
        let st = app.state::<Mutex<App>>();
        let mut a = st.lock().unwrap();
        if let Some(x) = a
            .live
            .as_mut()
            .and_then(|l| l.convos.get_mut(to))
            .and_then(|c| c.iter_mut().find(|x| x.guid == line.guid))
        {
            // A receipt may already have beaten the reply here.
            if x.state == "sending" {
                x.state = state.into();
            }
            line.state = x.state.clone();
        }
    }
    let _ = app.emit(
        "ack",
        serde_json::json!({ "login": to, "guid": line.guid, "state": line.state, "error": error }),
    );
    match error {
        Some(e) => Err(e),
        None => Ok(line),
    }
}

fn client_login(app: &AppHandle) -> String {
    let st = app.state::<Mutex<App>>();
    let a = st.lock().unwrap();
    a.live
        .as_ref()
        .map(|l| l.account.login.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn send_im(app: AppHandle, login: String, body: String) -> Result<Line, String> {
    let client = live_client(&app.state::<Mutex<App>>())?;
    if body.trim().is_empty() {
        return Err("Type a message first.".into());
    }
    client.typing(&login, false);
    send_line(&app, &client, &login, &body, "out").await
}

/// Read receipts for everything shown in an IM window.
#[tauri::command]
pub fn mark_read(app: AppHandle, state: AppState, login: String) {
    let mut a = state.lock().unwrap();
    let Some(l) = a.live.as_mut() else { return };
    let client = l.client.clone();
    let mut any = false;
    if let Some(c) = l.convos.get_mut(&login) {
        for x in c.iter_mut().filter(|x| x.state == "unread") {
            x.state = "read".into();
            client.ack_nowait(&x.guid, &login, AckKind::Read);
            any = true;
        }
    }
    drop(a);
    if any {
        emit_state(&app);
    }
}

#[tauri::command]
pub fn typing(state: AppState, login: String, typing: bool) {
    if let Ok(c) = live_client(&state) {
        c.typing(&login, typing);
    }
}

// ---------- presence ----------

/// `message` Some = go away with it; None = come back.
#[tauri::command]
pub async fn set_away(
    app: AppHandle,
    state: AppState<'_>,
    message: Option<String>,
) -> Result<(), String> {
    let client = live_client(&state)?;
    let (p, text) = match &message {
        Some(m) => (Presence::Away, m.clone()),
        None => (Presence::Online, String::new()),
    };
    client.set_presence(p, &text, None).await.map_err(err)?;
    {
        let mut a = state.lock().unwrap();
        if let Some(l) = a.live.as_mut() {
            l.presence = p;
            l.status = text;
            l.auto_answered.clear();
        }
    }
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn save_away_messages(state: AppState, messages: Vec<AwayMessage>) {
    let mut a = state.lock().unwrap();
    a.settings.away_messages = messages;
    a.save();
}

// ---------- buddies ----------

fn reply_ok(r: Result<Reply, Error>) -> Result<(), String> {
    r.map(|_| ()).map_err(err)
}

#[tauri::command]
pub async fn add_buddy(
    app: AppHandle,
    state: AppState<'_>,
    login: String,
    note: String,
    group: String,
) -> Result<(), String> {
    let client = live_client(&state)?;
    let login = login.trim().to_string();
    if login.is_empty() {
        return Err("Enter the buddy's screen name.".into());
    }
    // Check the name first so a typo reads as "no such user", not a vague refusal.
    let (found, _) = client.find_user(&login).await.map_err(err)?;
    client.add_friend(&found, &note).await.map_err(err)?;
    {
        let mut a = state.lock().unwrap();
        if let Some(key) = a.live.as_ref().map(|l| l.account.key()) {
            let groups = a.settings.groups_for(&key);
            for g in groups.iter_mut() {
                g.members.retain(|m| *m != found);
            }
            let target = groups.iter().position(|g| g.name == group).unwrap_or(0);
            if let Some(g) = groups.get_mut(target) {
                g.members.push(found.clone());
            }
            a.save();
        }
    }
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub async fn respond_request(
    state: AppState<'_>,
    login: String,
    accept: bool,
) -> Result<(), String> {
    reply_ok(live_client(&state)?.respond(&login, accept).await)
}

#[tauri::command]
pub async fn remove_buddy(state: AppState<'_>, login: String) -> Result<(), String> {
    reply_ok(live_client(&state)?.remove_friend(&login).await)
}

#[tauri::command]
pub async fn block_buddy(state: AppState<'_>, login: String) -> Result<(), String> {
    reply_ok(live_client(&state)?.block(&login).await)
}

#[tauri::command]
pub async fn unblock_buddy(state: AppState<'_>, login: String) -> Result<(), String> {
    reply_ok(live_client(&state)?.unblock(&login).await)
}

#[tauri::command]
pub async fn set_alias(state: AppState<'_>, login: String, alias: String) -> Result<(), String> {
    reply_ok(live_client(&state)?.set_alias(&login, alias.trim()).await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoView {
    login: String,
    name: Option<String>,
    profile: Option<Profile>,
    buddy: Option<RosterEntry>,
    is_me: bool,
}

#[tauri::command]
pub async fn get_info(state: AppState<'_>, login: String) -> Result<InfoView, String> {
    let client = live_client(&state)?;
    let info = client.get_info(&login).await.map_err(err)?;
    let a = state.lock().unwrap();
    let l = a.live.as_ref();
    Ok(InfoView {
        buddy: l.and_then(|l| l.roster.get(&login)).cloned(),
        is_me: l.is_some_and(|l| l.account.login == login),
        login,
        name: info.name,
        profile: info.profile,
    })
}

#[tauri::command]
pub async fn set_profile(app: AppHandle, state: AppState<'_>, profile: Profile) -> Result<(), String> {
    reply_ok(live_client(&state)?.set_info(&profile).await)?;
    set_my_name(&app, profile.nickname.clone());
    emit_state(&app);
    Ok(())
}

/// Changes only the name buddies see. Set User Info replaces the whole profile, so
/// the rest of it is read first and sent back unchanged.
#[tauri::command]
pub async fn set_display_name(app: AppHandle, state: AppState<'_>, name: String) -> Result<(), String> {
    let client = live_client(&state)?;
    let login = client_login(&app);
    let mut profile = client.get_info(&login).await.map_err(err)?.profile.unwrap_or_default();
    let name = name.trim();
    profile.nickname = (!name.is_empty()).then(|| name.to_string());
    client.set_info(&profile).await.map_err(err)?;
    // The server may fall back to another name when the nickname is cleared: ask it.
    load_my_name(&app, &client).await;
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub async fn search_users(
    state: AppState<'_>,
    query: String,
) -> Result<Vec<(String, Option<String>)>, String> {
    live_client(&state)?.search(query.trim()).await.map_err(err)
}

// ---------- groups (kept locally) ----------

fn with_groups(
    app: &AppHandle,
    state: &AppState,
    f: impl FnOnce(&mut Vec<Group>) -> Result<(), String>,
) -> Result<(), String> {
    {
        let mut a = state.lock().unwrap();
        let key = a
            .live
            .as_ref()
            .map(|l| l.account.key())
            .ok_or("You're not signed on.")?;
        f(a.settings.groups_for(&key))?;
        a.save();
    }
    emit_state(app);
    Ok(())
}

#[tauri::command]
pub fn add_group(app: AppHandle, state: AppState, name: String) -> Result<(), String> {
    let name = name.trim().to_string();
    with_groups(&app, &state, |g| {
        if name.is_empty() || g.iter().any(|x| x.name.eq_ignore_ascii_case(&name)) {
            return Err("Choose a new group name.".into());
        }
        g.push(Group {
            name,
            members: vec![],
        });
        Ok(())
    })
}

#[tauri::command]
pub fn rename_group(
    app: AppHandle,
    state: AppState,
    old: String,
    name: String,
) -> Result<(), String> {
    let name = name.trim().to_string();
    with_groups(&app, &state, |g| {
        if name.is_empty()
            || g.iter()
                .any(|x| x.name.eq_ignore_ascii_case(&name) && x.name != old)
        {
            return Err("Choose a new group name.".into());
        }
        if let Some(x) = g.iter_mut().find(|x| x.name == old) {
            x.name = name;
        }
        Ok(())
    })
}

/// Its buddies move to the first remaining group.
#[tauri::command]
pub fn delete_group(app: AppHandle, state: AppState, name: String) -> Result<(), String> {
    with_groups(&app, &state, |g| {
        if g.len() <= 1 {
            return Err("You need at least one group.".into());
        }
        let Some(i) = g.iter().position(|x| x.name == name) else {
            return Ok(());
        };
        let moved = g.remove(i).members;
        g[0].members.extend(moved);
        Ok(())
    })
}

#[tauri::command]
pub fn move_buddy(
    app: AppHandle,
    state: AppState,
    login: String,
    group: String,
) -> Result<(), String> {
    with_groups(&app, &state, |g| {
        for x in g.iter_mut() {
            x.members.retain(|m| *m != login);
        }
        if let Some(x) = g.iter_mut().find(|x| x.name == group) {
            x.members.push(login);
        }
        Ok(())
    })
}
