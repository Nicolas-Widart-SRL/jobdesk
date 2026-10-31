use egui::epaint::text::{FontInsert, InsertFontFamily};
use egui::{Context, FontData, FontDefinitions, FontFamily};
use egui::epaint::text::FontPriority;

const SCHIBSTED_GROTESK: &[u8] = include_bytes!("../../assets/fonts/SchibstedGrotesk-Variable.ttf");
const ZILLA_SLAB: &[u8] = include_bytes!("../../assets/fonts/ZillaSlab-SemiBold.ttf");
const COURIER_PRIME: &[u8] = include_bytes!("../../assets/fonts/CourierPrime-Regular.ttf");
const COURIER_PRIME_BOLD: &[u8] = include_bytes!("../../assets/fonts/CourierPrime-Bold.ttf");

/// The voices in the design. Slab for headings and figures, typed for commands and paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Sans,
    Medium,
    Semibold,
    Slab,
    Typed,
    TypedBold,
}

impl Face {
    pub fn family(self) -> FontFamily {
        match self {
            Self::Sans => FontFamily::Proportional,
            Self::Typed => FontFamily::Monospace,
            Self::Medium => FontFamily::Name("medium".into()),
            Self::Semibold => FontFamily::Name("semibold".into()),
            Self::Slab => FontFamily::Name("slab".into()),
            Self::TypedBold => FontFamily::Name("typed-bold".into()),
        }
    }
}

/// The design's own faces, embedded so the app reads the same on every machine.
/// egui's bundled faces stay behind them for glyphs they lack.
pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let default_sans = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let default_mono = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    for face in [Face::Medium, Face::Semibold, Face::Slab] {
        fonts.families.insert(face.family(), default_sans.clone());
    }
    fonts.families.insert(Face::TypedBold.family(), default_mono);
    ctx.set_fonts(fonts);

    let faces = [
        ("schibsted-regular", SCHIBSTED_GROTESK, Face::Sans, Some(400.0)),
        ("schibsted-medium", SCHIBSTED_GROTESK, Face::Medium, Some(500.0)),
        ("schibsted-semibold", SCHIBSTED_GROTESK, Face::Semibold, Some(600.0)),
        ("zilla-slab", ZILLA_SLAB, Face::Slab, None),
        ("courier-prime", COURIER_PRIME, Face::Typed, None),
        ("courier-prime-bold", COURIER_PRIME_BOLD, Face::TypedBold, None),
    ];
    for (name, bytes, face, weight) in faces {
        let mut data = FontData::from_static(bytes);
        if let Some(weight) = weight {
            data.tweak.coords = egui::epaint::text::VariationCoords::new([("wght", weight)]);
        }
        ctx.add_font(FontInsert {
            name: name.to_owned(),
            data,
            families: vec![InsertFontFamily { family: face.family(), priority: FontPriority::Highest }],
        });
    }
}
