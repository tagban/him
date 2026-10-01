//! Chat rooms: any Hotline server that lets guests in is a room, the way AIM
//! had its chat rooms. Each room is its own classic guest session (no
//! messaging bits), so people there see us as an ordinary Hotline user.

use crate::session::App;
use crate::windows;
use hotline_im::tracker::{self, ListedServer};
use hotline_im::{ChatUser, Client, ConnectOptions, Event, Security};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

const TRACKER_TIMEOUT: Duration = Duration::from_secs(12);
const LIST_FRESH: Duration = Duration::from_secs(300);
const MAX_LINES: usize = 1000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomLine {
    /// "chat", "emote", "join", "leave", "system", "pm"
    pub kind: String,
    pub name: String,
    pub text: String,
    pub ts: u64,
    pub mine: bool,
}

pub struct Room {
    client: Option<Client>,
    pub host: String,
    pub port: u16,
    pub title: String,
    nick: String,
    users: BTreeMap<u16, ChatUser>,
    lines: Vec<RoomLine>,
    /// "connecting", "in", "left"
    state: String,
    error: Option<String>,
    epoch: u64,
}

#[derive(Default)]
pub struct Rooms {
    map: HashMap<String, Room>,
    epoch: u64,
    listing: Option<(Instant, Vec<ListedServer>, Vec<String>)>,
}

impl Rooms {
    /// Leaves every room (signing off, quitting).
    pub fn leave_all(&mut self) {
        for r in self.map.values_mut() {
            if let Some(c) = r.client.take() {
                c.disconnect();
            }
        }
        self.map.clear();
    }
}

pub fn room_id(host: &str, port: u16) -> String {
    format!("{host}:{port}")
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

type RoomsState<'a> = State<'a, Mutex<Rooms>>;

// ---------- the room list ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingView {
    servers: Vec<ListedServer>,
    failed: Vec<String>,
    trackers: Vec<String>,
    age_secs: u64,
}

#[tauri::command]
pub async fn list_rooms(app: AppHandle, refresh: bool) -> ListingView {
    let (trackers, hidden) = {
        let st = app.state::<Mutex<App>>();
        let a = st.lock().unwrap();
        let hidden = crate::settings::HIDDEN_ROOMS
            .iter()
            .map(|h| h.to_string())
            .chain(a.settings.hidden_rooms.iter().cloned())
            .map(|h| h.to_lowercase())
            .filter(|h| !h.trim().is_empty())
            .collect::<Vec<_>>();
        (a.settings.trackers.clone(), hidden)
    };
    let shown = |list: &[ListedServer]| -> Vec<ListedServer> {
        list.iter()
            .filter(|s| {
                let name = s.name.to_lowercase();
                !hidden.iter().any(|h| name.contains(h.as_str()))
            })
            .cloned()
            .collect()
    };
    {
        let rooms = app.state::<Mutex<Rooms>>();
        let r = rooms.lock().unwrap();
        if let Some((at, list, failed)) = &r.listing {
            if !refresh && at.elapsed() < LIST_FRESH {
                return ListingView {
                    servers: shown(list),
                    failed: failed.clone(),
                    trackers,
                    age_secs: at.elapsed().as_secs(),
                };
            }
        }
    }
    let (list, failed) = tracker::query_all(&trackers, TRACKER_TIMEOUT).await;
    let rooms = app.state::<Mutex<Rooms>>();
    rooms.lock().unwrap().listing = Some((Instant::now(), list.clone(), failed.clone()));
    ListingView {
        servers: shown(&list),
        failed,
        trackers,
        age_secs: 0,
    }
}

// ---------- joining ----------

/// The name we chat under: the IM screen name when signed on, else the last one used.
fn nickname(app: &AppHandle) -> String {
    let st = app.state::<Mutex<App>>();
    let a = st.lock().unwrap();
    a.live
        .as_ref()
        .map(|l| l.account.login.clone())
        .or_else(|| a.settings.chat_nick.clone())
        .or_else(|| a.settings.accounts.first().map(|x| x.login.clone()))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "HIM Guest".into())
}

/// "Chat rooms only" from the Sign On window: remember the name, open the room list.
#[tauri::command]
pub async fn chat_only(app: AppHandle, nick: String) -> Result<(), String> {
    let nick = nick.trim().to_string();
    if nick.is_empty() {
        return Err("Type the name you want to chat as in Screen Name first.".into());
    }
    {
        let st = app.state::<Mutex<App>>();
        let mut a = st.lock().unwrap();
        a.settings.chat_nick = Some(nick);
        a.save_settings();
    }
    windows::open_dialog(app, "chatrooms".into(), None).await;
    Ok(())
}

#[tauri::command]
pub async fn join_room(
    app: AppHandle,
    host: String,
    port: u16,
    name: Option<String>,
) -> Result<String, String> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err("Enter a server address.".into());
    }
    let id = room_id(&host, port);
    let title = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
        format!(
            "{host}{}",
            if port == 5500 {
                String::new()
            } else {
                format!(":{port}")
            }
        )
    });
    let fresh = {
        let rooms = app.state::<Mutex<Rooms>>();
        let mut r = rooms.lock().unwrap();
        let active = r.map.get(&id).is_some_and(|x| x.state != "left");
        if !active {
            r.epoch += 1;
            let epoch = r.epoch;
            r.map.insert(
                id.clone(),
                Room {
                    client: None,
                    host: host.clone(),
                    port,
                    title: title.clone(),
                    nick: nickname(&app),
                    users: BTreeMap::new(),
                    lines: vec![],
                    state: "connecting".into(),
                    error: None,
                    epoch,
                },
            );
        }
        !active
    };
    {
        let st = app.state::<Mutex<App>>();
        let mut a = st.lock().unwrap();
        a.settings
            .recent_rooms
            .retain(|x| !(x.host == host && x.port == port));
        a.settings.recent_rooms.insert(
            0,
            crate::settings::RecentRoom {
                host: host.clone(),
                port,
                name: title.clone(),
            },
        );
        a.settings.recent_rooms.truncate(12);
        a.save_settings();
    }
    windows::open(
        &app,
        &format!("room-{id}"),
        &format!("room.html?id={id}"),
        "Chat Room",
        540.0,
        380.0,
        true,
        true,
    );
    if fresh {
        tauri::async_runtime::spawn(connect_room(app.clone(), id.clone()));
    }
    Ok(id)
}

async fn connect_room(app: AppHandle, id: String) {
    let (opts, epoch) = {
        let rooms = app.state::<Mutex<Rooms>>();
        let r = rooms.lock().unwrap();
        let Some(room) = r.map.get(&id) else { return };
        (
            ConnectOptions {
                host: room.host.clone(),
                port: room.port,
                login: String::new(),
                password: String::new(),
                nickname: room.nick.clone(),
                // No icon: rooms show names only, like AIM's.
                icon: 0,
                security: Security::Auto,
                classic: true,
                media: false,
            },
            room.epoch,
        )
    };
    let result = hotline_im::connect(&opts).await;
    let session = match result {
        Ok(s) => s,
        Err(e) => {
            update(&app, &id, epoch, |room| {
                room.state = "left".into();
                room.error = Some(e.to_string());
                push(room, "system", "", &format!("Couldn't join: {e}"), false);
            });
            return;
        }
    };
    let client = session.client.clone();
    // Remember the server's own name for the Recent list.
    if let Some(n) = client
        .info
        .server_name
        .as_ref()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
    {
        let st = app.state::<Mutex<App>>();
        let mut a = st.lock().unwrap();
        if let Some(r) = a
            .settings
            .recent_rooms
            .iter_mut()
            .find(|r| r.host == opts.host && r.port == opts.port)
        {
            r.name = n;
        }
        a.save_settings();
    }
    let ok = update(&app, &id, epoch, |room| {
        room.client = Some(client.clone());
        if let Some(n) = &client.info.server_name {
            if !n.trim().is_empty() {
                room.title = n.trim().to_string();
            }
        }
        room.state = "in".into();
        push(
            room,
            "system",
            "",
            &format!("You have entered {}.", room.title),
            false,
        );
    });
    if !ok {
        client.disconnect();
        return;
    }
    tauri::async_runtime::spawn(pump(app.clone(), id.clone(), epoch, session.events));
    load_users(&app, &id, epoch, &client).await;
}

async fn load_users(app: &AppHandle, id: &str, epoch: u64, client: &Client) {
    match client.get_users().await {
        Ok(users) => {
            update(app, id, epoch, |room| {
                room.users = users.into_iter().map(|u| (u.id, u)).collect();
            });
        }
        Err(e) => {
            update(app, id, epoch, |room| {
                push(
                    room,
                    "system",
                    "",
                    &format!("The server won't show who's here ({e})."),
                    false,
                )
            });
        }
    }
}

async fn pump(
    app: AppHandle,
    id: String,
    epoch: u64,
    mut events: tokio::sync::mpsc::UnboundedReceiver<Event>,
) {
    while let Some(ev) = events.recv().await {
        let alive = match ev {
            // Agreements are accepted on the user's behalf and never shown (the user's choice).
            Event::Agreement { .. } => {
                let (client, nick) = {
                    let rooms = app.state::<Mutex<Rooms>>();
                    let r = rooms.lock().unwrap();
                    match r.map.get(&id).filter(|x| x.epoch == epoch) {
                        Some(room) => (room.client.clone(), room.nick.clone()),
                        None => return,
                    }
                };
                if let Some(c) = client {
                    c.agree_nowait(&nick, 0);
                    // Some servers only list users once the agreement is accepted.
                    load_users(&app, &id, epoch, &c).await;
                }
                true
            }
            Event::ChatMessage {
                chat_id: None,
                text,
                ..
            } => update(&app, &id, epoch, |room| {
                for raw in text.split(['\r', '\n']).filter(|l| !l.trim().is_empty()) {
                    let (kind, name, body) = parse_chat(raw);
                    let mine = name == room.nick
                        || (kind == "emote" && body.starts_with(&format!("{} ", room.nick)));
                    push(room, kind, &name, &body, mine);
                }
            }),
            Event::ChatMessage { .. } => true, // private chats aren't part of a room
            Event::UserChanged { user } => update(&app, &id, epoch, |room| {
                match room.users.get(&user.id) {
                    None => {
                        let n = user.name.clone();
                        push(
                            room,
                            "join",
                            &n,
                            &format!("{n} has entered the room."),
                            false,
                        );
                    }
                    Some(old) if old.name != user.name && !old.name.is_empty() => {
                        let (a, b) = (old.name.clone(), user.name.clone());
                        push(
                            room,
                            "system",
                            "",
                            &format!("{a} is now known as {b}."),
                            false,
                        );
                    }
                    _ => {}
                }
                room.users.insert(user.id, user.clone());
            }),
            Event::UserLeft { id: uid } => update(&app, &id, epoch, |room| {
                if let Some(u) = room.users.remove(&uid) {
                    push(
                        room,
                        "leave",
                        &u.name,
                        &format!("{} has left the room.", u.name),
                        false,
                    );
                }
            }),
            Event::PrivateMessage {
                from_name, text, ..
            } => update(&app, &id, epoch, |room| {
                push(room, "pm", &from_name, &text, false);
            }),
            Event::ServerMessage { text } => update(&app, &id, epoch, |room| {
                push(room, "system", "", &text, false)
            }),
            Event::Disconnected { reason } => {
                update(&app, &id, epoch, |room| {
                    if room.state != "left" {
                        room.state = "left".into();
                        room.client = None;
                        room.users.clear();
                        push(
                            room,
                            "system",
                            "",
                            &format!("You left the room: {reason}"),
                            false,
                        );
                    }
                });
                return;
            }
            _ => true,
        };
        if !alive {
            return;
        }
    }
}

/// Classic chat lines: `"      name:  text"`, or `" *** name does something"` for an emote.
fn parse_chat(raw: &str) -> (&'static str, String, String) {
    let line = raw.trim_start();
    if let Some(rest) = line.strip_prefix("***") {
        return ("emote", String::new(), rest.trim().to_string());
    }
    if let Some(i) = line.find(":  ") {
        let name = line[..i].trim();
        if !name.is_empty() && name.chars().count() <= 64 {
            return ("chat", name.to_string(), line[i + 3..].to_string());
        }
    }
    ("system", String::new(), line.trim_end().to_string())
}

fn push(room: &mut Room, kind: &str, name: &str, text: &str, mine: bool) {
    room.lines.push(RoomLine {
        kind: kind.into(),
        name: name.into(),
        text: text.into(),
        ts: now(),
        mine,
    });
    if room.lines.len() > MAX_LINES {
        let cut = room.lines.len() - MAX_LINES;
        room.lines.drain(..cut);
    }
}

/// Applies a change to a live room and sends the window its new state. False once the
/// room has been left or replaced, so a stale task stops.
fn update(app: &AppHandle, id: &str, epoch: u64, f: impl FnOnce(&mut Room)) -> bool {
    let view = {
        let rooms = app.state::<Mutex<Rooms>>();
        let mut r = rooms.lock().unwrap();
        let Some(room) = r.map.get_mut(id).filter(|x| x.epoch == epoch) else {
            return false;
        };
        let before = room.lines.len();
        f(room);
        let new_lines = room.lines[before.min(room.lines.len())..].to_vec();
        (view_of(id, room, false), new_lines)
    };
    let _ = app.emit(
        "room",
        serde_json::json!({ "id": id, "room": view.0, "lines": view.1 }),
    );
    true
}

// ---------- the room window's view ----------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomView {
    id: String,
    title: String,
    host: String,
    port: u16,
    nick: String,
    state: String,
    error: Option<String>,
    users: Vec<ChatUser>,
    lines: Vec<RoomLine>,
    transport: String,
    encrypted: bool,
}

fn view_of(id: &str, room: &Room, with_lines: bool) -> RoomView {
    let mut users: Vec<ChatUser> = room.users.values().cloned().collect();
    users.sort_by_key(|u| u.name.to_lowercase());
    RoomView {
        id: id.into(),
        title: room.title.clone(),
        host: room.host.clone(),
        port: room.port,
        nick: room.nick.clone(),
        state: room.state.clone(),
        error: room.error.clone(),
        users,
        lines: if with_lines {
            room.lines.clone()
        } else {
            vec![]
        },
        transport: room
            .client
            .as_ref()
            .map(|c| c.info.transport.clone())
            .unwrap_or_default(),
        encrypted: room.client.as_ref().is_some_and(|c| c.info.encrypted),
    }
}

#[tauri::command]
pub fn get_room(rooms: RoomsState, id: String) -> Option<RoomView> {
    let r = rooms.lock().unwrap();
    r.map.get(&id).map(|room| view_of(&id, room, true))
}

#[tauri::command]
pub fn room_send(rooms: RoomsState, id: String, text: String) -> Result<(), String> {
    let r = rooms.lock().unwrap();
    let room = r.map.get(&id).ok_or("That room is closed.")?;
    let client = room
        .client
        .as_ref()
        .filter(|_| room.state == "in")
        .ok_or("You're not in the room right now.")?;
    let text = text.trim_end();
    if text.trim().is_empty() {
        return Ok(());
    }
    match text.strip_prefix("/me ") {
        Some(action) => client.send_chat(action, true),
        None => client.send_chat(text, false),
    }
    Ok(())
}

#[tauri::command]
pub fn leave_room(rooms: RoomsState, id: String) {
    let mut r = rooms.lock().unwrap();
    if let Some(mut room) = r.map.remove(&id) {
        if let Some(c) = room.client.take() {
            c.disconnect();
        }
    }
}

/// Rejoin after being dropped.
#[tauri::command]
pub async fn rejoin_room(app: AppHandle, id: String) -> Result<(), String> {
    let (host, port, title) = {
        let rooms = app.state::<Mutex<Rooms>>();
        let mut r = rooms.lock().unwrap();
        let room = r.map.remove(&id).ok_or("That room is closed.")?;
        (room.host, room.port, room.title)
    };
    join_room(app, host, port, Some(title)).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::parse_chat;

    #[test]
    fn classic_chat_lines() {
        assert_eq!(
            parse_chat("        Pat:  hey everybody"),
            ("chat", "Pat".into(), "hey everybody".into())
        );
        assert_eq!(
            parse_chat("Sam:  time: 5pm"),
            ("chat", "Sam".into(), "time: 5pm".into())
        );
        assert_eq!(
            parse_chat(" *** Pat waves"),
            ("emote", String::new(), "Pat waves".into())
        );
        assert_eq!(
            parse_chat("The server will restart soon"),
            (
                "system",
                String::new(),
                "The server will restart soon".into()
            )
        );
    }
}
