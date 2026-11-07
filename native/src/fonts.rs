use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::{path::PathBuf, sync::Arc};

pub fn install(ctx: &egui::Context) {
    let directory = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");
    let mut fonts = FontDefinitions::default();
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
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.into());
            }
        }
    }
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    #[test]
    fn supplied_styled_title_has_real_glyphs() {
        let ctx = eframe::egui::Context::default();
        super::install(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts_mut(|fonts| {
                assert!(fonts.has_glyphs(
                    &eframe::egui::FontId::proportional(14.0),
                    "𝗡𝗜𝗚𝗛𝗧 𝗦𝗛𝗜𝗙𝗧 🚀 𝗔𝗟𝗟 𝗧𝗛𝗘 𝗖𝗜𝗧𝗬 𝗟𝗜𝗚𝗛𝗧𝗦 ▪ 𝗢𝗙𝗜𝗖𝗜𝗔𝗟 𝗔𝗨𝗗𝗜𝗢"
                ));
            });
        });
    }
}
