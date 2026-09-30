//! Buddy Icons (Capabilities-Buddy-Icons.md): yours, chosen from a file or a link and
//! kept on the server for your buddies, and theirs, fetched when their hash changes
//! and cached on disk by hash.

use crate::session::App;
use hotline_im::icon::{self, Format};
use hotline_im::messaging::hex;
use hotline_im::Client;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// What HIM takes in; bigger pictures are scaled down before they're sent.
const MAX_INPUT_BYTES: u64 = 256 * 1024;
/// AIM's size. A picture larger than the floor every server accepts is scaled to this.
const SIZE: u32 = 48;

/// Where the "Browse" button sends people. Only these open from HIM.
const ICON_SITES: &[&str] = &["https://www.badassbuddy.com/"];

pub fn log(app: &AppHandle, line: &str) {
    app.state::<Mutex<App>>().lock().unwrap().log(line);
}

fn icons_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let st = app.state::<std::sync::Mutex<crate::session::App>>();
    let dir = st.lock().unwrap().data_dir("icons");
    Ok(dir)
}

fn cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    icons_dir(app).map(|d| d.join("cache"))
}

/// Makes a picture fit every server: at most 64 pixels a side, 32 frames and 16 KB.
/// A picture that already fits is kept byte for byte, so classic GIFs stay as drawn.
pub fn prepare(bytes: &[u8]) -> Result<(Vec<u8>, Format), String> {
    let h = icon::inspect(bytes).ok_or("That doesn't look like a GIF, PNG or JPEG picture.")?;
    if h.width.max(h.height) <= icon::FLOOR_DIMENSION
        && h.frames <= icon::FLOOR_FRAMES
        && bytes.len() <= icon::DEFAULT_MAX_BYTES as usize
    {
        return Ok((bytes.to_vec(), h.format));
    }
    let (w, ht) = if h.width >= h.height {
        (SIZE, (SIZE * h.height / h.width).max(1))
    } else {
        ((SIZE * h.width / h.height).max(1), SIZE)
    };
    let too_big = || "That picture is too big for a Buddy Icon, even scaled down. Try a smaller one.".to_string();
    if h.format == Format::Gif && h.frames > 1 {
        use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
        use image::{AnimationDecoder, Delay, Frame};
        let frames = GifDecoder::new(std::io::Cursor::new(bytes))
            .and_then(|d| d.into_frames().collect_frames())
            .map_err(|e| format!("Couldn't read that GIF: {e}"))?;
        let scaled: Vec<(image::RgbaImage, u32)> = frames
            .iter()
            .take(icon::FLOOR_FRAMES)
            .map(|f| {
                let (n, d) = f.delay().numer_denom_ms();
                (
                    image::imageops::resize(f.buffer(), w, ht, image::imageops::FilterType::Triangle),
                    n / d.max(1),
                )
            })
            .collect();
        // Too many bytes: keep every other frame, holding each twice as long.
        let mut step = 1;
        while step <= scaled.len() {
            let mut out = Vec::new();
            {
                let mut enc = GifEncoder::new_with_speed(&mut out, 10);
                enc.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
                let picked = scaled.iter().step_by(step).map(|(img, ms)| {
                    Frame::from_parts(img.clone(), 0, 0, Delay::from_numer_denom_ms(ms * step as u32, 1))
                });
                enc.encode_frames(picked).map_err(|e| e.to_string())?;
            }
            if out.len() <= icon::DEFAULT_MAX_BYTES as usize {
                return Ok((out, Format::Gif));
            }
            step *= 2;
        }
        return Err(too_big());
    }
    let img = image::load_from_memory(bytes).map_err(|e| format!("Couldn't read that picture: {e}"))?;
    let img = img.resize(w, ht, image::imageops::FilterType::Lanczos3);
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    if out.len() > icon::DEFAULT_MAX_BYTES as usize {
        return Err(too_big());
    }
    Ok((out, Format::Png))
}

/// Opens a file picker; returns the chosen file's name, "gallery" when it was a zip
/// of many icons (the page then opens the gallery), or None if canceled.
#[tauri::command]
pub async fn pick_icon(app: AppHandle) -> Result<Option<String>, String> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Choose a Buddy Icon")
        .add_filter("Pictures, or a zip of them", &["gif", "png", "jpg", "jpeg", "zip"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let src = picked.into_path().map_err(|e| e.to_string())?;
    if src.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        return open_zip(&app, &src).await.map(Some);
    }
    if std::fs::metadata(&src).map_err(|e| e.to_string())?.len() > MAX_INPUT_BYTES {
        return Err("That picture is too big for a Buddy Icon (256 KB at most).".into());
    }
    let bytes = std::fs::read(&src).map_err(|e| e.to_string())?;
    let shown = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "icon".into());
    use_icon(&app, &bytes, &shown).await?;
    Ok(Some(shown))
}

/// One icon from a link the user copied (say, from BadassBuddy): fetched once and
/// kept, the same as downloading it.
#[tauri::command]
pub async fn icon_from_url(app: AppHandle, url: String) -> Result<String, String> {
    let url = url.trim().to_string();
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Paste the picture's full address (starting with https://).".into());
    }
    let fetched = tauri::async_runtime::spawn_blocking({
        let url = url.clone();
        move || -> Result<Vec<u8>, String> {
            let resp = ureq::get(&url)
                .timeout(std::time::Duration::from_secs(20))
                .set("User-Agent", concat!("HIM/", env!("CARGO_PKG_VERSION")))
                .call()
                .map_err(|e| format!("Couldn't get that picture: {e}"))?;
            let mut bytes = Vec::new();
            use std::io::Read;
            resp.into_reader()
                .take(MAX_INPUT_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            Ok(bytes)
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    if fetched.len() as u64 > MAX_INPUT_BYTES {
        return Err("That picture is too big for a Buddy Icon (256 KB at most).".into());
    }
    if Format::sniff(&fetched).is_none() {
        return Err("That link isn't a GIF, PNG or JPEG picture. Copy the icon's own address (right-click it > Copy Image Address).".into());
    }
    let shown = url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("icon")
        .split('?')
        .next()
        .unwrap_or("icon")
        .to_string();
    use_icon(&app, &fetched, &shown).await?;
    Ok(shown)
}

// ---------- a zip of icons (BadassBuddy's downloads come zipped) ----------

struct Gallery {
    path: PathBuf,
    archive: zip::ZipArchive<std::io::Cursor<Vec<u8>>>,
    /// (index in the archive, title)
    items: Vec<(usize, String)>,
}

static GALLERY: std::sync::LazyLock<Mutex<Option<Gallery>>> = std::sync::LazyLock::new(Default::default);

/// The pictures in a zip, skipping folders and macOS's `__MACOSX` / `._` copies, with
/// titles from a JSON index (`[{"title", "filename"}]`) when the zip has one.
fn read_zip(path: &std::path::Path) -> Result<Gallery, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| "That zip file couldn't be opened.".to_string())?;
    let mut titles = std::collections::HashMap::new();
    let mut items = Vec::new();
    for i in 0..archive.len() {
        let Ok(mut f) = archive.by_index(i) else { continue };
        let name = f.name().to_string();
        let base = name.rsplit('/').next().unwrap_or_default().to_string();
        if f.is_dir() || name.starts_with("__MACOSX/") || base.starts_with("._") {
            continue;
        }
        let ext = base.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
        if ext == "json" && f.size() < 4 << 20 {
            let mut s = String::new();
            use std::io::Read;
            if f.read_to_string(&mut s).is_ok() {
                if let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(&s) {
                    for v in list {
                        if let (Some(t), Some(n)) = (v["title"].as_str(), v["filename"].as_str()) {
                            titles.insert(n.to_string(), t.to_string());
                        }
                    }
                }
            }
        } else if matches!(ext.as_str(), "gif" | "png" | "jpg" | "jpeg") && f.size() <= MAX_INPUT_BYTES {
            items.push((i, base));
        }
    }
    for (_, name) in items.iter_mut() {
        let title = titles.get(name.as_str()).cloned().unwrap_or_else(|| {
            name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name).replace(['_', '-'], " ")
        });
        *name = title;
    }
    items.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    Ok(Gallery { path: path.to_path_buf(), archive, items })
}

fn gallery_bytes(g: &mut Gallery, i: usize) -> Result<Vec<u8>, String> {
    let idx = g.items.get(i).ok_or("No such icon.")?.0;
    let mut f = g.archive.by_index(idx).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    use std::io::Read;
    f.read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

async fn open_zip(app: &AppHandle, path: &std::path::Path) -> Result<String, String> {
    let path = path.to_path_buf();
    let mut g = tauri::async_runtime::spawn_blocking(move || read_zip(&path))
        .await
        .map_err(|e| e.to_string())??;
    match g.items.len() {
        0 => Err("There are no GIF, PNG or JPEG pictures in that zip.".into()),
        1 => {
            let title = g.items[0].1.clone();
            let bytes = gallery_bytes(&mut g, 0)?;
            use_icon(app, &bytes, &title).await?;
            Ok(title)
        }
        _ => {
            let shown = g.path.to_string_lossy().into_owned();
            *GALLERY.lock().unwrap() = Some(g);
            let prefs = {
                let st = app.state::<Mutex<App>>();
                let mut a = st.lock().unwrap();
                a.settings.prefs.icon_gallery = Some(shown);
                a.save_settings();
                a.settings.prefs.clone()
            };
            let _ = app.emit("prefs", &prefs);
            Ok("gallery".into())
        }
    }
}

/// The gallery's icons as (number, title), opening the remembered zip if needed.
#[tauri::command]
pub async fn gallery_list(app: AppHandle) -> Result<Vec<(usize, String)>, String> {
    let loaded = GALLERY.lock().unwrap().is_some();
    if !loaded {
        let path = {
            let st = app.state::<Mutex<App>>();
            let a = st.lock().unwrap();
            a.settings.prefs.icon_gallery.clone()
        }
        .ok_or("Choose a zip of icons first.")?;
        let g = tauri::async_runtime::spawn_blocking(move || read_zip(std::path::Path::new(&path)))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e} (Was the zip moved? Choose it again.)"))?;
        *GALLERY.lock().unwrap() = Some(g);
    }
    let g = GALLERY.lock().unwrap();
    Ok(g.as_ref()
        .map(|g| g.items.iter().enumerate().map(|(i, (_, t))| (i, t.clone())).collect())
        .unwrap_or_default())
}

#[tauri::command]
pub fn gallery_image(n: usize) -> Result<tauri::ipc::Response, String> {
    let mut g = GALLERY.lock().unwrap();
    let g = g.as_mut().ok_or("No gallery open.")?;
    gallery_bytes(g, n).map(tauri::ipc::Response::new)
}

#[tauri::command]
pub async fn gallery_use(app: AppHandle, n: usize) -> Result<String, String> {
    let (bytes, title) = {
        let mut g = GALLERY.lock().unwrap();
        let g = g.as_mut().ok_or("No gallery open.")?;
        let title = g.items.get(n).map(|x| x.1.clone()).unwrap_or_default();
        (gallery_bytes(g, n)?, title)
    };
    use_icon(&app, &bytes, &title).await?;
    Ok(title)
}

/// Fits the picture, keeps it, and sends it to the server if we're signed on.
async fn use_icon(app: &AppHandle, bytes: &[u8], shown: &str) -> Result<(), String> {
    let (bytes, format) = prepare(bytes)?;
    save_local(app, Some((&bytes, format)), shown)?;
    upload(app).await
}

/// Replaces (or with None, removes) our icon file and the pref that points to it.
fn save_local(app: &AppHandle, pic: Option<(&[u8], Format)>, shown: &str) -> Result<(), String> {
    let dir = icons_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // The icon being replaced is kept as previous.* (one level), never just deleted.
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let old: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        let is = |p: &PathBuf, stem: &str| p.file_stem().is_some_and(|s| s == stem);
        if old.iter().any(|p| is(p, "me")) {
            for p in old.iter().filter(|p| is(p, "previous")) {
                let _ = std::fs::remove_file(p);
            }
            for p in old.iter().filter(|p| is(p, "me")) {
                let ext = p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
                let _ = std::fs::rename(p, dir.join(format!("previous.{ext}")));
            }
        }
    }
    let pref = match pic {
        Some((bytes, format)) => {
            let file = format!("me.{}", format.ext());
            std::fs::write(dir.join(&file), bytes).map_err(|e| e.to_string())?;
            Some(format!("{file}|{shown}"))
        }
        None => None,
    };
    set_pref(app, pref);
    Ok(())
}

#[tauri::command]
pub async fn clear_icon(app: AppHandle) -> Result<(), String> {
    save_local(&app, None, "")?;
    upload(&app).await
}

fn set_pref(app: &AppHandle, v: Option<String>) {
    let prefs = {
        let st = app.state::<Mutex<App>>();
        let mut a = st.lock().unwrap();
        a.settings.prefs.buddy_icon = v;
        a.save_settings();
        a.settings.prefs.clone()
    };
    let _ = app.emit("prefs", &prefs);
}

/// Our icon's bytes, if we have one.
fn local_icon(app: &AppHandle) -> Option<Vec<u8>> {
    let file = {
        let st = app.state::<Mutex<App>>();
        let a = st.lock().unwrap();
        a.settings.prefs.buddy_icon.as_ref()?.split('|').next()?.to_string()
    };
    if file.contains('/') || file.contains('\\') || file.starts_with('.') {
        return None;
    }
    std::fs::read(icons_dir(app).ok()?.join(file)).ok()
}

/// The signed-on client, account key and Login, when the server keeps Buddy Icons.
fn icon_session(app: &AppHandle) -> Option<(Client, String, String)> {
    let st = app.state::<Mutex<App>>();
    let a = st.lock().unwrap();
    let l = a.live.as_ref().filter(|l| l.connected)?;
    l.client
        .has_buddy_icons()
        .then(|| (l.client.clone(), l.account.key(), l.account.login.clone()))
}

/// What we last agreed with the server on, per account: our file's hash and the hash
/// the server reported for what it stored. They differ when the server re-encodes
/// (Janus strips metadata), so each is compared with its own kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Synced {
    local: Option<String>,
    server: Option<String>,
}

impl Synced {
    /// Saved as "local:server", either side empty for none. An older single hash
    /// stands for both.
    fn parse(s: &str) -> Synced {
        let some = |x: &str| (!x.is_empty()).then(|| x.to_string());
        match s.split_once(':') {
            Some((l, r)) => Synced { local: some(l), server: some(r) },
            None => Synced { local: some(s), server: some(s) },
        }
    }
    fn save(&self) -> String {
        format!(
            "{}:{}",
            self.local.as_deref().unwrap_or_default(),
            self.server.as_deref().unwrap_or_default()
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Upload,
    Adopt,
    Nothing,
}

/// A change made here since the last sync wins; otherwise a different icon set
/// elsewhere is taken. A server reporting no icon never wipes ours: at sign-on that
/// can't be told apart from a server that lost it, so ours goes back up. (A clear made
/// from another session while we're signed on arrives as an 827 and is followed.)
fn decide(local: Option<&str>, synced: Option<&Synced>, server: Option<&str>) -> Action {
    match synced {
        None if local.is_some() => Action::Upload,
        None if server.is_some() => Action::Adopt,
        None => Action::Nothing,
        Some(s) if s.local.as_deref() != local => Action::Upload,
        Some(s) if s.server.as_deref() != server && server.is_some() => Action::Adopt,
        Some(_) if server.is_none() && local.is_some() => Action::Upload,
        Some(_) => Action::Nothing,
    }
}

fn synced(app: &AppHandle, key: &str) -> Option<Synced> {
    let st = app.state::<Mutex<App>>();
    let a = st.lock().unwrap();
    a.settings.icon_synced.get(key).map(|s| Synced::parse(s))
}

fn set_synced(app: &AppHandle, key: &str, s: Synced) {
    let st = app.state::<Mutex<App>>();
    let mut a = st.lock().unwrap();
    a.settings.icon_synced.insert(key.to_string(), s.save());
    a.save_settings();
}

fn local_hash(app: &AppHandle) -> Option<String> {
    local_icon(app).map(|b| hex(&icon::hash(&b)))
}

/// Sends our icon (or clears it) on the server. Not signed on, or a server without
/// Buddy Icons: nothing to do, and the next sign-on catches up.
async fn upload(app: &AppHandle) -> Result<(), String> {
    let Some((client, key, _)) = icon_session(app) else {
        return Ok(());
    };
    let pic = local_icon(app).unwrap_or_default();
    let server = client.set_buddy_icon(&pic).await.map_err(|e| {
        log(app, &format!("icon upload ({} bytes) refused: {e}", pic.len()));
        format!("Your Buddy Icon is saved here, but the server didn't take it: {e}")
    })?;
    log(app, &format!("icon upload ({} bytes) ok, server hash {server:?}", pic.len()));
    let local = (!pic.is_empty()).then(|| hex(&icon::hash(&pic)));
    set_synced(app, &key, Synced { local, server });
    Ok(())
}

/// Takes the icon the server holds for us (set from another session or computer).
async fn adopt(app: &AppHandle, client: &Client, key: &str, login: &str) -> Result<(), String> {
    let got = client.get_buddy_icon(login).await.map_err(|e| e.to_string())?;
    log(app, &format!("icon adopt: server sent {:?}", got.as_ref().map(|(h, p)| (h, p.len()))));
    // No picture after all: keep ours rather than guess.
    if let Some((hash, pic)) = got {
        let h = icon::inspect(&pic).ok_or("The server sent a picture HIM can't read.")?;
        save_local(app, Some((&pic, h.format)), "From your account")?;
        let local = Some(hex(&icon::hash(&pic)));
        set_synced(app, key, Synced { local, server: Some(hash) });
    }
    Ok(())
}

/// After signing on: send a change made here while away from this server, or take
/// one made elsewhere. The account's icon hash comes from Get User Info about ourselves.
pub async fn sync(app: AppHandle) {
    let Some((client, key, login)) = icon_session(&app) else {
        return;
    };
    let Ok(info) = client.get_info(&login).await else {
        return;
    };
    let local = local_hash(&app);
    let action = decide(local.as_deref(), synced(&app, &key).as_ref(), info.icon_hash.as_deref());
    log(&app, &format!(
        "icon sync {key}: limits {:?} bytes, {} px; local {local:?}, synced {:?}, server {:?} -> {action:?}",
        client.info.limits.max_icon_bytes,
        client.info.limits.max_icon_dimension,
        synced(&app, &key),
        info.icon_hash
    ));
    let result = match action {
        Action::Upload => upload(&app).await,
        Action::Adopt => adopt(&app, &client, &key, &login).await,
        Action::Nothing => {
            set_synced(&app, &key, Synced { local, server: info.icon_hash });
            Ok(())
        }
    };
    if let Err(e) = result {
        log(&app, &format!("icon sync failed: {e}"));
        let _ = app.emit("notice", e);
    }
}

/// Another of our sessions changed the icon (827 notification).
pub async fn own_icon_changed(app: AppHandle, hash: Option<String>) {
    let Some((client, key, login)) = icon_session(&app) else {
        return;
    };
    log(&app, &format!("icon changed in another session: {hash:?}"));
    if synced(&app, &key).is_some_and(|s| s.server == hash) {
        return; // already ours
    }
    if hash.is_none() {
        // Cleared on purpose from another session (ours is kept as previous.*).
        if let Err(e) = save_local(&app, None, "") {
            log(&app, &format!("icon clear failed: {e}"));
        }
        set_synced(&app, &key, Synced::default());
        return;
    }
    if let Err(e) = adopt(&app, &client, &key, &login).await {
        let _ = app.emit("notice", e);
    }
}

/// Our icon's bytes, for a page to show.
#[tauri::command]
pub fn icon_data(app: AppHandle) -> Result<tauri::ipc::Response, String> {
    local_icon(&app)
        .map(tauri::ipc::Response::new)
        .ok_or_else(|| "No Buddy Icon.".into())
}

/// A buddy's icon: from the cache when we have their current hash, else fetched once.
/// Empty when they have none.
#[tauri::command]
pub async fn buddy_icon(app: AppHandle, login: String) -> Result<tauri::ipc::Response, String> {
    let (client, hash) = {
        let st = app.state::<Mutex<App>>();
        let a = st.lock().unwrap();
        let Some(l) = a.live.as_ref() else {
            return Ok(tauri::ipc::Response::new(vec![]));
        };
        (
            l.client.clone(),
            l.roster.get(&login).and_then(|e| e.icon_hash.clone()),
        )
    };
    let Some(hash) = hash.filter(|h| h.len() == 32 && h.chars().all(|c| c.is_ascii_hexdigit()))
    else {
        log(&app, &format!("icon for {login}: roster has no hash"));
        return Ok(tauri::ipc::Response::new(vec![]));
    };
    let dir = cache_dir(&app)?;
    if let Ok(b) = std::fs::read(dir.join(&hash)) {
        return Ok(tauri::ipc::Response::new(b));
    }
    let fetched = client.get_buddy_icon(&login).await.map_err(|e| {
        log(&app, &format!("icon for {login}: 828 failed: {e}"));
        e.to_string()
    })?;
    let Some((got, pic)) = fetched else {
        log(&app, &format!("icon for {login}: 828 returned no picture (roster hash {hash})"));
        return Ok(tauri::ipc::Response::new(vec![]));
    };
    // Untrusted: it must be what the hash says and a picture of sane size.
    let header = icon::inspect(&pic);
    let sane = header.is_some_and(|h| h.width.max(h.height) <= 512 && h.frames <= 500);
    let actual = hex(&icon::hash(&pic));
    log(&app, &format!(
        "icon for {login}: roster {hash}, 828 {got}, {} bytes hashing to {actual}, header {header:?}, first bytes {:02x?}",
        pic.len(),
        &pic[..pic.len().min(12)]
    ));
    if got != actual || !sane {
        return Ok(tauri::ipc::Response::new(vec![]));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _ = std::fs::write(dir.join(&got), &pic);
    Ok(tauri::ipc::Response::new(pic))
}

/// A page reporting something worth a line in him.log.
#[tauri::command]
pub fn log_ui(app: AppHandle, text: String) {
    log(&app, &format!("ui: {}", text.chars().take(300).collect::<String>()));
}

/// Opens an icon site in the user's browser.
#[tauri::command]
pub fn open_icon_site(app: AppHandle, url: String) -> Result<(), String> {
    if !ICON_SITES.contains(&url.as_str()) {
        return Err("HIM only opens the icon sites it knows.".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// "Get a Screen Name": the server's own sign-up page, for the servers HIM knows.
#[tauri::command]
pub fn open_signup(app: AppHandle, host: String) -> Result<(), String> {
    let url = crate::settings::signup_page(&host)
        .ok_or("HIM doesn't know where this server makes accounts. Ask its operator.")?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 20, 20, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    fn gif(w: u32, h: u32, frames: usize) -> Vec<u8> {
        use image::codecs::gif::GifEncoder;
        let mut out = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut out);
            let fr = (0..frames).map(|i| {
                image::Frame::new(image::RgbaImage::from_pixel(w, h, image::Rgba([(i * 7) as u8, 90, 200, 255])))
            });
            enc.encode_frames(fr).unwrap();
        }
        out
    }

    #[test]
    fn small_pictures_are_kept_as_they_are() {
        let g = gif(48, 48, 4);
        assert_eq!(prepare(&g).unwrap().0, g);
    }

    #[test]
    fn big_pictures_are_scaled_to_48() {
        let (out, f) = prepare(&png(300, 150)).unwrap();
        let h = icon::inspect(&out).unwrap();
        assert_eq!((f, h.width, h.height), (Format::Png, 48, 24));

        let (out, f) = prepare(&gif(100, 100, 40)).unwrap();
        let h = icon::inspect(&out).unwrap();
        assert_eq!((f, h.width, h.height), (Format::Gif, 48, 48));
        assert!(h.frames <= icon::FLOOR_FRAMES);
        assert!(out.len() <= icon::DEFAULT_MAX_BYTES as usize);
    }

    #[test]
    fn sync_survives_a_reencoding_server() {
        let s = |l: Option<&str>, r: Option<&str>| Synced { local: l.map(Into::into), server: r.map(Into::into) };
        // Uploaded "a"; the server stored its re-encode "A". Nothing changed since.
        let after = s(Some("a"), Some("A"));
        assert_eq!(decide(Some("a"), Some(&after), Some("A")), Action::Nothing);
        // Chosen here while signed off: upload.
        assert_eq!(decide(Some("b"), Some(&after), Some("A")), Action::Upload);
        assert_eq!(decide(None, Some(&after), Some("A")), Action::Upload);
        // Changed on another computer: take it.
        assert_eq!(decide(Some("a"), Some(&after), Some("C")), Action::Adopt);
        // The server says none: never wipe ours, send it back up.
        assert_eq!(decide(Some("a"), Some(&after), None), Action::Upload);
        assert_eq!(decide(None, Some(&s(None, None)), None), Action::Nothing);
        // First time on an account.
        assert_eq!(decide(Some("a"), None, Some("Z")), Action::Upload);
        assert_eq!(decide(None, None, Some("Z")), Action::Adopt);
        assert_eq!(decide(None, None, None), Action::Nothing);
        // Saved forms, old and new.
        assert_eq!(Synced::parse(&after.save()), after);
        assert_eq!(Synced::parse("x"), s(Some("x"), Some("x")));
        assert_eq!(Synced::parse(":"), Synced::default());
    }

    #[test]
    fn zips_skip_mac_extras_and_take_titles_from_the_index() {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("pack.json", o).unwrap();
            z.write_all(br#"[{"title":"Do the Dew","filename":"dew.gif"}]"#).unwrap();
            z.add_directory("pack/", o).unwrap();
            z.start_file("pack/dew.gif", o).unwrap();
            z.write_all(&gif(48, 48, 2)).unwrap();
            z.start_file("pack/snow_man.png", o).unwrap();
            z.write_all(&png(20, 20)).unwrap();
            z.start_file("__MACOSX/pack/._dew.gif", o).unwrap();
            z.write_all(b"junk").unwrap();
            z.start_file("pack/readme.txt", o).unwrap();
            z.write_all(b"hi").unwrap();
            z.finish().unwrap();
        }
        let dir = std::env::temp_dir().join(format!("him-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pack.zip");
        std::fs::write(&path, buf.into_inner()).unwrap();
        let mut g = read_zip(&path).unwrap();
        let titles: Vec<_> = g.items.iter().map(|x| x.1.as_str()).collect();
        assert_eq!(titles, ["Do the Dew", "snow man"]);
        assert!(icon::inspect(&gallery_bytes(&mut g, 0).unwrap()).is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn not_a_picture() {
        assert!(prepare(b"hello").is_err());
    }
}
