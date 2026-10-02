//! What the app remembers between runs: accounts, Buddy List groups, away
//! messages and preferences (JSON in the app's config folder). Passwords go to
//! the system keychain, never into this file.

use hotline_im::Security;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

const KEYCHAIN_SERVICE: &str = "HIM (Hotline Instant Messenger)";

/// Where a new install signs on until the user picks another server in Setup.
pub const DEFAULT_HOST: &str = "hotline.vespernet.net";
pub const DEFAULT_PORT: u16 = 5500;

/// Servers whose accounts can be made on a web page ("Get a Screen Name"). Hotline
/// has no sign-up transaction; accounts are made by the server's operators.
pub const SIGNUP_PAGES: &[(&str, &str)] = &[("hotline.vespernet.net", "https://agora.vespernet.net/messenger")];

/// Buddies HIM suggests on a server, optional: (host, login, name, about).
pub const SUGGESTED_BUDDIES: &[(&str, &str, &str, &str)] = &[
    ("hotline.vespernet.net", "john", "John", "Made HIM. Say hi!"),
    ("hotline.vespernet.net", "smarterchild", "SmarterChild", "A chatbot: weather, news, trivia"),
];

/// Debug builds also suggest on the local test server (mock-server), to try it there.
#[cfg(debug_assertions)]
pub const SUGGESTED_FOR_TESTS: &[(&str, &str, &str, &str)] = &[
    ("127.0.0.1", "carol", "Carol", "A test buddy on the local server"),
    ("127.0.0.1", "hotbot", "HotBot", "The test server's bot"),
];
#[cfg(not(debug_assertions))]
pub const SUGGESTED_FOR_TESTS: &[(&str, &str, &str, &str)] = &[];

pub fn signup_page(host: &str) -> Option<&'static str> {
    SIGNUP_PAGES
        .iter()
        .find(|(h, _)| h.eq_ignore_ascii_case(host.trim()))
        .map(|(_, u)| *u)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SavedAccount {
    pub login: String,
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub save_password: bool,
    pub auto_login: bool,
}

impl Default for SavedAccount {
    fn default() -> Self {
        SavedAccount {
            login: String::new(),
            host: DEFAULT_HOST.into(),
            port: DEFAULT_PORT,
            security: Security::Auto,
            save_password: false,
            auto_login: false,
        }
    }
}

impl SavedAccount {
    pub fn key(&self) -> String {
        format!("{}@{}:{}", self.login, self.host, self.port)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub name: String,
    pub members: Vec<String>,
}

/// A chat room (Hotline server) joined before, for the room list's "Recent".
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentRoom {
    pub host: String,
    pub port: u16,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwayMessage {
    pub label: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub sounds: bool,
    pub timestamps: bool,
    /// Draw :-) and friends as pictures.
    pub smileys: bool,
    /// Answer IMs once with the away message while away (AIM's auto-response).
    pub auto_response: bool,
    pub discoverable: bool,
    pub font_size: u8,
    /// The user's own sound files (copied into the app's sounds folder), by event:
    /// buddyIn, buddyOut, imReceive, imSend. Missing = HIM's built-in sound.
    pub custom_sounds: HashMap<String, String>,
    /// Your Buddy Icon ("file|original name" in the app's icons folder).
    pub buddy_icon: Option<String>,
    /// A zip of icons you opened (say, BadassBuddy's collection), to browse again.
    pub icon_gallery: Option<String>,
    /// Where files from buddies are saved; None is Downloads/HIM.
    pub download_dir: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            sounds: true,
            timestamps: false,
            smileys: true,
            auto_response: true,
            discoverable: false,
            font_size: 12,
            custom_sounds: HashMap::new(),
            buddy_icon: None,
            icon_gallery: None,
            download_dir: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub accounts: Vec<SavedAccount>,
    pub last_account: Option<String>,
    /// Buddy List groups per account key; the server has no groups of its own.
    pub groups: HashMap<String, Vec<Group>>,
    pub away_messages: Vec<AwayMessage>,
    pub prefs: Prefs,
    /// Where the chat room list comes from (host or host:port).
    pub trackers: Vec<String>,
    pub recent_rooms: Vec<RecentRoom>,
    /// More rooms to keep off the list (names containing any of these, ignoring case),
    /// on top of `HIDDEN_ROOMS`.
    pub hidden_rooms: Vec<String>,
    /// The name used in chat rooms when not signed on to IM.
    pub chat_nick: Option<String>,
    /// Accounts that said "Not now" to the suggested buddies.
    pub suggestions_hidden: Vec<String>,
    /// Per account key: "local:server", our icon file's hash and the hash the server
    /// reported for what it stored when we last agreed (empty = none; they differ when
    /// the server re-encodes). Tells a local change (upload it) from one made elsewhere.
    pub icon_synced: HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            accounts: Vec::new(),
            last_account: None,
            groups: HashMap::new(),
            away_messages: vec![
                AwayMessage {
                    label: "Default".into(),
                    text: "I am away from my computer right now.".into(),
                },
                AwayMessage {
                    label: "Eating".into(),
                    text: "Out to lunch. Back soon!".into(),
                },
                AwayMessage {
                    label: "Sleeping".into(),
                    text: "zzz... I'll write back in the morning.".into(),
                },
            ],
            prefs: Prefs::default(),
            trackers: hotline_im::tracker::DEFAULT_TRACKERS
                .iter()
                .map(|t| t.to_string())
                .collect(),
            recent_rooms: Vec::new(),
            hidden_rooms: Vec::new(),
            chat_nick: None,
            suggestions_hidden: Vec::new(),
            icon_synced: HashMap::new(),
        }
    }
}

/// Tracker entries that aren't chat rooms: dividers, archive-only servers, and the
/// trackers' own placeholder listings.
pub const HIDDEN_ROOMS: &[&str] = &["MAJOR MAC BACKUP", "----", "Welcome to hotline"];

pub const DEFAULT_GROUPS: &[&str] = &["Buddies", "Family", "Co-Workers"];

impl Settings {
    pub fn load(path: &PathBuf) -> Settings {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &PathBuf) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.tmp");
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    pub fn remember(&mut self, acct: SavedAccount) {
        let key = acct.key();
        self.accounts.retain(|a| a.key() != key);
        self.accounts.insert(0, acct);
        self.last_account = Some(key);
    }

    /// The account's groups, creating AIM's three defaults the first time.
    pub fn groups_for(&mut self, key: &str) -> &mut Vec<Group> {
        self.groups.entry(key.to_string()).or_insert_with(|| {
            DEFAULT_GROUPS
                .iter()
                .map(|n| Group {
                    name: n.to_string(),
                    members: vec![],
                })
                .collect()
        })
    }
}

pub fn keychain_get(key: &str) -> Option<String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, key)
        .ok()?
        .get_password()
        .ok()
}

pub fn keychain_set(key: &str, password: &str) {
    if let Ok(e) = keyring::Entry::new(KEYCHAIN_SERVICE, key) {
        let _ = e.set_password(password);
    }
}

pub fn keychain_forget(key: &str) {
    if let Ok(e) = keyring::Entry::new(KEYCHAIN_SERVICE, key) {
        let _ = e.delete_credential();
    }
}
