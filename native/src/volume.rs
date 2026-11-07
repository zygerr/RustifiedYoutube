use eframe::egui::{self, Event, MouseWheelUnit};

#[derive(Default)]
pub struct Wheel {
    remainder: f32,
}

impl Wheel {
    fn apply(&mut self, volume: &mut f32, units: f32, step: u8) {
        self.remainder += units;
        let notches = (self.remainder + self.remainder.signum() * 0.000001).trunc();
        self.remainder -= notches;
        if notches != 0.0 {
            *volume = (((*volume * 100.0).round() + notches * step.clamp(1, 100) as f32)
                .clamp(0.0, 100.0))
                / 100.0;
        }
    }

    pub fn over(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        volume: &mut f32,
        step: u8,
    ) -> bool {
        let before = *volume;
        if ui.rect_contains_pointer(rect) {
            let units = ui.input_mut(|input| {
                let mut units = 0.;
                input.events.retain(|event| {
                    if let Event::MouseWheel { unit, delta, .. } = event
                        && delta.y != 0.
                    {
                        units += match unit {
                            MouseWheelUnit::Point => delta.y / 40.,
                            _ => delta.y,
                        };
                        return false;
                    }
                    true
                });
                input.raw_scroll_delta.y = 0.;
                input.smooth_scroll_delta.y = 0.;
                units
            });
            self.apply(volume, units, step);
        } else {
            self.remainder = 0.;
        }
        *volume != before
    }
    pub fn control(&mut self, ui: &mut egui::Ui, volume: &mut f32, step: u8) -> bool {
        let before = *volume;
        ui.allocate_ui_with_layout(
            egui::vec2(140.0, 24.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                // Consume the wheel before egui's slider can apply its default step.
                self.over(ui, ui.max_rect(), volume, step);
                ui.label(format!("{:.0}%", *volume * 100.0));
                ui.add_sized(
                    [95.0, 22.0],
                    egui::Slider::new(volume, 0.0..=1.0)
                        .step_by(0.01)
                        .show_value(false),
                )
                .on_hover_text(format!("Scroll: ±{}% per notch", step.clamp(1, 100)));
            },
        );
        *volume != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artwork_scroll_is_consumed_only_over_the_image_and_accumulates_independently() {
        let ctx = egui::Context::default();
        let mut artwork = Wheel::default();
        let mut slider = Wheel::default();
        let mut volume = 0.5;
        let mut frame = |point, delta, unit| {
            let mut remaining = 0;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 300.),
                    )),
                    events: vec![
                        Event::PointerMoved(point),
                        Event::MouseWheel {
                            unit,
                            delta: egui::vec2(0., delta),
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            slider.control(ui, &mut volume, 7);
                            let (image, _) = ui
                                .allocate_exact_size(egui::vec2(200., 200.), egui::Sense::hover());
                            artwork.over(ui, image, &mut volume, 7);
                            remaining = ui.input(|input| {
                                input
                                    .events
                                    .iter()
                                    .filter(|event| matches!(event, Event::MouseWheel { .. }))
                                    .count()
                            });
                        });
                },
            );
            (volume, remaining)
        };
        frame(egui::pos2(100., 100.), 0., MouseWheelUnit::Point);
        for _ in 0..4 {
            assert_eq!(
                frame(egui::pos2(100., 100.), 8., MouseWheelUnit::Point),
                (0.5, 0)
            );
        }
        assert_eq!(
            frame(egui::pos2(100., 100.), 8., MouseWheelUnit::Point),
            (0.57, 0)
        );
        assert_eq!(
            frame(egui::pos2(300., 270.), 1., MouseWheelUnit::Line),
            (0.57, 1)
        );
        assert_eq!(
            frame(egui::pos2(100., 100.), -1., MouseWheelUnit::Line),
            (0.5, 0)
        );
    }
    #[test]
    fn ui_wheel_changes_volume_only_over_the_control_and_uses_one_configured_step() {
        let ctx = egui::Context::default();
        let mut wheel = Wheel::default();
        let mut volume = 0.5;
        let mut frame = |events| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 100.0),
                )),
                events,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        wheel.control(ui, &mut volume, 7);
                    });
            });
            volume
        };
        frame(vec![]);
        let scroll = |delta| Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: egui::vec2(0.0, delta),
            modifiers: egui::Modifiers::NONE,
        };
        assert!(
            (frame(vec![
                Event::PointerMoved(egui::pos2(50.0, 12.0)),
                scroll(1.0)
            ]) - 0.57)
                .abs()
                < 0.00001
        );
        assert!(
            (frame(vec![
                Event::PointerMoved(egui::pos2(300.0, 12.0)),
                scroll(1.0)
            ]) - 0.57)
                .abs()
                < 0.00001
        );
        assert!(
            (frame(vec![
                Event::PointerMoved(egui::pos2(50.0, 12.0)),
                scroll(-1.0)
            ]) - 0.50)
                .abs()
                < 0.00001
        );
    }
    #[test]
    fn configured_step_is_exact_and_clamped() {
        let mut wheel = Wheel::default();
        let mut volume = 0.5;
        wheel.apply(&mut volume, 1.0, 7);
        assert!((volume - 0.57).abs() < 0.00001);
        wheel.apply(&mut volume, -2.0, 7);
        assert!((volume - 0.43).abs() < 0.00001);
        wheel.apply(&mut volume, 1.0, 100);
        assert_eq!(volume, 1.0);
        wheel.apply(&mut volume, -1.0, 100);
        assert_eq!(volume, 0.0);
    }
    #[test]
    fn fractional_wheel_events_accumulate_one_step() {
        let mut wheel = Wheel::default();
        let mut volume = 0.5;
        for _ in 0..4 {
            wheel.apply(&mut volume, 0.2, 1);
        }
        assert_eq!(volume, 0.5);
        wheel.apply(&mut volume, 0.2, 1);
        assert_eq!(volume, 0.51);
    }
}
