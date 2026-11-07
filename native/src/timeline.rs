use crate::player::PlayerState;
use eframe::egui::{self, Sense, Vec2};
use std::time::{Duration, Instant};

struct Pending {
    target: f64,
    revision: u64,
    generation: u64,
    requested: Instant,
}
#[derive(Default)]
pub struct Timeline {
    preview: Option<f64>,
    pending: Option<Pending>,
}
impl Timeline {
    pub fn reset(&mut self) {
        self.preview = None;
        self.pending = None;
    }
    pub fn pending_confirmed(&self) -> bool {
        self.pending.is_none()
    }
    pub fn request(&mut self, target: f64, revision: u64, generation: u64) {
        self.preview = None;
        self.pending = Some(Pending {
            target,
            revision,
            generation,
            requested: Instant::now(),
        });
    }
    pub fn observe(&mut self, state: &PlayerState) {
        if self.pending.as_ref().is_some_and(|pending| {
            state.generation == pending.generation
                && (state.error.is_some()
                    || (state.seek_ack >= pending.revision
                        && (state.position - pending.target).abs() < 2.0))
        }) {
            self.pending = None;
        }
    }
    pub fn expire(&mut self) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.requested.elapsed() > Duration::from_secs(8))
        {
            self.pending = None;
            true
        } else {
            false
        }
    }
    pub fn position(&self, actual: f64) -> f64 {
        self.preview
            .or_else(|| self.pending.as_ref().map(|pending| pending.target))
            .unwrap_or(actual)
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        duration: f64,
        actual: f64,
        enabled: bool,
    ) -> (egui::Response, Option<f64>) {
        let width = ui.available_width().max(1.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
        let enabled = enabled && duration.is_finite() && duration > 0.0;
        let response = ui.interact(
            rect,
            ui.make_persistent_id(id),
            if enabled {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::slider(enabled, self.position(actual), "Playback position")
        });
        let mut seek = None;
        if enabled {
            if (response.clicked() || response.dragged() || response.drag_stopped())
                && let Some(pointer) = response.interact_pointer_pos()
            {
                self.preview = Some(
                    (((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64) * duration,
                );
            }
            if response.clicked() {
                response.request_focus();
                seek = self.preview;
            }
            if response.drag_stopped() {
                seek = self.preview;
            }
            if response.has_focus() {
                let delta = ui.input_mut(|input| {
                    if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                        5.0
                    } else if input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                        -5.0
                    } else {
                        0.0
                    }
                });
                if delta != 0.0 {
                    seek = Some((self.position(actual) + delta).clamp(0.0, duration));
                }
            }
        }
        let fraction = if duration > 0.0 {
            (self.position(actual) / duration).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let rail = egui::Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
        ui.painter()
            .rect_filled(rail, 2.0, crate::theme::palette(ui.ctx()).rail);
        if enabled {
            let accent = crate::theme::palette(ui.ctx()).progress;
            let end = rail.left() + fraction * rail.width();
            ui.painter().rect_filled(
                egui::Rect::from_min_max(rail.min, egui::pos2(end, rail.bottom())),
                2.0,
                accent,
            );
            ui.painter().circle_filled(
                egui::pos2(end, rail.center().y),
                if response.hovered() || response.dragged() {
                    6.0
                } else {
                    4.0
                },
                accent,
            );
        }
        (response, seek)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clicking_and_dragging_commit_the_selected_position() {
        let ctx = egui::Context::default();
        let mut timeline = Timeline::default();
        let mut frame = |events| {
            let mut seek = None;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(400., 80.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            seek = timeline.show(ui, "seek", 200., 10., true).1;
                        });
                },
            );
            seek
        };
        let pointer = |x, pressed| egui::Event::PointerButton {
            pos: egui::pos2(x, 12.),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![]);
        assert!(
            frame(vec![
                egui::Event::PointerMoved(egui::pos2(100., 12.)),
                pointer(100., true)
            ])
            .is_none()
        );
        assert_eq!(frame(vec![pointer(100., false)]), Some(50.));
        frame(vec![pointer(100., true)]);
        assert!(frame(vec![egui::Event::PointerMoved(egui::pos2(300., 12.))]).is_none());
        assert_eq!(frame(vec![pointer(350., false)]), Some(175.));
    }
    #[test]
    fn stale_updates_cannot_pull_the_thumb_back_after_seek() {
        let mut timeline = Timeline::default();
        timeline.request(90.0, 7, 2);
        timeline.observe(&PlayerState {
            generation: 2,
            position: 12.0,
            seek_ack: 6,
            ..Default::default()
        });
        assert_eq!(timeline.position(12.0), 90.0);
        timeline.observe(&PlayerState {
            generation: 2,
            position: 90.1,
            seek_ack: 7,
            ..Default::default()
        });
        assert_eq!(timeline.position(90.1), 90.1);
    }
    #[test]
    fn only_the_latest_seek_in_the_current_track_can_be_confirmed() {
        let mut timeline = Timeline::default();
        timeline.request(40.0, 1, 3);
        timeline.request(140.0, 2, 3);
        timeline.observe(&PlayerState {
            generation: 3,
            position: 40.0,
            seek_ack: 1,
            ..Default::default()
        });
        timeline.observe(&PlayerState {
            generation: 2,
            position: 140.0,
            seek_ack: 2,
            ..Default::default()
        });
        assert_eq!(timeline.position(40.0), 140.0);
        timeline.observe(&PlayerState {
            generation: 3,
            position: 140.0,
            seek_ack: 2,
            ..Default::default()
        });
        assert!(timeline.pending.is_none());
    }
    #[test]
    fn seek_rail_uses_the_actual_available_width() {
        let draw = |width| {
            let ctx = egui::Context::default();
            let mut timeline = Timeline::default();
            let mut measured = 0.0;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(width, 80.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            measured = timeline.show(ui, "seek", 200.0, 50.0, true).0.rect.width();
                        });
                },
            );
            measured
        };
        assert!((draw(300.0) - 300.0).abs() < 1.0);
        assert!((draw(900.0) - 900.0).abs() < 1.0);
    }
}
