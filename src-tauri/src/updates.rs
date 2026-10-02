//! A light update check: once at startup, ask GitHub which HIM release is the latest
//! and, if it's newer than this one, tell the windows; they show a line with a link
//! to download it. Nothing about the user is sent (just HIM's version, as the user agent).

use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const LATEST: &str = "https://api.github.com/repos/tagban/him/releases/latest";
const RELEASES: &str = "https://github.com/tagban/him/releases";

#[derive(Clone, Serialize)]
pub struct Update {
    pub version: String,
    pub url: String,
}

/// What the check found (None: up to date, not checked yet, or GitHub didn't answer).
#[derive(Default)]
pub struct UpdateState(pub Mutex<Option<Update>>);

/// "v0.2.10" -> [0, 2, 10]
fn numbers(v: &str) -> Vec<u64> {
    v.trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .map_while(|p| p.parse().ok())
        .collect()
}

fn newer(theirs: &str, ours: &str) -> bool {
    let (a, b) = (numbers(theirs), numbers(ours));
    (0..a.len().max(b.len())).map(|i| (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0))).find(|(x, y)| x != y).is_some_and(|(x, y)| x > y)
}

/// Starts the check in the background (release builds; debug ones with HIM_UPDATE_CHECK=1).
pub fn check(app: AppHandle) {
    if cfg!(debug_assertions) && std::env::var("HIM_UPDATE_CHECK").is_err() {
        return;
    }
    tauri::async_runtime::spawn_blocking(move || {
        // Test runs can pretend to be older (HIM_UPDATE_PRETEND=0.0.1) to see the notice.
        let pretend = cfg!(debug_assertions).then(|| std::env::var("HIM_UPDATE_PRETEND").ok()).flatten();
        let ours = pretend.as_deref().unwrap_or(env!("CARGO_PKG_VERSION"));
        let latest = ureq::get(LATEST)
            .set("User-Agent", &format!("HIM/{ours}"))
            .set("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(10))
            .call()
            .ok()
            .and_then(|r| r.into_string().ok())
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        let Some(v) = latest else { return };
        let (Some(tag), false) = (v["tag_name"].as_str(), v["draft"].as_bool().unwrap_or(false)) else { return };
        if !newer(tag, ours) {
            return;
        }
        // Only ever HIM's own releases page.
        let url = v["html_url"].as_str().filter(|u: &&str| u.starts_with(RELEASES)).unwrap_or(RELEASES).to_string();
        let update = Update { version: tag.trim_start_matches(['v', 'V']).to_string(), url };
        *app.state::<UpdateState>().0.lock().unwrap() = Some(update.clone());
        let _ = app.emit("update", &update);
    });
}

/// For windows that open after the check: is there a newer HIM?
#[tauri::command]
pub fn update_available(state: tauri::State<'_, UpdateState>) -> Option<Update> {
    state.0.lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn compares_versions() {
        assert!(newer("v0.2.0", "0.1.1"));
        assert!(newer("0.1.10", "0.1.9"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("v0.1.1", "0.1.1"));
        assert!(!newer("0.1.0", "0.1.1"));
        assert!(!newer("garbage", "0.1.1"));
    }
}
