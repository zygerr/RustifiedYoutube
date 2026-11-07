//! Development-only fixture rendering; never opens the media or account session.
use eframe::egui;
use rustified_core::model::{Playlist, Track};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub fn active() -> bool {
    cfg!(debug_assertions) && std::env::args().any(|arg| arg == "--ui-snapshot")
}
pub fn theme() -> Option<crate::theme::Theme> {
    if !active() {
        return None;
    }
    Some(
        match std::env::args()
            .skip_while(|arg| arg != "--theme")
            .nth(1)
            .as_deref()
        {
            Some("purple") => crate::theme::Theme::PurpleHaze,
            Some("oled") => crate::theme::Theme::Oled,
            _ => crate::theme::Theme::Classic,
        },
    )
}
pub fn compact() -> bool {
    active() && std::env::args().any(|arg| arg == "--compact")
}
pub fn storage_path() -> Option<PathBuf> {
    active().then(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/ui-preview/test-preferences.ron")
    })
}
pub struct Snapshot {
    pub page: String,
    frames: u32,
    started: Instant,
    requested: bool,
}
impl Snapshot {
    pub fn new() -> Option<Self> {
        if !active() {
            return None;
        }
        let page = std::env::args()
            .skip_while(|arg| arg != "--ui-snapshot")
            .nth(1)
            .unwrap_or_else(|| "discover".into());
        let page = if [
            "discover", "library", "search", "queue", "player", "settings",
        ]
        .contains(&page.as_str())
        {
            page
        } else {
            "discover".into()
        };
        Some(Self {
            page,
            frames: 0,
            started: Instant::now(),
            requested: false,
        })
    }
    pub fn finish(&mut self, ctx: &egui::Context) -> bool {
        self.frames += 1;
        if self.frames == 1 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(if compact() {
                egui::vec2(1080., 700.)
            } else {
                egui::vec2(1440., 900.)
            }));
        }
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = image {
            let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/ui-preview");
            let name = format!(
                "{}-{}-{}.png",
                self.page,
                match theme().unwrap_or_default() {
                    crate::theme::Theme::Classic => "classic",
                    crate::theme::Theme::PurpleHaze => "purple",
                    crate::theme::Theme::Oled => "oled",
                },
                if compact() { "1080" } else { "1440" }
            );
            let output = directory.join(name);
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                std::fs::create_dir_all(directory)?;
                let pixels = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();
                let image =
                    image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, pixels)
                        .ok_or("Invalid screenshot dimensions")?;
                image.save(&output)?;
                Ok(())
            })();
            match result {
                Ok(()) => println!("UI snapshot saved: {}", output.display()),
                Err(error) => {
                    eprintln!("Snapshot failed: {error}");
                    crate::STREAM_CHECK_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        if self.frames >= 10 && !self.requested && ctx.content_rect().width() >= 1000. {
            self.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        if self.started.elapsed() > Duration::from_secs(10) {
            eprintln!("UI snapshot timed out");
            crate::STREAM_CHECK_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return true;
        }
        ctx.request_repaint_after(Duration::from_millis(40));
        false
    }
}
pub fn tracks() -> Vec<Track> {
    let titles = [
        "𝗡𝗜𝗚𝗛𝗧 𝗦𝗛𝗜𝗙𝗧 🚀 𝗔𝗟𝗟 𝗧𝗛𝗘 𝗖𝗜𝗧𝗬 𝗟𝗜𝗚𝗛𝗧𝗦 ▪ 𝗢𝗙𝗜𝗖𝗜𝗔𝗟 𝗔𝗨𝗗𝗜𝗢",
        "Paper Planes (feat. June Hollow)",
        "NIGHT SHIFT",
        "Slow Motion Summer",
        "A very long song title that should always fit the same card height without moving the next row",
        "Glass Harbor",
    ];
    (0..24)
        .map(|index| Track {
            id: format!("fixture{index:04}"),
            title: titles[index % titles.len()].into(),
            artist: [
                "Nova Lane",
                "Marlow & Finch",
                "Nova Lane",
                "Lumen Drift",
                "Various artists",
                "Marlow & Finch",
            ][index % 6]
                .into(),
            cover: format!("fixture-{}", index % 6),
            duration: "3:28".into(),
            is_video: index % 2 == 0,
            ..Default::default()
        })
        .collect()
}
pub fn playlists() -> Vec<Playlist> {
    (0..16)
        .map(|index| Playlist {
            id: format!("playlist-{index}"),
            title: [
                "Your likes",
                "Music for late nights",
                "Weekend favorites",
                "Focus & unwind",
                "A playlist with a long title that must stay aligned",
                "On repeat",
            ][index % 6]
                .into(),
            subtitle: "Playlist · 24 songs".into(),
            cover: format!("fixture-{}", index % 6),
        })
        .collect()
}
pub fn textures(ctx: &egui::Context) -> Vec<(String, egui::TextureHandle)> {
    (0..6)
        .map(|index| {
            let size = match index % 3 {
                0 => [320, 180],
                1 => [240, 240],
                _ => [180, 320],
            };
            let base = [
                [35, 83, 141],
                [156, 72, 37],
                [65, 117, 76],
                [124, 61, 130],
                [170, 125, 35],
                [45, 123, 136],
            ][index];
            let mut image = egui::ColorImage::filled(size, egui::Color32::BLACK);
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let stripe = if (x / 24 + y / 24) % 2 == 0 {
                        1.0
                    } else {
                        0.72
                    };
                    let light = (0.55 + 0.45 * y as f32 / size[1] as f32) * stripe;
                    image.pixels[y * size[0] + x] = egui::Color32::from_rgb(
                        (base[0] as f32 * light) as u8,
                        (base[1] as f32 * light) as u8,
                        (base[2] as f32 * light) as u8,
                    );
                }
            }
            let name = format!("fixture-{index}");
            (
                name.clone(),
                ctx.load_texture(name, image, egui::TextureOptions::LINEAR),
            )
        })
        .collect()
}
