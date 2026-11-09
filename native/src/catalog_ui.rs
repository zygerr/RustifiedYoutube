use crate::icons::{self, Icon};
use eframe::egui::{self, Color32, FontId, Rect, Response, Sense, Vec2, pos2};
use rustified_core::model::Track;

pub const CARD_ART: f32 = 180.;
pub const CARD_HEIGHT: f32 = 256.;
pub const CARD_GAP: f32 = 24.;
pub const ROW_ART: f32 = 48.;
pub const ROW_HEIGHT: f32 = 64.;

pub fn columns(width: f32, item_width: f32, gap: f32) -> usize {
    ((width + gap) / (item_width + gap)).floor().max(1.) as usize
}

fn crop_uv(size: Vec2) -> Rect {
    let ratio = size.x.max(1.) / size.y.max(1.);
    let extent = if ratio > 1. {
        Vec2::new(1. / ratio, 1.)
    } else {
        Vec2::new(1., ratio)
    };
    Rect::from_center_size(pos2(0.5, 0.5), extent)
}
pub fn cover(ui: &egui::Ui, rect: Rect, texture: Option<&egui::TextureHandle>) {
    if let Some(texture) = texture {
        egui::Image::new(texture)
            .uv(crop_uv(texture.size_vec2()))
            .corner_radius(3)
            .paint_at(ui, rect);
    } else {
        ui.painter()
            .rect_filled(rect, 3., crate::theme::palette(ui.ctx()).surface);
        let colors = crate::theme::palette(ui.ctx());
        let radius = (rect.width() * 0.18).clamp(7., 30.);
        ui.painter().circle_stroke(
            rect.center(),
            radius,
            egui::Stroke::new(1.5_f32, colors.border),
        );
        ui.painter().circle_filled(rect.center(), 3., colors.muted);
    }
}

/// Static placeholders reserve the final catalog geometry without an animated
/// spinner or a layout jump when artwork and metadata arrive.
pub fn skeleton_rows(ui: &mut egui::Ui, rows: usize) {
    let colors = crate::theme::palette(ui.ctx());
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.;
        for _ in 0..rows {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::hover());
            let image = Rect::from_min_size(rect.min + Vec2::splat(8.), Vec2::splat(ROW_ART));
            ui.painter().rect_filled(image, 3., colors.surface);
            for (y, width) in [(15., 0.48), (38., 0.30)] {
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        pos2(image.right() + 14., rect.top() + y),
                        Vec2::new((rect.width() - 92.).max(1.) * width, 10.),
                    ),
                    3.,
                    colors.surface,
                );
            }
        }
    });
}

pub fn skeleton_shelf(ui: &mut egui::Ui) {
    let colors = crate::theme::palette(ui.ctx());
    let count = columns(ui.available_width(), CARD_ART, CARD_GAP);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = CARD_GAP;
        for _ in 0..count {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(CARD_ART, CARD_HEIGHT), Sense::hover());
            ui.painter().rect_filled(
                Rect::from_min_size(rect.min, Vec2::splat(CARD_ART)),
                3.,
                colors.surface,
            );
            for (y, width) in [(193., CARD_ART * 0.88), (215., CARD_ART * 0.64)] {
                ui.painter().rect_filled(
                    Rect::from_min_size(rect.min + Vec2::new(0., y), Vec2::new(width, 10.)),
                    3.,
                    colors.surface,
                );
            }
        }
    });
}
fn label(ui: &egui::Ui, rect: Rect, text: &str, size: f32, color: Color32, lines: usize) {
    let mut job = egui::text::LayoutJob::simple(
        text.to_string(),
        FontId::proportional(size),
        color,
        rect.width(),
    );
    job.wrap.max_rows = lines;
    job.wrap.break_anywhere = false;
    job.break_on_newline = false;
    let galley = ui.painter().layout_job(job);
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(rect.min, galley, color);
}

pub fn card(
    ui: &mut egui::Ui,
    id: egui::Id,
    title: &str,
    subtitle: &str,
    texture: Option<&egui::TextureHandle>,
    action: &str,
) -> Response {
    let colors = crate::theme::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(CARD_ART, CARD_HEIGHT), Sense::hover());
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_text(format!("{title}\n{subtitle}"));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, action));
    if ui.is_rect_visible(rect) {
        let image = Rect::from_min_size(rect.min, Vec2::splat(CARD_ART));
        cover(ui, image, texture);
        if response.hovered() || response.has_focus() {
            ui.painter()
                .rect_filled(image, 3., Color32::from_black_alpha(65));
            let center = image.right_bottom() - Vec2::splat(28.);
            ui.painter().circle_filled(center, 22., colors.text);
            icons::draw(
                ui.painter(),
                Rect::from_center_size(center, Vec2::splat(24.)),
                Icon::Play,
                colors.background,
            );
        }
        label(
            ui,
            Rect::from_min_size(
                pos2(rect.left(), image.bottom() + 10.),
                Vec2::new(CARD_ART, 36.),
            ),
            title,
            15.,
            colors.text,
            2,
        );
        label(
            ui,
            Rect::from_min_size(
                pos2(rect.left(), image.bottom() + 51.),
                Vec2::new(CARD_ART, 20.),
            ),
            subtitle,
            13.,
            colors.muted,
            1,
        );
    }
    response
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SongAction {
    Play,
    Like,
    Add,
}

pub fn song_row(
    ui: &mut egui::Ui,
    id: egui::Id,
    track: &Track,
    texture: Option<&egui::TextureHandle>,
    active: bool,
    actions: bool,
) -> Option<SongAction> {
    let colors = crate::theme::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(1.), ROW_HEIGHT),
        Sense::hover(),
    );
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_text(format!("{}\n{}", track.title, track.artist));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("Play {}", track.title),
        )
    });
    let mut action = None;
    if ui.is_rect_visible(rect) {
        if active || response.hovered() || response.has_focus() {
            ui.painter().rect_filled(
                rect,
                4.,
                if active {
                    colors.selected
                } else {
                    colors.hover
                },
            );
        }
        let image = Rect::from_min_size(rect.min + Vec2::new(8., 8.), Vec2::splat(ROW_ART));
        cover(ui, image, texture);
        if response.hovered() {
            ui.painter()
                .rect_filled(image, 3., Color32::from_black_alpha(130));
            icons::draw(ui.painter(), image, Icon::Play, colors.text);
        }
        let reserve = if actions {
            136.
        } else if rect.width() > 360. {
            58.
        } else {
            8.
        };
        let width = (rect.width() - ROW_ART - 28. - reserve).max(20.);
        label(
            ui,
            Rect::from_min_size(
                pos2(image.right() + 14., rect.top() + 12.),
                Vec2::new(width, 22.),
            ),
            &track.title,
            14.,
            colors.text,
            1,
        );
        label(
            ui,
            Rect::from_min_size(
                pos2(image.right() + 14., rect.top() + 36.),
                Vec2::new(width, 18.),
            ),
            &track.artist,
            12.,
            colors.muted,
            1,
        );
        if actions {
            for (offset, icon, name, result) in [
                (34., Icon::Plus, "Add to queue", SongAction::Add),
                (
                    70.,
                    Icon::Heart,
                    if track.liked {
                        "Unlike song"
                    } else {
                        "Like song"
                    },
                    SongAction::Like,
                ),
            ] {
                let button = Rect::from_center_size(
                    pos2(rect.right() - offset, rect.center().y),
                    Vec2::splat(28.),
                );
                let response = ui
                    .interact(button, id.with(name), Sense::click())
                    .on_hover_text(name);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name)
                });
                if response.hovered() {
                    ui.painter()
                        .circle_filled(button.center(), 15., colors.hover);
                }
                icons::draw(
                    ui.painter(),
                    button,
                    icon,
                    if track.liked && result == SongAction::Like {
                        colors.text
                    } else {
                        colors.muted
                    },
                );
                if response.clicked() {
                    action = Some(result);
                }
            }
        }
        if actions || rect.width() > 360. {
            label(
                ui,
                Rect::from_min_size(
                    pos2(
                        rect.right() - if actions { 130. } else { 54. },
                        rect.top() + 24.,
                    ),
                    Vec2::new(48., 18.),
                ),
                &track.duration,
                12.,
                colors.muted,
                1,
            );
        }
    }
    action.or_else(|| response.clicked().then_some(SongAction::Play))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn row_actions_do_not_also_start_playback() {
        let ctx = egui::Context::default();
        let track = Track {
            title: "A song".into(),
            artist: "An artist".into(),
            ..Default::default()
        };
        let frame = |events| {
            let mut action = None;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(600., 80.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            action = song_row(ui, ui.id().with("row"), &track, None, false, true);
                        });
                },
            );
            action
        };
        let pointer = |x, pressed| egui::Event::PointerButton {
            pos: pos2(x, 32.),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![]);
        frame(vec![
            egui::Event::PointerMoved(pos2(566., 32.)),
            pointer(566., true),
        ]);
        assert_eq!(frame(vec![pointer(566., false)]), Some(SongAction::Add));
        frame(vec![
            egui::Event::PointerMoved(pos2(530., 32.)),
            pointer(530., true),
        ]);
        assert_eq!(frame(vec![pointer(530., false)]), Some(SongAction::Like));
        frame(vec![
            egui::Event::PointerMoved(pos2(150., 32.)),
            pointer(150., true),
        ]);
        assert_eq!(frame(vec![pointer(150., false)]), Some(SongAction::Play));
    }
    #[test]
    fn wide_and_portrait_art_are_center_cropped_without_stretching() {
        let wide = crop_uv(Vec2::new(320., 180.));
        assert_eq!(wide.center(), pos2(0.5, 0.5));
        assert!((wide.width() - 0.5625).abs() < 0.001);
        assert_eq!(wide.height(), 1.);
        let tall = crop_uv(Vec2::new(180., 320.));
        assert_eq!(tall.center(), pos2(0.5, 0.5));
        assert_eq!(tall.width(), 1.);
        assert!((tall.height() - 0.5625).abs() < 0.001);
    }
    #[test]
    fn loading_wide_and_square_cards_have_identical_layout_even_with_long_titles() {
        let ctx = egui::Context::default();
        let mut rectangles = Vec::new();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1000., 600.),
                )),
                ..Default::default()
            },
            |ctx| {
                let wide = ctx.load_texture(
                    "wide",
                    egui::ColorImage::filled([32, 18], Color32::WHITE),
                    Default::default(),
                );
                let square = ctx.load_texture(
                    "square",
                    egui::ColorImage::filled([32, 32], Color32::WHITE),
                    Default::default(),
                );
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal_top(|ui| {
                        for (index, texture) in
                            [None, Some(&wide), Some(&square)].into_iter().enumerate()
                        {
                            rectangles.push(
                                card(
                                    ui,
                                    ui.id().with(index),
                                    &"A long title ".repeat(30),
                                    "Artist",
                                    texture,
                                    "Play",
                                )
                                .rect,
                            );
                        }
                    });
                });
            },
        );
        for rect in &rectangles {
            assert_eq!(rect.size(), Vec2::new(CARD_ART, CARD_HEIGHT));
            assert_eq!(rect.top(), rectangles[0].top());
        }
    }
}
