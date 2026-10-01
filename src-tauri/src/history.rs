//! IM conversations kept on this computer: one JSON file per buddy in
//! history/<screen name@server>/ beside the settings, the newest 2,000 lines each.

use crate::session::{App, Line};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const KEEP: usize = 2000;

fn safe(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || "-_.@:".contains(c) { c } else { '_' })
        .collect()
}

pub fn folder(a: &App, account_key: &str) -> PathBuf {
    a.data_dir("history").join(safe(account_key))
}

/// Everything said with everyone, for an account.
pub fn load(dir: &PathBuf) -> HashMap<String, Vec<Line>> {
    let mut out = HashMap::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        let Ok(saved) = serde_json::from_str::<Saved>(&text) else { continue };
        let lines = saved
            .lines
            .into_iter()
            .map(|mut l| {
                if l.state == "sending" {
                    l.state = "failed".into(); // HIM closed before it went
                }
                l
            })
            .collect();
        out.insert(saved.login, lines);
    }
    out
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Saved {
    login: String,
    lines: Vec<Line>,
}

/// Writes one conversation as it stands now.
pub fn save(app: &AppHandle, login: &str) {
    let job = {
        let st = app.state::<Mutex<App>>();
        let a = st.lock().unwrap();
        let Some(l) = a.live.as_ref() else { return };
        let Some(c) = l.convos.get(login) else { return };
        let lines = c[c.len().saturating_sub(KEEP)..].to_vec();
        (folder(&a, &l.account.key()), Saved { login: login.to_string(), lines })
    };
    let (dir, saved) = job;
    std::thread::spawn(move || {
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{}.json", safe(&saved.login)));
        let tmp = path.with_extension("json.tmp");
        if let Ok(text) = serde_json::to_string(&saved) {
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    });
}
