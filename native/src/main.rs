#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod audio_loader;
mod backend;
mod icons;
mod player;
mod volume;

fn main() -> eframe::Result {
    if std::env::args().any(|arg| arg == "--smoke-audio" || arg == "--smoke-prefetch") {
        let result = if std::env::args().any(|arg| arg == "--smoke-prefetch") {
            smoke_prefetch()
        } else {
            smoke_audio()
        };
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

fn smoke_prefetch() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let (tx, rx) = std::sync::mpsc::channel();
        let loader = audio_loader::spawn(
            rustified_core::session::load_cookie()?,
            tx,
            eframe::egui::Context::default(),
        );
        let first = "Fixture0001";
        let next = "Vnx_DyrYdeM";
        let started = std::time::Instant::now();
        loader.send(audio_loader::Request::Prefetch(Some(first.into())))?;
        loader.send(audio_loader::Request::Play(1, first.into()))?;
        loader.send(audio_loader::Request::Prefetch(Some(next.into())))?;
        let deadline = started + std::time::Duration::from_secs(180);
        let mut cold = None;
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(backend::Response::Stream(1, result)) => {
                    cold = Some(result?);
                    println!("Cold preparation: {:.2}s", started.elapsed().as_secs_f64());
                }
                Ok(backend::Response::Preloaded(id)) if id == next => break,
                _ => {}
            }
        }
        if cold.is_none() {
            return Err(std::io::Error::other("Cold preparation did not finish").into());
        }
        for (id, video) in [(2, next), (3, first)] {
            let ready = std::time::Instant::now();
            loader.send(audio_loader::Request::Play(id, video.into()))?;
            loop {
                match rx.recv_timeout(std::time::Duration::from_secs(2))? {
                    backend::Response::Stream(request, result) if request == id => {
                        let audio = result?;
                        if ready.elapsed() > std::time::Duration::from_millis(500) {
                            return Err(
                                std::io::Error::other("Cached playback was too slow").into()
                            );
                        }
                        if !std::path::Path::new(
                            &reqwest::Url::parse(&audio.url)?
                                .to_file_path()
                                .map_err(|_| std::io::Error::other("Invalid cached audio"))?,
                        )
                        .is_file()
                        {
                            return Err(std::io::Error::other("Cached audio disappeared").into());
                        }
                        println!(
                            "{}: {:.2}ms",
                            if id == 2 {
                                "Prefetched next track"
                            } else {
                                "Replay previous track"
                            },
                            ready.elapsed().as_secs_f64() * 1000.0
                        );
                        break;
                    }
                    _ => {}
                }
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })
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
        let args: Vec<String> = std::env::args().collect();
        let query = args
            .iter()
            .position(|arg| arg == "--smoke-query")
            .and_then(|index| args.get(index + 1))
            .map(String::as_str)
            .unwrap_or("Rick Astley Never Gonna Give You Up");
        let tracks = api.search(query).await?;
        let track = tracks
            .first()
            .ok_or_else(|| std::io::Error::other("No search results"))?;
        let id = args
            .iter()
            .position(|arg| arg == "--smoke-video")
            .and_then(|index| args.get(index + 1))
            .map(String::as_str)
            .unwrap_or(&track.id);
        let url = rustified_core::resolver::prepare(id, cookie.as_deref()).await?;
        Ok::<_, Box<dyn std::error::Error>>(url)
    })?;
    let player = player::Player::start(eframe::egui::Context::default());
    player.send(player::PlayerCommand::Volume(0.));
    player.send(player::PlayerCommand::Play {
        generation: 1,
        audio: url,
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut verified = false;
    while std::time::Instant::now() < deadline {
        if let Ok(state) = player.rx.recv_timeout(std::time::Duration::from_secs(2)) {
            if let Some(error) = state.error {
                return Err(std::io::Error::other(error).into());
            }
            if state.ended {
                if verified {
                    println!(
                        "Native Windows audio verified: 8 seconds of playback, seeking and natural EOF (muted test)"
                    );
                    return Ok(());
                }
                return Err(
                    std::io::Error::other("Track ended before the audio check completed").into(),
                );
            }
            if !verified && state.position > 8.0 && state.duration > 10. && state.playing {
                println!(
                    "Playback advanced normally to {:.1}/{:.0}s; testing seek to ending",
                    state.position, state.duration
                );
                verified = true;
                player.send(player::PlayerCommand::Seek(state.duration - 1.0));
            }
        }
    }
    Err(std::io::Error::other("Native player did not start within 30 seconds").into())
}
