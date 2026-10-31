use std::fs;

use egui::epaint::text::{FontInsert, InsertFontFamily};
use egui::{Context, FontData, FontDefinitions, FontFamily};
use egui::epaint::text::FontPriority;

const SEMIBOLD: &str = "semibold";

/// System faces keep the app native on each platform and add nothing to the binary.
#[cfg(target_os = "macos")]
const SYSTEM_FONTS: Candidates = Candidates {
    sans: &["/System/Library/Fonts/SFNS.ttf"],
    mono: &["/System/Library/Fonts/SFNSMono.ttf", "/System/Library/Fonts/Menlo.ttc"],
    bold: &["/System/Library/Fonts/SFNS.ttf"],
};

#[cfg(target_os = "windows")]
const SYSTEM_FONTS: Candidates = Candidates {
    sans: &["C:\\Windows\\Fonts\\segoeui.ttf"],
    mono: &["C:\\Windows\\Fonts\\consola.ttf"],
    bold: &["C:\\Windows\\Fonts\\seguisb.ttf"],
};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const SYSTEM_FONTS: Candidates = Candidates { sans: &[], mono: &[], bold: &[] };

struct Candidates {
    sans: &'static [&'static str],
    mono: &'static [&'static str],
    bold: &'static [&'static str],
}

pub fn semibold() -> FontFamily {
    FontFamily::Name(SEMIBOLD.into())
}

fn first_readable(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| fs::read(path).ok())
}

/// Falls back to egui's bundled faces when a system font is missing.
pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let default_sans = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.insert(semibold(), default_sans);
    ctx.set_fonts(fonts);

    let faces = [
        ("system-sans", SYSTEM_FONTS.sans, FontFamily::Proportional, None),
        ("system-mono", SYSTEM_FONTS.mono, FontFamily::Monospace, None),
        ("system-semibold", SYSTEM_FONTS.bold, semibold(), Some(600.0)),
    ];
    for (name, paths, family, weight) in faces {
        let Some(bytes) = first_readable(paths) else { continue };
        let mut data = FontData::from_owned(bytes);
        if let Some(weight) = weight {
            data.tweak.coords = egui::epaint::text::VariationCoords::new([("wght", weight)]);
        }
        ctx.add_font(FontInsert {
            name: name.to_owned(),
            data,
            families: vec![InsertFontFamily { family, priority: FontPriority::Highest }],
        });
    }
}
