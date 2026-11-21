//! Interface fonts. Font files are read from the local Windows installation
//! and never redistributed. The chosen font is the primary face; egui's
//! built-in fonts and Windows symbol/emoji fonts remain as fallbacks so styled
//! Unicode titles and emoji keep rendering whatever font is selected.
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, RichText, Vec2};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiFont {
    #[default]
    Default,
    SegoeUi,
    SegoeUiVariable,
    Calibri,
    Verdana,
    Georgia,
    Trebuchet,
    Bahnschrift,
}

impl UiFont {
    pub const ALL: [Self; 8] = [
        Self::Default,
        Self::SegoeUi,
        Self::SegoeUiVariable,
        Self::Calibri,
        Self::Verdana,
        Self::Georgia,
        Self::Trebuchet,
        Self::Bahnschrift,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::SegoeUi => "Segoe UI",
            Self::SegoeUiVariable => "Segoe UI Variable",
            Self::Calibri => "Calibri",
            Self::Verdana => "Verdana",
            Self::Georgia => "Georgia",
            Self::Trebuchet => "Trebuchet MS",
            Self::Bahnschrift => "Bahnschrift",
        }
    }

    /// The Windows font file, or `None` for egui's built-in font.
    fn file(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::SegoeUi => Some("segoeui.ttf"),
            Self::SegoeUiVariable => Some("SegUIVar.ttf"),
            Self::Calibri => Some("calibri.ttf"),
            Self::Verdana => Some("verdana.ttf"),
            Self::Georgia => Some("georgia.ttf"),
            Self::Trebuchet => Some("trebuc.ttf"),
            Self::Bahnschrift => Some("bahnschrift.ttf"),
        }
    }

    fn key(self) -> String {
        format!("ui-font-{self:?}")
    }

    /// A font family that renders in this font (used for previews).
    fn family(self) -> FontFamily {
        FontFamily::Name(self.key().into())
    }
}

fn font_directory() -> PathBuf {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts")
}

/// Installs the interface fonts with `selected` as the primary face and
/// returns which choices are available on this PC. A missing font falls back
/// to the default.
pub fn install(ctx: &egui::Context, selected: UiFont) -> Vec<UiFont> {
    install_from(ctx, selected, &font_directory())
}

fn install_from(ctx: &egui::Context, selected: UiFont, directory: &std::path::Path) -> Vec<UiFont> {
    let mut fonts = FontDefinitions::default();
    let defaults = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    // The default face keeps its own preview family; the interface family
    // starts with whichever font is selected.
    fonts
        .families
        .insert(UiFont::Default.family(), defaults.clone());
    let mut available = vec![UiFont::Default];
    for font in UiFont::ALL {
        let Some(file) = font.file() else {
            continue;
        };
        let Ok(bytes) = std::fs::read(directory.join(file)) else {
            continue;
        };
        let key = font.key();
        fonts
            .font_data
            .insert(key.clone(), Arc::new(FontData::from_owned(bytes)));
        // A preview family in this font, falling back like the interface.
        let mut family = vec![key.clone()];
        family.extend(defaults.iter().cloned());
        fonts.families.insert(font.family(), family);
        if font == selected {
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, key);
        }
        available.push(font);
    }
    let fallback_families: Vec<FontFamily> = fonts.families.keys().cloned().collect();
    for (name, file, index) in [
        ("windows-symbols", "seguisym.ttf", 0),
        ("windows-math", "cambria.ttc", 1),
        ("windows-emoji", "seguiemj.ttf", 0),
        ("windows-history", "seguihis.ttf", 0),
    ] {
        if let Ok(bytes) = std::fs::read(directory.join(file)) {
            let mut data = FontData::from_owned(bytes);
            data.index = index;
            fonts.font_data.insert(name.into(), Arc::new(data));
            for family in &fallback_families {
                fonts
                    .families
                    .entry(family.clone())
                    .or_default()
                    .push(name.into());
            }
        }
    }
    ctx.set_fonts(fonts);
    available
}

/// Font picker for Settings → Appearance. Each choice previews in its own font.
pub fn selector(ui: &mut egui::Ui, selected: UiFont, available: &[UiFont]) -> Option<UiFont> {
    let mut choice = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(10.);
        for font in UiFont::ALL {
            let installed = available.contains(&font);
            let label = RichText::new(format!("{}\nAa Bb Șș 123", font.name()))
                .family(if installed {
                    font.family()
                } else {
                    UiFont::Default.family()
                })
                .size(16.);
            let colors = crate::theme::palette(ui.ctx());
            let active = selected == font;
            let response = ui
                .add_enabled(
                    installed,
                    egui::Button::new(label)
                        .min_size(Vec2::new(170., 58.))
                        .corner_radius(8)
                        .fill(if active {
                            colors.selected
                        } else {
                            colors.panel
                        })
                        .stroke(egui::Stroke::new(
                            if active { 2_f32 } else { 1_f32 },
                            if active { colors.text } else { colors.border },
                        )),
                )
                .on_disabled_hover_text("This font isn't installed on this PC.");
            if response.clicked() && selected != font {
                choice = Some(font);
            }
        }
    });
    choice
}

#[cfg(test)]
mod tests {
    use super::*;
    const STYLED: &str = "𝗡𝗜𝗚𝗛𝗧 𝗦𝗛𝗜𝗙𝗧 🚀 𝗔𝗟𝗟 𝗧𝗛𝗘 𝗖𝗜𝗧𝗬 𝗟𝗜𝗚𝗛𝗧𝗦 ▪ 𝗢𝗙𝗜𝗖𝗜𝗔𝗟 𝗔𝗨𝗗𝗜𝗢";

    #[test]
    fn supplied_styled_title_has_real_glyphs() {
        let ctx = eframe::egui::Context::default();
        install(&ctx, UiFont::Default);
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts_mut(|fonts| {
                assert!(fonts.has_glyphs(&eframe::egui::FontId::proportional(14.0), STYLED));
            });
        });
    }

    #[test]
    fn every_installed_font_becomes_primary_and_keeps_emoji_and_styled_fallbacks() {
        let ctx = eframe::egui::Context::default();
        let available = install(&ctx, UiFont::Default);
        assert_eq!(available[0], UiFont::Default);
        for font in available.into_iter().skip(1) {
            install(&ctx, font);
            let _ = ctx.run(Default::default(), |ctx| {
                let primary = ctx.fonts(|fonts| {
                    fonts.definitions().families[&FontFamily::Proportional][0].clone()
                });
                assert_eq!(primary, font.key(), "{font:?} is not the primary face");
                ctx.fonts_mut(|fonts| {
                    for family in [FontFamily::Proportional, font.family()] {
                        assert!(
                            fonts.has_glyphs(
                                &eframe::egui::FontId::new(14.0, family),
                                &format!("{STYLED} Cântec de toamnă ȘȚ")
                            ),
                            "{font:?} lost fallback glyphs"
                        );
                    }
                });
            });
        }
    }

    #[test]
    fn the_default_preview_keeps_the_built_in_face_when_another_font_is_selected() {
        let ctx = eframe::egui::Context::default();
        let builtin = FontDefinitions::default().families[&FontFamily::Proportional][0].clone();
        install(&ctx, UiFont::SegoeUi);
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts(|fonts| {
                assert_eq!(
                    fonts.definitions().families[&UiFont::Default.family()][0],
                    builtin
                );
            });
        });
    }
    #[test]
    fn a_missing_font_falls_back_to_the_default_face() {
        let ctx = eframe::egui::Context::default();
        let defaults = FontDefinitions::default().families[&FontFamily::Proportional][0].clone();
        let empty = std::env::temp_dir().join(format!("rustified-no-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&empty).unwrap();
        let available = install_from(&ctx, UiFont::SegoeUi, &empty);
        let _ = std::fs::remove_dir_all(&empty);
        assert_eq!(available, vec![UiFont::Default]);
        let _ = ctx.run(Default::default(), |ctx| {
            let primary = ctx
                .fonts(|fonts| fonts.definitions().families[&FontFamily::Proportional][0].clone());
            assert_eq!(primary, defaults);
        });
    }
}
