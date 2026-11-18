//! Keeps the restored main window usable.
//!
//! eframe persists the window geometry periodically and on exit. While the
//! window is minimized or hidden in the tray, Windows can report a near-zero
//! client size, which eframe saves (clamped to 64×64) and restores on the next
//! launch as a tiny window. This guard refuses to persist such geometry and
//! resizes a window that nevertheless starts below the minimum size.
use eframe::egui;

pub const MIN_SIZE: [f32; 2] = [1080., 700.];
pub const DEFAULT_SIZE: [f32; 2] = [1440., 900.];
pub const COMPACT_SIZE: [f32; 2] = [1080., 700.];
/// eframe's storage key for the native window geometry.
const STORAGE_KEY: &str = "window";
/// Tolerates fractional rounding from DPI scaling.
const SLACK: f32 = 2.;

/// Reads `inner_size_points` from eframe's serialized window settings.
fn saved_size(settings: &str) -> Option<(f32, f32)> {
    let (_, rest) = settings.split_once("inner_size_points:Some((")?;
    let (pair, _) = rest.split_once("))")?;
    let mut width = None;
    let mut height = None;
    for field in pair.split(',') {
        let (key, value) = field.split_once(':')?;
        let value: f32 = value.trim().parse().ok()?;
        match key.trim() {
            "x" => width = Some(value),
            "y" => height = Some(value),
            _ => {}
        }
    }
    Some((width?, height?))
}

fn too_small(width: f32, height: f32) -> bool {
    width + SLACK < MIN_SIZE[0] || height + SLACK < MIN_SIZE[1]
}

/// Whether saved geometry would restore a window of usable size.
fn usable(settings: &str) -> bool {
    if settings.contains("maximized:true") {
        return true;
    }
    saved_size(settings).is_none_or(|(width, height)| !too_small(width, height))
}

#[derive(Default)]
pub struct Guard {
    last_good: Option<String>,
    checked: bool,
}

impl Guard {
    pub fn new(storage: Option<&dyn eframe::Storage>) -> Self {
        Self {
            last_good: storage
                .and_then(|storage| storage.get_string(STORAGE_KEY))
                .filter(|settings| usable(settings)),
            checked: false,
        }
    }

    /// Called from `App::save`, which eframe runs right after writing the
    /// window geometry and before flushing, so bad geometry never reaches disk.
    pub fn repair(&mut self, storage: &mut dyn eframe::Storage) {
        let Some(current) = storage.get_string(STORAGE_KEY) else {
            return;
        };
        if usable(&current) {
            self.last_good = Some(current);
            return;
        }
        // Without a known good geometry, fall back to the default size and
        // let Windows choose the position.
        let replacement = self.last_good.clone().unwrap_or_else(|| {
            "(inner_position_pixels:None,outer_position_pixels:None,fullscreen:false,maximized:false,inner_size_points:None)".into()
        });
        storage.set_string(STORAGE_KEY, replacement);
    }

    /// Once the window first reports its size, enlarge it if it was restored
    /// below the minimum (for example from geometry saved by an older version).
    pub fn check_startup(&mut self, ctx: &egui::Context) {
        if self.checked {
            return;
        }
        let (rect, minimized, maximized) = ctx.input(|input| {
            let viewport = input.viewport();
            (viewport.inner_rect, viewport.minimized, viewport.maximized)
        });
        let Some(rect) = rect else {
            return;
        };
        if minimized == Some(true) {
            return;
        }
        self.checked = true;
        if maximized != Some(true) && too_small(rect.width(), rect.height()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::from(
                DEFAULT_SIZE,
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::Storage as _;
    use std::collections::HashMap;

    const TINY: &str = "(inner_position_pixels:Some((x:8.0,y:31.0)),outer_position_pixels:Some((x:0.0,y:0.0)),fullscreen:false,maximized:false,inner_size_points:Some((x:64.0,y:64.0)))";
    const GOOD: &str = "(inner_position_pixels:Some((x:8.0,y:31.0)),outer_position_pixels:Some((x:0.0,y:0.0)),fullscreen:false,maximized:false,inner_size_points:Some((x:1440.0,y:900.0)))";

    #[derive(Default)]
    struct Memory(HashMap<String, String>);
    impl eframe::Storage for Memory {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn saved_geometry_below_the_minimum_is_not_usable() {
        assert_eq!(saved_size(TINY), Some((64., 64.)));
        assert!(!usable(TINY));
        assert!(usable(GOOD));
        assert!(usable(&TINY.replace("maximized:false", "maximized:true")));
        assert!(usable(
            "(inner_position_pixels:None,outer_position_pixels:None,fullscreen:false,maximized:false,inner_size_points:None)"
        ));
        // DPI rounding just under the minimum still counts as usable.
        assert!(usable(
            &GOOD.replace("x:1440.0,y:900.0", "x:1079.5,y:699.4")
        ));
    }

    #[test]
    fn saving_while_minimized_keeps_the_last_good_geometry() {
        let mut storage = Memory::default();
        storage.set_string(STORAGE_KEY, GOOD.into());
        let mut guard = Guard::new(Some(&storage));
        storage.set_string(STORAGE_KEY, TINY.into());
        guard.repair(&mut storage);
        assert_eq!(storage.get_string(STORAGE_KEY).as_deref(), Some(GOOD));
    }

    #[test]
    fn a_corrupted_file_without_good_geometry_falls_back_to_the_default_size() {
        let mut storage = Memory::default();
        storage.set_string(STORAGE_KEY, TINY.into());
        let mut guard = Guard::new(Some(&storage));
        guard.repair(&mut storage);
        let repaired = storage.get_string(STORAGE_KEY).unwrap();
        assert!(repaired.contains("inner_size_points:None"));
        assert!(usable(&repaired));
    }

    #[test]
    fn a_window_restored_too_small_is_enlarged_once() {
        let ctx = egui::Context::default();
        let mut guard = Guard::default();
        let mut frame = |size: egui::Vec2| {
            let mut input = egui::RawInput::default();
            input
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default()
                .inner_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
            let output = ctx.run(input, |ctx| guard.check_startup(ctx));
            output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .map(|viewport| viewport.commands.clone())
                .unwrap_or_default()
        };
        assert_eq!(
            frame(egui::vec2(64., 64.)),
            vec![egui::ViewportCommand::InnerSize(egui::Vec2::from(
                DEFAULT_SIZE
            ))]
        );
        // Later user resizes are not overridden.
        assert!(frame(egui::vec2(64., 64.)).is_empty());
    }
}
