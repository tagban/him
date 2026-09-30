//! AIM was a family of small separate windows; so is this. Every window is
//! undecorated and draws its own Windows 98 title bar.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Window labels may only hold `a-zA-Z0-9-/:_`, so logins are hex-encoded.
pub fn label_for(prefix: &str, login: &str) -> String {
    format!(
        "{prefix}-{}",
        login
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn open(
    app: &AppHandle,
    label: &str,
    page: &str,
    title: &str,
    w: f64,
    h: f64,
    resizable: bool,
    focus: bool,
) -> Option<WebviewWindow> {
    if let Some(win) = app.get_webview_window(label) {
        if focus {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        }
        return Some(win);
    }
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .title(title)
        .inner_size(w, h)
        .min_inner_size(w.min(160.0), h.min(140.0))
        .resizable(resizable)
        .decorations(false)
        .focused(focus)
        .build()
        .ok()
}

pub fn open_buddy_list(app: &AppHandle) {
    let placed = app.get_webview_window("buddylist").is_some();
    if let Some(win) = open(
        app,
        "buddylist",
        "buddylist.html",
        "Buddy List",
        214.0,
        470.0,
        true,
        true,
    ) {
        // First time ever: AIM's spot at the right edge. After that the window-state
        // plugin puts it back where the user left it.
        let remembered = app
            .path()
            .app_config_dir()
            .ok()
            .and_then(|d| std::fs::read_to_string(d.join(".window-state.json")).ok())
            .is_some_and(|s| s.contains("\"buddylist\""));
        if !placed && !remembered {
            // AIM lived at the right edge of the screen.
            if let Ok(Some(mon)) = win.current_monitor() {
                let scale = mon.scale_factor();
                let size = mon.size().to_logical::<f64>(scale);
                let pos = mon.position().to_logical::<f64>(scale);
                let _ = win.set_position(tauri::LogicalPosition::new(
                    pos.x + size.width - 214.0 - 40.0,
                    pos.y + 70.0,
                ));
            }
        }
    }
}

pub fn open_im_window(app: &AppHandle, login: &str, focus: bool) {
    let label = label_for("im", login);
    open(
        app,
        &label,
        &format!("im.html?u={}", enc(login)),
        "Instant Message",
        400.0,
        330.0,
        true,
        focus,
    );
}

#[tauri::command]
pub async fn open_im(app: AppHandle, login: String) {
    open_im_window(&app, &login, true);
}

/// Small dialogs: addbuddy, info, away, setup, request, agreement, profile, groups.
#[tauri::command]
pub async fn open_dialog(app: AppHandle, kind: String, arg: Option<String>) {
    let arg = arg.unwrap_or_default();
    let (w, h, title) = match kind.as_str() {
        "addbuddy" => (300.0, 220.0, "Add Buddy"),
        "info" => (300.0, 330.0, "Buddy Info"),
        "away" => (340.0, 300.0, "Away Message"),
        "setup" => (380.0, 522.0, "Setup"),
        "request" => (300.0, 190.0, "Buddy Request"),
        "profile" => (330.0, 380.0, "Edit Profile"),
        "find" => (320.0, 300.0, "Find a Buddy"),
        "group" => (280.0, 150.0, "Group"),
        "about" => (300.0, 250.0, "About"),
        "icon" => (320.0, 252.0, "Buddy Icon"),
        "name" => (290.0, 170.0, "Display Name"),
        "gallery" => (470.0, 440.0, "Buddy Icons"),
        "chatrooms" => (430.0, 400.0, "Chat Rooms"),
        _ => return,
    };
    let label = if arg.is_empty() {
        kind.clone()
    } else {
        label_for(&kind, &arg)
    };
    open(
        &app,
        &label,
        &format!("{kind}.html?a={}", enc(&arg)),
        title,
        w,
        h,
        false,
        true,
    );
}

#[tauri::command]
pub async fn quit_app(app: AppHandle) {
    app.exit(0);
}
