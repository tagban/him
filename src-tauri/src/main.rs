// HIM, the Hotline Instant Messenger: the late-90s AIM experience on the Hotline IM network.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod history;
mod icons;
mod rooms;
mod session;
mod settings;
mod sounds;
mod windows;

use session::App;
use std::sync::Mutex;
use tauri::Manager;

/// Debug builds only: drive the app from the environment for testing without a mouse.
/// HIM_DEV_SIGNON="login:password@host:port", HIM_DEV_IM="login:message".
#[cfg(debug_assertions)]
fn dev_script(app: tauri::AppHandle) {
    let spec = std::env::var("HIM_DEV_SIGNON").unwrap_or_default();
    let any = [
        "HIM_DEV_SIGNON",
        "HIM_DEV_OPEN",
        "HIM_DEV_EVAL",
        "HIM_DEV_IM",
    ]
    .iter()
    .any(|v| std::env::var(v).is_ok());
    if !any {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        // An empty HIM_DEV_SIGNON skips signing on (to test the Sign On window itself).
        if !spec.is_empty() {
            let (cred, addr) = spec.split_once('@').unwrap_or((&spec, "127.0.0.1:5500"));
            let (login, password) = cred.split_once(':').unwrap_or((cred, ""));
            let (host, port) = addr
                .rsplit_once(':')
                .map(|(h, p)| (h, p.parse().unwrap_or(5500)))
                .unwrap_or((addr, 5500));
            let req = session::SignOnRequest {
                login: login.into(),
                password: password.into(),
                host: host.into(),
                port,
                security: hotline_im::Security::Auto,
                save_password: false,
                auto_login: false,
            };
            if let Err(e) = session::do_sign_on(&app, req).await {
                eprintln!("dev sign-on failed: {e}");
                return;
            }
        }
        // HIM_DEV_OPEN="away,info=bob,addbuddy" opens dialogs.
        if let Ok(list) = std::env::var("HIM_DEV_OPEN") {
            for item in list.split(',') {
                let (kind, arg) = item
                    .split_once('=')
                    .map(|(k, a)| (k, Some(a.to_string())))
                    .unwrap_or((item, None));
                windows::open_dialog(app.clone(), kind.to_string(), arg).await;
            }
        }
        // HIM_DEV_EVAL="label|delay_ms|js;;label|delay_ms|js": run page code in a window.
        if let Ok(steps) = std::env::var("HIM_DEV_EVAL") {
            use tauri::Manager;
            for step in steps.split(";;") {
                let mut parts = step.splitn(3, '|');
                let (Some(label), Some(delay), Some(js)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                tokio::time::sleep(std::time::Duration::from_millis(
                    delay.trim().parse().unwrap_or(500),
                ))
                .await;
                if label.trim() == "open" {
                    let (kind, arg) = js
                        .split_once('=')
                        .map(|(k, a)| (k, Some(a.to_string())))
                        .unwrap_or((js, None));
                    windows::open_dialog(app.clone(), kind.trim().to_string(), arg).await;
                    continue;
                }
                match app.get_webview_window(label.trim()) {
                    Some(w) => {
                        let _ = w.eval(js);
                    }
                    None => eprintln!("dev eval: no window {label}"),
                }
            }
        }
        if let Ok(im) = std::env::var("HIM_DEV_IM") {
            let (to, text) = im.split_once(':').unwrap_or((&im, "hello"));
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            windows::open_im_window(&app, to, true);
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
            if let Err(e) = session::send_im(app.clone(), to.into(), text.into()).await {
                eprintln!("dev IM failed: {e}");
            }
        }
    });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // Windows reopen where they were left (AIM remembered its Buddy List spot).
        // Position only: sizes stay as each window is designed.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::POSITION)
                .build(),
        )
        .setup(|app| {
            // Test runs driven by HIM_DEV_* keep their own settings, away from the user's.
            let testing = cfg!(debug_assertions) && std::env::vars().any(|(k, _)| k.starts_with("HIM_DEV_"));
            let path = app
                .path()
                .app_config_dir()
                .expect("a config folder")
                .join(if testing { "settings.dev.json" } else { "settings.json" });
            app.manage(Mutex::new(App::new(path)));
            app.manage(Mutex::new(rooms::Rooms::default()));
            #[cfg(debug_assertions)]
            dev_script(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            session::get_settings,
            session::save_prefs,
            session::forget_account,
            session::sign_on,
            session::sign_off,
            session::get_state,
            session::get_conversation,
            session::send_im,
            session::mark_read,
            session::typing,
            session::set_away,
            session::save_away_messages,
            session::add_buddy,
            session::respond_request,
            session::get_request_note,
            session::remove_buddy,
            session::block_buddy,
            session::unblock_buddy,
            session::set_alias,
            session::get_info,
            session::set_profile,
            session::set_display_name,
            session::search_users,
            session::add_group,
            session::rename_group,
            session::delete_group,
            session::move_buddy,
            rooms::list_rooms,
            rooms::join_room,
            rooms::chat_only,
            rooms::get_room,
            rooms::room_send,
            rooms::leave_room,
            rooms::rejoin_room,
            icons::pick_icon,
            icons::icon_from_url,
            icons::clear_icon,
            icons::icon_data,
            icons::open_icon_site,
            icons::buddy_icon,
            icons::open_signup,
            icons::log_ui,
            icons::gallery_list,
            icons::gallery_image,
            icons::gallery_use,
            sounds::pick_sound,
            sounds::clear_sound,
            sounds::sound_data,
            windows::open_im,
            windows::open_dialog,
            windows::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building HIM")
        .run(|app, event| {
            // Closing the Buddy List signs off; with no windows left, quit (AIM did).
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(s) = app.try_state::<Mutex<App>>() {
                    s.lock().unwrap().shutdown();
                }
                if let Some(r) = app.try_state::<Mutex<rooms::Rooms>>() {
                    r.lock().unwrap().leave_all();
                }
            }
        });
}
