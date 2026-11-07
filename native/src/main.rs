#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod account;
mod app;
mod artwork;
mod backend;
mod catalog_ui;
mod discord;
mod fonts;
mod icons;
mod playback_session;
mod player;
mod preview;
mod queue;
mod theme;
mod timeline;
mod volume;

pub static STREAM_CHECK_FAILED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn main() -> eframe::Result {
    if std::env::args().any(|arg| arg == "--smoke-artwork") {
        if let Err(error) = artwork::smoke() {
            eprintln!("Artwork check failed: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--smoke-rpc") {
        match discord::smoke() {
            Ok(()) => println!("Discord handshake and activity acknowledgement verified"),
            Err(error) => {
                eprintln!("Discord RPC check failed: {error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    let smoke = std::env::args().any(|arg| arg == "--smoke-stream")
        || account::smoke_active()
        || preview::active();
    let instance = single_instance::SingleInstance::new("RustifiedYoutube-native")
        .expect("single instance mutex");
    if !smoke && !instance.is_single() {
        use windows::{
            Win32::UI::WindowsAndMessaging::{
                FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
            },
            core::w,
        };
        unsafe {
            if let Ok(window) = FindWindowW(None, w!("RustifiedYoutube")) {
                let _ = ShowWindow(window, SW_RESTORE);
                let _ = SetForegroundWindow(window);
            }
        }
        return Ok(());
    }
    let image = image::load_from_memory(include_bytes!("../../assets/icon.png"))
        .expect("app icon")
        .to_rgba8();
    let icon = eframe::egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(if preview::compact() {
                [1080., 700.]
            } else {
                [1440., 900.]
            })
            .with_min_inner_size([1080., 700.])
            .with_icon(icon),
        renderer: eframe::Renderer::Wgpu,
        persist_window: !smoke,
        persistence_path: preview::storage_path(),
        ..Default::default()
    };
    let result = eframe::run_native(
        "RustifiedYoutube",
        options,
        Box::new(|cc| Ok(Box::new(app::MusicApp::new(cc)))),
    );
    drop(instance);
    if smoke && STREAM_CHECK_FAILED.load(std::sync::atomic::Ordering::SeqCst) {
        std::process::exit(1);
    }
    result
}
