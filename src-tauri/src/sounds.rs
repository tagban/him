//! Custom sounds: the user picks their own file for an event, in place of the
//! classic recordings HIM comes with (ui/sounds). A picked file is copied into the
//! app's sounds folder.

use crate::session::App;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

pub const KINDS: &[&str] = &["buddyIn", "buddyOut", "imReceive", "imSend"];
const MAX_BYTES: u64 = 4 * 1024 * 1024;

fn sounds_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let st = app.state::<std::sync::Mutex<crate::session::App>>();
    let dir = st.lock().unwrap().data_dir("sounds");
    Ok(dir)
}

fn check_kind(kind: &str) -> Result<(), String> {
    if KINDS.contains(&kind) {
        Ok(())
    } else {
        Err("Unknown sound.".into())
    }
}

/// Opens a file picker; returns the chosen file's name, or None if canceled.
#[tauri::command]
pub async fn pick_sound(app: AppHandle, kind: String) -> Result<Option<String>, String> {
    check_kind(&kind)?;
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Choose a sound")
        .add_filter("Sounds", &["wav", "aif", "aiff", "mp3", "m4a", "ogg"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let src = picked.into_path().map_err(|e| e.to_string())?;
    let size = std::fs::metadata(&src).map_err(|e| e.to_string())?.len();
    if size > MAX_BYTES {
        return Err("That file is too big for a sound (4 MB at most).".into());
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("wav")
        .to_lowercase();
    let dir = sounds_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = format!("{kind}.{ext}");
    // Clear an older pick with another extension.
    for old in KINDS.iter().filter(|k| **k == kind) {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{old}."))
                {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
    std::fs::copy(&src, dir.join(&file)).map_err(|e| e.to_string())?;
    let shown = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(file.clone());
    let prefs = {
        let st = app.state::<Mutex<App>>();
        let mut a = st.lock().unwrap();
        a.settings
            .prefs
            .custom_sounds
            .insert(kind, format!("{file}|{shown}"));
        a.save_settings();
        a.settings.prefs.clone()
    };
    let _ = app.emit("prefs", &prefs);
    Ok(Some(shown))
}

#[tauri::command]
pub fn clear_sound(app: AppHandle, kind: String) -> Result<(), String> {
    check_kind(&kind)?;
    let prefs = {
        let st = app.state::<Mutex<App>>();
        let mut a = st.lock().unwrap();
        if let Some(v) = a.settings.prefs.custom_sounds.remove(&kind) {
            if let (Ok(dir), Some(file)) = (sounds_dir(&app), v.split('|').next()) {
                let _ = std::fs::remove_file(dir.join(file));
            }
        }
        a.save_settings();
        a.settings.prefs.clone()
    };
    let _ = app.emit("prefs", &prefs);
    Ok(())
}

/// The picked file's bytes, for the page to decode and play.
#[tauri::command]
pub fn sound_data(app: AppHandle, kind: String) -> Result<tauri::ipc::Response, String> {
    check_kind(&kind)?;
    let file = {
        let st = app.state::<Mutex<App>>();
        let a = st.lock().unwrap();
        a.settings
            .prefs
            .custom_sounds
            .get(&kind)
            .and_then(|v| v.split('|').next().map(str::to_string))
    }
    .ok_or("No custom sound.")?;
    if file.contains('/') || file.contains('\\') || file.starts_with('.') {
        return Err("Bad sound file name.".into());
    }
    let bytes = std::fs::read(sounds_dir(&app)?.join(file)).map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}
