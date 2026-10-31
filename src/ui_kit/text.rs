use std::sync::Arc;

use egui::{Align2, Color32, Galley, Rect, Response, RichText, TextWrapMode, Ui, WidgetText};

use crate::ui_kit::Palette;
use crate::ui_kit::palette::Ink;
use crate::ui_kit::tokens::Type;

/// Text in a named type style. The way a screen asks for anything beyond body text.
pub fn rich(ui: &Ui, text: impl Into<String>, style: Type, ink: impl Into<Ink>) -> RichText {
    RichText::new(text).font(style.font()).color(ink.into().color(&Palette::of(ui)))
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::BODY, Ink::Muted);
    ui.label(text)
}

pub fn faint_mono(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::MONO, Ink::Faint);
    ui.label(text)
}

pub fn strong(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::STRONG, Ink::Text);
    ui.label(text)
}

/// Something went wrong. Closed is the one status that reads as failure.
pub fn alert(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::BODY, crate::ui_kit::Tone::Closed);
    ui.label(text)
}

pub fn table_head(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::TABLE_HEAD, Ink::Muted);
    ui.label(text)
}

pub fn score(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::SCORE, Ink::Text);
    ui.label(text)
}

/// A large count, read at a glance.
pub fn figure(ui: &mut Ui, text: impl Into<String>, ink: impl Into<Ink>) -> Response {
    let text = rich(ui, text, Type::FIGURE, ink);
    ui.label(text)
}

/// A strong line over a muted one, each cut to one line, the pair centred in the height it has. Such as a role in a table row.
pub fn title_over(ui: &mut Ui, title: &str, detail: &str) -> Response {
    let p = Palette::of(ui);
    let width = ui.available_width();
    let title = line(ui, title, Type::STRONG, p.text, width);
    let detail = line(ui, detail, Type::BODY, p.muted, width);
    ui.vertical(|ui| {
        ui.add_space(((ui.available_height() - title.size().y - detail.size().y) / 2.0).max(0.0));
        ui.label(title);
        ui.label(detail);
    })
    .response
}

/// A single line cut to `width`, for widgets that paint their own text.
pub(in crate::ui_kit) fn line(ui: &Ui, text: &str, style: Type, color: Color32, width: f32) -> Arc<Galley> {
    let text = RichText::new(text).font(style.font()).color(color);
    WidgetText::from(text).into_galley(ui, Some(TextWrapMode::Truncate), width, style.font())
}

pub(in crate::ui_kit) fn paint_centred(ui: &Ui, rect: Rect, galley: Arc<Galley>, color: Color32) {
    let at = Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size()).min;
    ui.painter().galley(at, galley, color);
}
