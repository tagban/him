//! Files between buddies (guide §14): offering, answering, and moving them through the
//! server's relay. Each transfer is a line in the conversation, updated as it goes.

use crate::session::{App, Line};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// A file on a conversation line.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub guid: String,
    pub name: String,
    pub size: u64,
    /// Ours to send, or where theirs was saved.
    pub path: Option<String>,
    /// "offered" (ours, waiting), "incoming" (theirs, waiting for us), "starting",
    /// "moving", "done", "declined", "failed"
    pub state: String,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub error: Option<String>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Downloads/HIM: where files from buddies go unless Setup says otherwise.
pub fn default_received_folder(app: &AppHandle) -> PathBuf {
    app.path().download_dir().unwrap_or_else(|_| std::env::temp_dir()).join("HIM")
}

/// Where files from buddies go: the folder chosen in Setup, else Downloads/HIM.
pub fn received_folder(app: &AppHandle) -> PathBuf {
    #[cfg(debug_assertions)]
    if let Ok(d) = std::env::var("HIM_RECEIVED_DIR") {
        return PathBuf::from(d); // test runs
    }
    let chosen = {
        let st = app.state::<Mutex<App>>();
        let a = st.lock().unwrap();
        a.settings.prefs.download_dir.clone()
    };
    // A chosen folder that's gone (an unplugged drive, say) falls back to the default.
    let dir = chosen.map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(|| default_received_folder(app));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Setup's "Received files" line: the folder in use, and whether it's the default.
#[tauri::command]
pub fn received_folder_info(app: AppHandle) -> (String, bool) {
    let dir = received_folder(&app);
    let default = dir == default_received_folder(&app);
    (shown_path(&app, &dir), default)
}

/// A folder as people read it: "~/Downloads/HIM" rather than the whole home path.
fn shown_path(app: &AppHandle, dir: &std::path::Path) -> String {
    match app.path().home_dir().ok().and_then(|h| dir.strip_prefix(&h).ok().map(|r| r.to_path_buf())) {
        Some(rest) => format!("~/{}", rest.to_string_lossy()),
        None => dir.to_string_lossy().into_owned(),
    }
}

/// Setup's Change...: a folder picker; returns the folder chosen (None if canceled).
/// Setup saves it with the other preferences.
#[tauri::command]
pub async fn pick_received_folder(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .set_title("Save files from buddies in")
        .set_directory(received_folder(&app))
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

fn client(app: &AppHandle) -> Result<hotline_im::Client, String> {
    let st = app.state::<Mutex<App>>();
    let a = st.lock().unwrap();
    a.live
        .as_ref()
        .filter(|l| l.connected)
        .map(|l| l.client.clone())
        .ok_or_else(|| "You're not signed on.".to_string())
}

/// Adds a line to a conversation and shows it.
fn add_line(app: &AppHandle, login: &str, line: Line) {
    {
        let st = app.state::<Mutex<App>>();
        let mut a = st.lock().unwrap();
        let Some(l) = a.live.as_mut() else { return };
        l.convos.entry(login.to_string()).or_default().push(line.clone());
    }
    let _ = app.emit("im", serde_json::json!({ "login": login, "line": line }));
    crate::history::save(app, login);
}

/// Changes a file line and shows the change. `persist` writes history (not for progress ticks).
fn update(app: &AppHandle, guid: &str, persist: bool, f: impl FnOnce(&mut FileInfo)) -> Option<(String, FileInfo, bool)> {
    let (login, line) = {
        let st = app.state::<Mutex<App>>();
        let mut a = st.lock().unwrap();
        let l = a.live.as_mut()?;
        let (login, line) = l.convos.iter_mut().find_map(|(login, c)| {
            c.iter_mut()
                .rev()
                .find(|x| x.file.as_ref().is_some_and(|f| f.guid == guid))
                .map(|x| (login.clone(), x))
        })?;
        f(line.file.as_mut()?);
        (login, line.clone())
    };
    let _ = app.emit("im", serde_json::json!({ "login": login, "line": line }));
    if persist {
        crate::history::save(app, &login);
    }
    let file = line.file.clone()?;
    Some((login, file, line.dir == "in"))
}

// ---------- what the server says ----------

pub fn on_offer(app: &AppHandle, offer: hotline_im::FileOffer) {
    let line = Line {
        guid: offer.guid.clone(),
        dir: "in".into(),
        from: offer.from.clone(),
        body: offer.name.clone(),
        ts: now(),
        state: "unread".into(),
        file: Some(FileInfo {
            guid: offer.guid,
            name: hotline_im::transfer::safe_name(&offer.name),
            size: offer.size,
            path: None,
            state: "incoming".into(),
            progress: 0.0,
            error: None,
        }),
    };
    add_line(app, &offer.from, line);
    crate::windows::open_im_window(app, &offer.from, false);
}

pub fn on_accepted(app: &AppHandle, guid: &str) {
    update(app, guid, true, |f| {
        if f.state == "offered" {
            f.state = "starting".into();
        }
    });
}

pub fn on_declined(app: &AppHandle, guid: &str) {
    update(app, guid, true, |f| {
        if f.state != "done" && f.state != "failed" {
            f.state = "declined".into();
        }
    });
}

/// File Ready: send ours, or take theirs into Downloads/HIM.
pub fn on_ready(app: &AppHandle, guid: String, relay_ref: u32) {
    let Some((_, file, incoming)) = update(app, &guid, false, |f| {
        f.state = "moving".into();
        f.progress = 0.0;
    }) else {
        return;
    };
    let Ok(c) = client(app) else { return };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last = Instant::now();
        let tick = {
            let app = app.clone();
            let guid = guid.clone();
            move |done: u64, total: u64| {
                if done < total && last.elapsed().as_millis() < 200 {
                    return;
                }
                last = Instant::now();
                let p = if total > 0 { done as f64 / total as f64 } else { 0.0 };
                update(&app, &guid, false, |f| f.progress = p);
            }
        };
        let result = if incoming {
            receive(&c, relay_ref, &received_folder(&app), tick).await.map(Some)
        } else {
            match tokio::fs::File::open(file.path.clone().unwrap_or_default()).await {
                Ok(f) => c.send_file(relay_ref, &file.name, file.size, f, tick).await.map(|_| None).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            }
        };
        update(&app, &guid, true, |f| match result {
            Ok(saved) => {
                if let Some(p) = saved {
                    f.path = Some(p);
                }
                f.state = "done".into();
                f.progress = 1.0;
            }
            Err(e) => {
                f.state = "failed".into();
                f.error = Some(e);
            }
        });
    });
}

async fn receive(
    c: &hotline_im::Client,
    relay_ref: u32,
    dir: &Path,
    tick: impl FnMut(u64, u64) + Send,
) -> Result<String, String> {
    let part = dir.join(format!(".him-{relay_ref}.part"));
    let out = tokio::fs::File::create(&part).await.map_err(|e| e.to_string())?;
    let name = match c.receive_file(relay_ref, out, tick).await {
        Ok(n) => n,
        Err(e) => {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(e.to_string());
        }
    };
    let dest = unique(dir, &name);
    tokio::fs::rename(&part, &dest).await.map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().into_owned())
}

/// `name`, or "name 2.ext"... whichever isn't taken.
fn unique(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..).map(|n| dir.join(format!("{stem} {n}{ext}"))).find(|p| !p.exists()).unwrap()
}

/// Big photos go smaller (2048 px, JPEG): a picture over 4 MB or 2048 px. Else as it is.
fn shrink_picture(src: &Path) -> Option<(Vec<u8>, String)> {
    let ext = src.extension()?.to_string_lossy().to_lowercase();
    if !["jpg", "jpeg", "png"].contains(&ext.as_str()) {
        return None;
    }
    let len = std::fs::metadata(src).ok()?.len();
    let img = image::open(src).ok()?;
    if len <= 4 << 20 && img.width().max(img.height()) <= 2048 {
        return None;
    }
    let img = img.resize(2048, 2048, image::imageops::FilterType::Lanczos3).to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&img).ok()?;
    let stem = src.file_stem()?.to_string_lossy().into_owned();
    Some((out.into_inner(), format!("{stem}.jpg")))
}

// ---------- what the IM window asks ----------

/// "Send File": pick a file and offer it.
#[tauri::command]
pub async fn send_file(app: AppHandle, login: String, path: Option<String>) -> Result<(), String> {
    let online = {
        let st = app.state::<Mutex<App>>();
        let a = st.lock().unwrap();
        a.live.as_ref().and_then(|l| l.roster.get(&login)).is_some_and(|b| {
            b.state == hotline_im::RosterState::Accepted && b.presence != hotline_im::Presence::Offline
        })
    };
    if !online {
        return Err("Files can only go to a buddy who's online.".into());
    }
    // A file dropped on the IM window comes with its path; Send File asks for one.
    let src = match path {
        Some(p) => {
            let p = std::path::PathBuf::from(p);
            if !p.is_file() {
                return Err("Only files can be sent (not folders).".into());
            }
            p
        }
        None => {
            let Some(picked) = app.dialog().file().set_title(format!("Send a File to {login}")).blocking_pick_file() else {
                return Ok(());
            };
            picked.into_path().map_err(|e| e.to_string())?
        }
    };
    // A copy of our own, readable however long they take to answer.
    let outbox = {
        let st = app.state::<Mutex<App>>();
        let a = st.lock().unwrap();
        a.data_dir("outgoing")
    };
    std::fs::create_dir_all(&outbox).map_err(|e| e.to_string())?;
    let (name, copy) = match shrink_picture(&src) {
        Some((bytes, name)) => {
            let p = outbox.join(format!("{}-{name}", now()));
            std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
            (name, p)
        }
        None => {
            let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
            let p = outbox.join(format!("{}-{name}", now()));
            std::fs::copy(&src, &p).map_err(|e| e.to_string())?;
            (name, p)
        }
    };
    let size = std::fs::metadata(&copy).map(|m| m.len()).unwrap_or(0);
    let c = client(&app)?;
    let guid = match c.offer_file(&login, &name, size).await {
        Ok(g) => g,
        Err(e) => {
            let _ = std::fs::remove_file(&copy);
            return Err(e.to_string());
        }
    };
    add_line(
        &app,
        &login,
        Line {
            guid: guid.clone(),
            dir: "out".into(),
            from: String::new(),
            body: name.clone(),
            ts: now(),
            state: "sent".into(),
            file: Some(FileInfo {
                guid,
                name,
                size,
                path: Some(copy.to_string_lossy().into_owned()),
                state: "offered".into(),
                progress: 0.0,
                error: None,
            }),
        },
    );
    Ok(())
}

#[tauri::command]
pub async fn accept_file(app: AppHandle, guid: String) -> Result<(), String> {
    let c = client(&app)?;
    update(&app, &guid, true, |f| f.state = "starting".into());
    if let Err(e) = c.accept_file(&guid).await {
        update(&app, &guid, true, |f| {
            f.state = "failed".into();
            f.error = Some(e.to_string());
        });
        return Err(e.to_string());
    }
    Ok(())
}

/// Turns down their file, or calls off ours.
#[tauri::command]
pub async fn decline_file(app: AppHandle, guid: String) -> Result<(), String> {
    update(&app, &guid, true, |f| f.state = "declined".into());
    if let Ok(c) = client(&app) {
        let _ = c.decline_file(&guid).await;
    }
    Ok(())
}

/// Only files a conversation names (sent or received) can be opened or shown.
fn known_path(app: &AppHandle, path: &str) -> bool {
    let st = app.state::<Mutex<App>>();
    let a = st.lock().unwrap();
    a.live.as_ref().is_some_and(|l| {
        l.convos.values().flatten().any(|x| x.file.as_ref().and_then(|f| f.path.as_deref()) == Some(path))
    })
}

#[tauri::command]
pub fn open_file(app: AppHandle, path: String, reveal: bool) -> Result<(), String> {
    if !known_path(&app, &path) {
        return Err("HIM only opens files from your conversations.".into());
    }
    if reveal {
        app.opener().reveal_item_in_dir(&path).map_err(|e| e.to_string())
    } else {
        app.opener().open_path(&path, None::<&str>).map_err(|e| e.to_string())
    }
}

/// A received picture's bytes, for showing it in the window.
#[tauri::command]
pub fn file_data(app: AppHandle, path: String) -> Result<tauri::ipc::Response, String> {
    if !known_path(&app, &path) {
        return Err("Not a file from your conversations.".into());
    }
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > 16 << 20 {
        return Err("Too big to preview.".into());
    }
    std::fs::read(&path).map(tauri::ipc::Response::new).map_err(|e| e.to_string())
}
