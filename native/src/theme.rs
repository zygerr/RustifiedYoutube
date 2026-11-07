use eframe::egui::{self, Color32, FontId, Rect, Sense, Stroke, Vec2, pos2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Classic,
    PurpleHaze,
    Oled,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub panel: Color32,
    pub surface: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub progress: Color32,
    pub on_accent: Color32,
    pub rail: Color32,
    pub error: Color32,
}

impl Theme {
    pub const ALL: [Self; 3] = [Self::Classic, Self::PurpleHaze, Self::Oled];

    pub fn name(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::PurpleHaze => "Purple Haze",
            Self::Oled => "OLED",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Classic => "YouTube Music's familiar dark look",
            Self::PurpleHaze => "Deep plum, violet & soft lavender",
            Self::Oled => "Pure black surfaces & crisp whites",
        }
    }

    pub fn palette(self) -> Palette {
        let rgb = Color32::from_rgb;
        match self {
            Self::Classic => Palette {
                background: rgb(3, 3, 3),
                panel: rgb(18, 18, 18),
                surface: rgb(35, 35, 35),
                hover: rgb(45, 45, 45),
                selected: rgb(32, 32, 32),
                border: rgb(35, 35, 35),
                text: rgb(241, 241, 241),
                muted: rgb(170, 170, 170),
                accent: rgb(241, 241, 241),
                progress: rgb(255, 0, 51),
                on_accent: rgb(3, 3, 3),
                rail: rgb(56, 61, 72),
                error: rgb(55, 20, 24),
            },
            Self::PurpleHaze => Palette {
                background: rgb(15, 10, 24),
                panel: rgb(23, 15, 36),
                surface: rgb(39, 27, 57),
                hover: rgb(56, 37, 78),
                selected: rgb(61, 37, 87),
                border: rgb(54, 37, 73),
                text: rgb(245, 238, 255),
                muted: rgb(179, 162, 198),
                accent: rgb(205, 167, 255),
                progress: rgb(184, 124, 255),
                on_accent: rgb(30, 13, 48),
                rail: rgb(65, 46, 84),
                error: rgb(63, 24, 51),
            },
            Self::Oled => Palette {
                background: Color32::BLACK,
                panel: Color32::BLACK,
                surface: rgb(14, 14, 14),
                hover: rgb(24, 24, 24),
                selected: rgb(22, 22, 22),
                border: rgb(28, 28, 28),
                text: rgb(240, 240, 240),
                muted: rgb(160, 160, 160),
                accent: rgb(240, 240, 240),
                progress: rgb(240, 240, 240),
                on_accent: Color32::BLACK,
                rail: rgb(40, 40, 40),
                error: rgb(35, 12, 16),
            },
        }
    }

    pub fn apply(self, ctx: &egui::Context) {
        let p = self.palette();
        let mut v = egui::Visuals::dark();
        v.panel_fill = p.background;
        v.window_fill = p.panel;
        v.window_stroke = Stroke::new(1_f32, p.border);
        v.override_text_color = Some(p.text);
        v.weak_text_color = Some(p.muted);
        v.faint_bg_color = p.surface;
        v.extreme_bg_color = p.background;
        v.code_bg_color = p.surface;
        v.hyperlink_color = p.accent;
        v.selection.bg_fill = p.selected;
        v.selection.stroke = Stroke::new(1_f32, p.accent);
        v.widgets.noninteractive.bg_fill = p.panel;
        v.widgets.noninteractive.weak_bg_fill = p.panel;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, p.border);
        v.widgets.noninteractive.fg_stroke.color = p.muted;
        v.widgets.inactive.bg_fill = p.surface;
        v.widgets.inactive.weak_bg_fill = p.surface;
        v.widgets.inactive.bg_stroke = Stroke::new(1_f32, p.border);
        v.widgets.inactive.fg_stroke.color = p.text;
        v.widgets.hovered.bg_fill = p.hover;
        v.widgets.hovered.weak_bg_fill = p.hover;
        v.widgets.hovered.bg_stroke = Stroke::new(1_f32, p.accent);
        v.widgets.hovered.fg_stroke.color = p.text;
        v.widgets.active.bg_fill = p.selected;
        v.widgets.active.weak_bg_fill = p.selected;
        v.widgets.active.bg_stroke = Stroke::new(1_f32, p.accent);
        v.widgets.active.fg_stroke.color = p.accent;
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("app-theme"), self));
        ctx.set_visuals(v);
        ctx.request_repaint();
    }
}

pub fn palette(ctx: &egui::Context) -> Palette {
    ctx.data(|data| data.get_temp::<Theme>(egui::Id::new("app-theme")))
        .unwrap_or_default()
        .palette()
}

/// A native miniature of the actual palette, including its player controls.
pub fn selector(ui: &mut egui::Ui, selected: Theme) -> Option<Theme> {
    let mut choice = None;
    let outer = palette(ui.ctx());
    let width = ((ui.available_width() - 24.) / 3.).clamp(200., 260.);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(12.);
        for theme in Theme::ALL {
            let p = theme.palette();
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 150.), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::RadioButton,
                    true,
                    selected == theme,
                    theme.name(),
                )
            });
            let painter = ui.painter();
            let preview =
                Rect::from_min_size(rect.min + Vec2::splat(4.), Vec2::new(width - 8., 94.));
            painter.rect_filled(rect, 10., outer.panel);
            painter.rect_filled(preview, 7., p.background);
            let side = Rect::from_min_size(preview.min, Vec2::new(37., preview.height()));
            painter.rect_filled(side, 7., p.panel);
            for index in 0..3 {
                let y = side.top() + 15. + index as f32 * 13.;
                painter.line_segment(
                    [pos2(side.left() + 10., y), pos2(side.right() - 8., y)],
                    Stroke::new(2_f32, if index == 0 { p.accent } else { p.border }),
                );
            }
            let x = side.right() + 12.;
            for index in 0..3 {
                let art = Rect::from_min_size(
                    pos2(x + index as f32 * 48., preview.top() + 13.),
                    Vec2::splat(34.),
                );
                painter.rect_filled(art, 4., if index == 1 { p.selected } else { p.surface });
                painter.line_segment(
                    [
                        pos2(art.left(), art.bottom() + 7.),
                        pos2(art.right() - 6., art.bottom() + 7.),
                    ],
                    Stroke::new(2_f32, p.muted),
                );
            }
            let footer = Rect::from_min_max(
                pos2(preview.left(), preview.bottom() - 22.),
                preview.right_bottom(),
            );
            painter.rect_filled(footer, 3., p.panel);
            painter.line_segment(
                [
                    footer.left_top(),
                    footer.left_top() + Vec2::new(footer.width() * 0.6, 0.),
                ],
                Stroke::new(2_f32, p.progress),
            );
            painter.circle_filled(footer.center(), 7., p.accent);
            let center = footer.center();
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + Vec2::new(-2., -3.),
                    center + Vec2::new(3., 0.),
                    center + Vec2::new(-2., 3.),
                ],
                p.on_accent,
                Stroke::NONE,
            ));
            painter.text(
                rect.min + Vec2::new(12., 108.),
                egui::Align2::LEFT_TOP,
                theme.name(),
                FontId::proportional(15.),
                outer.text,
            );
            painter.text(
                rect.min + Vec2::new(12., 130.),
                egui::Align2::LEFT_TOP,
                theme.description(),
                FontId::proportional(11.),
                outer.muted,
            );
            let active = theme == selected;
            painter.rect_stroke(
                rect,
                10.,
                Stroke::new(
                    if active { 2_f32 } else { 1_f32 },
                    if active || response.hovered() || response.has_focus() {
                        outer.accent
                    } else {
                        outer.border
                    },
                ),
                egui::StrokeKind::Inside,
            );
            if active {
                painter.circle_filled(rect.right_top() + Vec2::new(-16., 16.), 5., outer.accent);
            }
            if response.clicked() {
                choice = Some(theme);
            }
        }
    });
    choice
}
