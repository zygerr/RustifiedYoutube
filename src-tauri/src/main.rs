#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod navigation;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    webview::NewWindowResponse,
    AppHandle, Manager, Theme, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

const MUSIC_URL: &str = "https://music.youtube.com/";
#[cfg(target_os = "windows")]
const HELP_URL: &str = "http://tauri.localhost/index.html";
#[cfg(not(target_os = "windows"))]
const HELP_URL: &str = "tauri://localhost/index.html";
const RELEASES_URL: &str = "https://github.com/zygerr/RustifiedYoutube/releases";
const PLAYBACK_SCRIPT: &str = include_str!("../../ui/playback.js");

fn state_flags() -> StateFlags {
    // Never restore visibility: an app quit from the tray must reopen visibly.
    StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn handle_action(app: &AppHandle, action: &str) {
    if action == "quit" {
        let _ = app.save_window_state(state_flags());
        app.exit(0);
        return;
    }
    if action == "show" {
        show_window(app);
        return;
    }
    if action == "releases" {
        let _ = open::that_detached(RELEASES_URL);
        return;
    }
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    match action {
        "home" => {
            let _ = window.navigate(MUSIC_URL.parse().expect("valid music URL"));
        }
        "reload" => {
            let _ = window.reload();
        }
        "connection_help" => {
            let _ = window.navigate(HELP_URL.parse().expect("valid help URL"));
        }
        "back" => {
            let _ = window.eval("history.back()");
        }
        "forward" => {
            let _ = window.eval("history.forward()");
        }
        "hide" => {
            let _ = app.save_window_state(state_flags());
            let _ = window.hide();
        }
        "play_pause" | "next" | "previous" | "mute" => {
            // Action comes only from the native menu, never from remote content.
            let script = format!("({PLAYBACK_SCRIPT})({action:?})");
            let _ = window.eval(script);
        }
        _ => {}
    }
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let item = |id, label, accelerator| MenuItem::with_id(app, id, label, true, accelerator);
    let home = item("home", "YouTube Music home", Some("Ctrl+H"))?;
    let back = item("back", "Back", Some("Alt+Left"))?;
    let forward = item("forward", "Forward", Some("Alt+Right"))?;
    let reload = item("reload", "Reload", Some("Ctrl+R"))?;
    let show = item("show", "Open RustifiedYoutube", None::<&str>)?;
    let hide = item("hide", "Keep playing in tray", Some("Ctrl+W"))?;
    let quit = item("quit", "Quit", Some("Ctrl+Q"))?;
    let play = item("play_pause", "Play / pause", Some("Ctrl+Alt+Space"))?;
    let next = item("next", "Next track", Some("Ctrl+Alt+Right"))?;
    let previous = item("previous", "Previous track", Some("Ctrl+Alt+Left"))?;
    let mute = item("mute", "Mute / unmute", Some("Ctrl+Alt+M"))?;
    let releases = item("releases", "Releases on GitHub", None::<&str>)?;
    let connection_help = item("connection_help", "Connection help", None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let app_menu = Submenu::with_items(
        app,
        "&App",
        true,
        &[&home, &back, &forward, &reload, &separator, &hide, &quit],
    )?;
    let playback_menu =
        Submenu::with_items(app, "&Playback", true, &[&play, &next, &previous, &mute])?;
    let help_menu = Submenu::with_items(app, "&Help", true, &[&connection_help, &releases])?;
    app.set_menu(Menu::with_items(
        app,
        &[&app_menu, &playback_menu, &help_menu],
    )?)?;

    let tray_menu = Menu::with_items(app, &[&show, &play, &next, &previous, &separator, &quit])?;
    TrayIconBuilder::with_id("player")
        .icon(app.default_window_icon().ok_or("missing app icon")?.clone())
        .tooltip("RustifiedYoutube — close the window to keep listening")
        .menu(&tray_menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;

    let profile_dir = app.path().app_local_data_dir()?.join("webview");
    std::fs::create_dir_all(&profile_dir)?;
    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(MUSIC_URL.parse()?))
        .title("RustifiedYoutube")
        .inner_size(1280.0, 840.0)
        .min_inner_size(860.0, 600.0)
        .theme(Some(Theme::Dark))
        .data_directory(profile_dir)
        .on_navigation(|url| {
            // This callback also runs on reloads and authentication redirects.
            // Keep it side-effect free: opening the browser here abandons login.
            navigation::allow_navigation(url) || url.as_str() == HELP_URL
        })
        .on_new_window(|url, _features| {
            match navigation::popup_route(&url) {
                // WebView2 creates a related popup sharing the browser context.
                // Do not navigate the opener or cancel Google's auth window.
                navigation::PopupRoute::WebView => NewWindowResponse::Allow,
                navigation::PopupRoute::Browser => {
                    let _ = open::that_detached(url.as_str());
                    NewWindowResponse::Deny
                }
                navigation::PopupRoute::Block => NewWindowResponse::Deny,
            }
        })
        .on_document_title_changed(|window, title| {
            let title = if title.is_empty() {
                "RustifiedYoutube".to_string()
            } else {
                format!("{title} — RustifiedYoutube")
            };
            let _ = window.set_title(&title);
        })
        .build()?;

    let close_handle = app.handle().clone();
    let close_window = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = close_handle.save_window_state(state_flags());
            let _ = close_window.hide();
        }
    });
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app);
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(state_flags())
                .build(),
        )
        .setup(setup)
        .on_menu_event(|app, event| handle_action(app, event.id().as_ref()))
        .run(tauri::generate_context!())
        .expect("RustifiedYoutube could not start");
}
