#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod backend;
mod icons;
mod player;

fn main() -> eframe::Result {
    if std::env::args().any(|arg| arg == "--smoke-audio") {
        let result = smoke_audio();
        if let Err(error) = result {
            eprintln!("Native audio check failed: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let instance = single_instance::SingleInstance::new("RustifiedYoutube-native")
        .expect("single instance mutex");
    if !instance.is_single() {
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
            .with_inner_size([1320., 850.])
            .with_min_inner_size([980., 650.])
            .with_icon(icon),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    let result = eframe::run_native(
        "RustifiedYoutube",
        options,
        Box::new(|cc| Ok(Box::new(app::MusicApp::new(cc)))),
    );
    drop(instance);
    result
}

fn smoke_audio() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let url = runtime.block_on(async {
        let cookie = if std::env::args().any(|arg| arg == "--authenticated") {
            rustified_core::session::load_cookie()?
        } else {
            None
        };
        let api = rustified_core::api::Catalog::new(cookie.as_deref()).await?;
        let tracks = api.search("Rick Astley Never Gonna Give You Up").await?;
        let track = tracks
            .first()
            .ok_or_else(|| std::io::Error::other("No search results"))?;
        let url = rustified_core::resolver::prepare(&track.id, cookie.as_deref()).await?;
        Ok::<_, Box<dyn std::error::Error>>(url)
    })?;
    let player = player::Player::start(eframe::egui::Context::default());
    player.send(player::PlayerCommand::Volume(0.));
    player.send(player::PlayerCommand::Play(url));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if let Ok(state) = player.rx.recv_timeout(std::time::Duration::from_secs(2)) {
            if let Some(error) = state.error {
                return Err(std::io::Error::other(error).into());
            }
            if state.position > 0.5 && state.duration > 1. {
                player.send(player::PlayerCommand::Stop);
                println!(
                    "Native Windows audio verified: playback position advanced, duration {:.0}s (muted test)",
                    state.duration
                );
                return Ok(());
            }
        }
    }
    Err(std::io::Error::other("Native player did not start within 30 seconds").into())
}
