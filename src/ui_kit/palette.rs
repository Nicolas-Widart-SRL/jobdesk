use egui::{Color32, Context, CornerRadius, CursorIcon, FontFamily, Shadow, Stroke, TextStyle, Theme, Ui, Visuals};

use crate::ui_kit::tokens::{BUTTON_PADDING, HAIRLINE, ITEM_GAP, RADIUS_BUTTON, RADIUS_PANEL, ROW_GAP, Type, WINDOW_SHADOW};

/// The Ledger tokens from `project/app.css`, one set per theme.
/// Depth comes from three flat planes: desk (`bg`), folder (`panel`), sheet (`sheet`).
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub sheet: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub line: Color32,
    pub field: Color32,
    pub off: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub primary: Color32,
    pub on_primary: Color32,
    pub drafted: Color32,
    pub active: Color32,
    pub interview: Color32,
    pub offer: Color32,
    pub hired: Color32,
    pub closed: Color32,
    pub turn: Color32,
    pub on_turn: Color32,
    pub page: Color32,
    pub page_line: Color32,
    pub page_head: Color32,
    /// The fixed navy frame, the same in both themes.
    pub chrome: Color32,
    pub chrome_tab: Color32,
    pub chrome_ink: Color32,
    pub chrome_muted: Color32,
    pub chrome_line: Color32,
    /// The failed-source square on a chrome tab.
    pub chrome_alert: Color32,
    /// `turn`, the stamp ink, where it sits on the navy frame.
    pub turn_on_chrome: Color32,
    pub on_turn_chrome: Color32,
    /// The fill of deal, job slip and application cards.
    pub card: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

const DARK: Palette = Palette {
    bg: hex(0x0d1a2b),
    sheet: hex(0x13243a),
    panel: hex(0x1a2f49),
    raised: hex(0x233b59),
    line: hex(0x33506f),
    field: hex(0x0d1a2b),
    off: hex(0x33506f),
    text: hex(0xe6edf5),
    muted: hex(0xb3c4d6),
    faint: hex(0x9eb2c7),
    primary: hex(0xe6edf5),
    on_primary: hex(0x0d1a2b),
    drafted: hex(0xafc2cb),
    active: hex(0x8cc2f7),
    interview: hex(0xedbe5e),
    offer: hex(0xcdbffc),
    hired: hex(0x7bdda3),
    closed: hex(0xf7a697),
    turn: hex(0xf48cc0),
    on_turn: hex(0x0d1a2b),
    page: hex(0xf7f9fc),
    page_line: hex(0xc7d3e0),
    page_head: hex(0x35495f),
    chrome: hex(0x0d1a2b),
    chrome_tab: hex(0x1a2f49),
    chrome_ink: hex(0xe6edf5),
    chrome_muted: hex(0xb3c4d6),
    chrome_line: hex(0x33506f),
    chrome_alert: hex(0xf7a697),
    turn_on_chrome: hex(0xf48cc0),
    on_turn_chrome: hex(0x0d1a2b),
    card: hex(0x1a2f49),
};

const LIGHT: Palette = Palette {
    bg: hex(0xbdccdc),
    sheet: hex(0xeff3f8),
    panel: hex(0xdfe7f0),
    raised: hex(0xd0dce9),
    line: hex(0xaebfd1),
    field: hex(0xfafcfe),
    off: hex(0xc2d0df),
    text: hex(0x12263f),
    muted: hex(0x35495f),
    faint: hex(0x44586f),
    primary: hex(0x12263f),
    on_primary: hex(0xeff3f8),
    drafted: hex(0x435863),
    active: hex(0x1657a8),
    interview: hex(0x7a4c06),
    offer: hex(0x5535a6),
    hired: hex(0x17643a),
    closed: hex(0x9a2a1a),
    turn: hex(0xb0246a),
    on_turn: hex(0xffffff),
    page: hex(0xffffff),
    page_line: hex(0xd0dae6),
    page_head: hex(0x35495f),
    chrome: hex(0x12263f),
    chrome_tab: hex(0x233b59),
    chrome_ink: hex(0xe6edf5),
    chrome_muted: hex(0xb3c4d6),
    chrome_line: hex(0x33506f),
    chrome_alert: hex(0xf7a697),
    turn_on_chrome: hex(0xf48cc0),
    on_turn_chrome: hex(0x0d1a2b),
    card: hex(0xffffff),
};

impl Palette {
    pub fn of(ui: &Ui) -> Self {
        Self::for_theme(ui.ctx().theme())
    }

    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => DARK,
            Theme::Light => LIGHT,
        }
    }
}

/// Colour is for job status only, except `Turn`, the stamp ink for what waits on you, such as a run's turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Drafted,
    Active,
    Interview,
    Offer,
    Hired,
    Closed,
    Turn,
}

impl Tone {
    pub(in crate::ui_kit) fn color(self, p: &Palette) -> Color32 {
        match self {
            Self::Plain => p.muted,
            Self::Drafted => p.drafted,
            Self::Active => p.active,
            Self::Interview => p.interview,
            Self::Offer => p.offer,
            Self::Hired => p.hired,
            Self::Closed => p.closed,
            Self::Turn => p.turn,
        }
    }
}

/// How strongly text or a mark reads: a neutral step, or a job status. `Status(Tone::Turn)` is the stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    Text,
    Muted,
    Faint,
    Status(Tone),
}

impl From<Tone> for Ink {
    fn from(tone: Tone) -> Self {
        Self::Status(tone)
    }
}

impl Ink {
    pub(in crate::ui_kit) fn color(self, p: &Palette) -> Color32 {
        match self {
            Self::Text => p.text,
            Self::Muted => p.muted,
            Self::Faint => p.faint,
            Self::Status(tone) => tone.color(p),
        }
    }
}

pub fn install_theme(ctx: &Context) {
    for theme in [Theme::Dark, Theme::Light] {
        ctx.set_visuals_of(theme, visuals(theme));
        ctx.style_mut_of(theme, |style| {
            style.text_styles.insert(TextStyle::Body, Type::BODY.font());
            style.text_styles.insert(TextStyle::Button, Type::BUTTON.font());
            style.text_styles.insert(TextStyle::Small, Type::SMALL.font());
            style.text_styles.insert(TextStyle::Monospace, egui::FontId::new(Type::MONO.size, FontFamily::Monospace));
            style.text_styles.insert(TextStyle::Heading, Type::H1.font());
            style.spacing.item_spacing = egui::vec2(ROW_GAP, ITEM_GAP);
            style.spacing.button_padding = egui::vec2(BUTTON_PADDING, 5.0);
            style.spacing.interact_size.y = 26.0;
            style.spacing.scroll.fade.strength = 0.0;
        });
    }
}

fn visuals(theme: Theme) -> Visuals {
    let p = Palette::for_theme(theme);
    let mut visuals = match theme {
        Theme::Dark => Visuals::dark(),
        Theme::Light => Visuals::light(),
    };
    let radius = CornerRadius::same(RADIUS_BUTTON);
    let window_shadow = Shadow { offset: [WINDOW_SHADOW, WINDOW_SHADOW], blur: 0, spread: 0, color: p.line };
    visuals.override_text_color = Some(p.text);
    visuals.panel_fill = p.sheet;
    visuals.window_fill = p.sheet;
    visuals.window_stroke = Stroke::new(HAIRLINE, p.text);
    visuals.window_corner_radius = CornerRadius::same(RADIUS_PANEL);
    visuals.window_shadow = window_shadow;
    visuals.menu_corner_radius = CornerRadius::same(RADIUS_PANEL);
    visuals.popup_shadow = window_shadow;
    visuals.extreme_bg_color = p.field;
    visuals.faint_bg_color = p.raised;
    visuals.code_bg_color = p.field;
    visuals.hyperlink_color = p.active;
    visuals.warn_fg_color = p.interview;
    visuals.error_fg_color = p.closed;
    visuals.selection.bg_fill = p.raised;
    visuals.selection.stroke = Stroke::new(HAIRLINE, p.text);
    visuals.interact_cursor = Some(CursorIcon::PointingHand);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(HAIRLINE, p.line);
    visuals.widgets.noninteractive.corner_radius = radius;
    for (state, fill, control) in [
        (&mut visuals.widgets.inactive, Color32::TRANSPARENT, p.field),
        (&mut visuals.widgets.hovered, p.raised, p.raised),
        (&mut visuals.widgets.active, p.raised, p.raised),
        (&mut visuals.widgets.open, p.raised, p.raised),
    ] {
        state.bg_fill = control;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(HAIRLINE, p.line);
        state.fg_stroke = Stroke::new(HAIRLINE, p.text);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Color32) -> f64 {
        let channel = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn stamp_and_chrome_inks_read_on_their_fills() {
        for theme in [Theme::Dark, Theme::Light] {
            let p = Palette::for_theme(theme);
            for (name, ink, fill) in [
                ("on_turn on turn", p.on_turn, p.turn),
                ("on_turn_chrome on turn_on_chrome", p.on_turn_chrome, p.turn_on_chrome),
                ("chrome_ink on chrome", p.chrome_ink, p.chrome),
                ("chrome_muted on chrome", p.chrome_muted, p.chrome),
                ("chrome_ink on chrome_tab", p.chrome_ink, p.chrome_tab),
                ("chrome_muted on chrome_line", p.chrome_muted, p.chrome_line),
            ] {
                let ratio = contrast(ink, fill);
                assert!(ratio >= 4.5, "{name} in {theme:?} reads at {ratio:.2}:1, under 4.5:1");
            }
        }
    }
}
