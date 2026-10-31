use egui::{Color32, Context, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Theme, Ui, Visuals};

use crate::ui_kit::semibold;

/// The Graphite tokens from the design, one set per theme.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub line: Color32,
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
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

const DARK: Palette = Palette {
    bg: hex(0x0f1114),
    panel: hex(0x16191d),
    raised: hex(0x20242a),
    line: hex(0x2c3138),
    text: hex(0xe8eaed),
    muted: hex(0xa9b0ba),
    faint: hex(0x8b939e),
    primary: hex(0xe8eaed),
    on_primary: hex(0x0f1114),
    drafted: hex(0xa3b1c4),
    active: hex(0x6fb0fb),
    interview: hex(0xfbbf24),
    offer: hex(0xc9bbfd),
    hired: hex(0x4ade80),
    closed: hex(0xfb8a8a),
};

const LIGHT: Palette = Palette {
    bg: hex(0xf4f5f6),
    panel: hex(0xffffff),
    raised: hex(0xeceef0),
    line: hex(0xd9dce0),
    text: hex(0x16191d),
    muted: hex(0x4f5660),
    faint: hex(0x5f6670),
    primary: hex(0x16191d),
    on_primary: hex(0xffffff),
    drafted: hex(0x475569),
    active: hex(0x1d4ed8),
    interview: hex(0x92400e),
    offer: hex(0x6d28d9),
    hired: hex(0x15803d),
    closed: hex(0xb91c1c),
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

pub fn install_theme(ctx: &Context) {
    for theme in [Theme::Dark, Theme::Light] {
        ctx.set_visuals_of(theme, visuals(theme));
        ctx.style_mut_of(theme, |style| {
            style.text_styles.insert(TextStyle::Body, FontId::proportional(13.0));
            style.text_styles.insert(TextStyle::Button, FontId::proportional(13.0));
            style.text_styles.insert(TextStyle::Small, FontId::proportional(11.0));
            style.text_styles.insert(TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace));
            style.text_styles.insert(TextStyle::Heading, FontId::new(18.0, semibold()));
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 5.0);
            style.spacing.interact_size.y = 26.0;
        });
    }
}

fn visuals(theme: Theme) -> Visuals {
    let p = Palette::for_theme(theme);
    let mut visuals = match theme {
        Theme::Dark => Visuals::dark(),
        Theme::Light => Visuals::light(),
    };
    let radius = CornerRadius::same(6);
    visuals.override_text_color = Some(p.text);
    visuals.panel_fill = p.bg;
    visuals.window_fill = p.panel;
    visuals.extreme_bg_color = p.bg;
    visuals.faint_bg_color = p.raised;
    visuals.hyperlink_color = p.active;
    visuals.selection.bg_fill = p.raised;
    visuals.selection.stroke = Stroke::new(1.0, p.text);
    visuals.window_stroke = Stroke::new(1.0, p.line);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    visuals.widgets.noninteractive.corner_radius = radius;
    for (state, stroke) in [
        (&mut visuals.widgets.inactive, p.line),
        (&mut visuals.widgets.hovered, p.faint),
        (&mut visuals.widgets.active, p.faint),
        (&mut visuals.widgets.open, p.faint),
    ] {
        state.bg_fill = p.raised;
        state.weak_bg_fill = p.raised;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.fg_stroke = Stroke::new(1.0, p.text);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    visuals
}
