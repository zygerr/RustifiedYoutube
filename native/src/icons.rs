use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Pause,
    Next,
    Previous,
    Heart,
    Queue,
    Volume,
    Home,
    Library,
    Settings,
    Plus,
    Repeat,
    Shuffle,
    Search,
}

pub fn draw(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
    let center = rect.center();
    let point = |x, y| Pos2::new(center.x + x, center.y + y);
    let stroke = Stroke::new(1.7_f32, color);
    let line = |a, b| painter.line_segment([a, b], stroke);
    match icon {
        Icon::Search => {
            painter.circle_stroke(point(-2., -2.), 6., stroke);
            line(point(3., 3.), point(8., 8.));
        }
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![point(-5., -8.), point(8., 0.), point(-5., 8.)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            for x in [-4., 4.] {
                painter.rect_filled(
                    egui::Rect::from_center_size(point(x, 0.), Vec2::new(3., 16.)),
                    1.,
                    color,
                );
            }
        }
        Icon::Next | Icon::Previous => {
            let d = if matches!(icon, Icon::Next) { 1. } else { -1. };
            painter.add(egui::Shape::convex_polygon(
                vec![point(-6. * d, -7.), point(4. * d, 0.), point(-6. * d, 7.)],
                color,
                Stroke::NONE,
            ));
            line(point(7. * d, -7.), point(7. * d, 7.));
        }
        Icon::Heart => {
            let points = vec![
                point(0., 8.),
                point(-8., 0.),
                point(-8., -4.),
                point(-5., -7.),
                point(-2., -7.),
                point(0., -4.),
                point(2., -7.),
                point(5., -7.),
                point(8., -4.),
                point(8., 0.),
                point(0., 8.),
            ];
            painter.add(egui::Shape::line(points, stroke));
        }
        Icon::Plus => {
            line(point(-7., 0.), point(7., 0.));
            line(point(0., -7.), point(0., 7.));
        }
        Icon::Queue | Icon::Library => {
            for y in [-6., 0., 6.] {
                line(point(-7., y), point(7., y));
            }
        }
        Icon::Home => {
            painter.add(egui::Shape::closed_line(
                vec![
                    point(-8., -1.),
                    point(0., -8.),
                    point(8., -1.),
                    point(6., -1.),
                    point(6., 8.),
                    point(-6., 8.),
                    point(-6., -1.),
                ],
                stroke,
            ));
        }
        Icon::Volume => {
            painter.add(egui::Shape::closed_line(
                vec![
                    point(-8., -3.),
                    point(-4., -3.),
                    point(1., -7.),
                    point(1., 7.),
                    point(-4., 3.),
                    point(-8., 3.),
                ],
                stroke,
            ));
            line(point(5., -5.), point(8., 0.));
            line(point(8., 0.), point(5., 5.));
        }
        Icon::Repeat => {
            line(point(-7., -4.), point(7., -4.));
            line(point(7., -4.), point(3., -8.));
            line(point(7., 4.), point(-7., 4.));
            line(point(-7., 4.), point(-3., 8.));
        }
        Icon::Shuffle => {
            for direction in [-1.0, 1.0] {
                ui_line(
                    painter,
                    &[
                        point(-8., -5. * direction),
                        point(-3., -5. * direction),
                        point(3., 5. * direction),
                        point(8., 5. * direction),
                    ],
                    stroke,
                );
                line(point(8., 5. * direction), point(4., 1. * direction));
                line(point(8., 5. * direction), point(4., 9. * direction));
            }
        }
        Icon::Settings => {
            painter.circle_stroke(center, 7., stroke);
            painter.circle_stroke(center, 2., stroke);
            for a in 0..8 {
                let angle = a as f32 * std::f32::consts::TAU / 8.;
                let v = Vec2::angled(angle);
                line(center + v * 7., center + v * 10.);
            }
        }
    }
}

fn ui_line(painter: &egui::Painter, points: &[Pos2], stroke: Stroke) {
    painter.add(egui::Shape::line(points.to_vec(), stroke));
}

pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    size: f32,
    accent: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if accent {
        ui.painter().circle_filled(
            rect.center(),
            size / 2. - 1.,
            Color32::from_rgb(241, 241, 241),
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 8., Color32::from_rgb(44, 47, 56));
    }
    draw(
        ui.painter(),
        rect,
        icon,
        if accent {
            Color32::from_rgb(3, 3, 3)
        } else {
            ui.visuals().text_color()
        },
    );
    response.on_hover_text(label)
}
