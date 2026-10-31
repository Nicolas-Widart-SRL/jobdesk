//! The in-tray's parts: one decision at a time, on top of a visible stack of what is left.

use egui::{CornerRadius, Frame, Margin, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, vec2};

use crate::ui_kit::Palette;
use crate::ui_kit::marks::paint_key;
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::surfaces::{offset_shadow, paint_outlined};
use crate::ui_kit::text::{line, rich};
use crate::ui_kit::tokens::{
    BUTTON_GAP, BUTTON_HEIGHT, BUTTON_PADDING, CARD_TAB_GAP, CARD_TAB_HEIGHT, CARD_TAB_PADDING, DEAL_CORNERS, DEAL_GAP, DEAL_PADDING, EDGE_CORNERS, HAIRLINE, HORIZON_CELL_HEIGHT,
    HORIZON_PADDING, ITEM_GAP, KEY_SIZE, OFFSET_SHADOW, RADIUS_BUTTON, RADIUS_PANEL, STACK_EDGE_HEIGHT, STACK_EDGE_STEP, STACK_EDGES_SHOWN, TAB_TOP, Type, UNDO_GAP, UNDO_PADDING,
};

/// A strip of day cells, one column each. The first cell is today.
pub fn horizon<T>(ui: &mut Ui, days: &[T], mut cell: impl FnMut(&mut Ui, &T, bool)) {
    let p = Palette::of(ui);
    let (x, top, bottom) = HORIZON_PADDING;
    Frame::new().fill(p.panel).corner_radius(RADIUS_PANEL).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.columns(days.len(), |columns| {
            for (index, (ui, day)) in columns.iter_mut().zip(days).enumerate() {
                let today = index == 0;
                let corners = if today { CornerRadius { nw: RADIUS_PANEL, sw: RADIUS_PANEL, ne: 0, se: 0 } } else { CornerRadius::ZERO };
                let shown = Frame::new().fill(if today { p.raised } else { p.panel }).corner_radius(corners).inner_margin(Margin { left: x, right: x, top, bottom }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(HORIZON_CELL_HEIGHT - (top + bottom) as f32);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        cell(ui, day, today);
                    });
                });
                if !today {
                    let rect = shown.response.rect;
                    ui.painter().vline(rect.left(), rect.y_range(), Stroke::new(HAIRLINE, p.line));
                }
            }
        });
    });
}

pub fn horizon_day(ui: &mut Ui, text: &str, today: bool) -> Response {
    let text = if today { rich(ui, text, Type::TAG, Ink::Text) } else { rich(ui, text, Type::SMALL, Ink::Muted) };
    ui.label(text)
}

/// An event on the strip. `tone` is `None` once the card behind it has left the tray.
pub fn horizon_mark(ui: &mut Ui, text: &str, tone: Option<Tone>) -> Response {
    let label = match tone {
        Some(tone) => egui::Label::new(rich(ui, text, Type::TAG, tone)).sense(Sense::click()),
        None => egui::Label::new(rich(ui, text, Type::TAG, Ink::Faint).strikethrough()).sense(Sense::hover()),
    };
    let response = ui.add(label.wrap());
    if let (Some(tone), true) = (tone, response.hovered()) {
        let color = tone.color(&Palette::of(ui));
        ui.painter().hline(response.rect.x_range(), response.rect.bottom(), Stroke::new(HAIRLINE, color));
    }
    response
}

/// The folder tab on top of the dealt card.
pub fn card_tab(ui: &mut Ui, title: &str, tone: Tone, when: &str) {
    let fill = Palette::of(ui).panel;
    let (x, y) = CARD_TAB_PADDING;
    Frame::new().fill(fill).corner_radius(TAB_TOP).inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = CARD_TAB_GAP;
        ui.set_min_height(CARD_TAB_HEIGHT - 2.0 * y as f32);
        ui.horizontal(|ui| {
            let title = rich(ui, title, Type::STRONG, tone);
            ui.label(title);
            crate::ui_kit::muted(ui, when);
        });
    });
}

/// The card being read. `waits` marks one that has no default and needs reading first.
pub fn deal<R>(ui: &mut Ui, waits: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    let (x, y) = DEAL_PADDING;
    let frame = Frame::new().fill(p.panel).corner_radius(DEAL_CORNERS).inner_margin(Margin::symmetric(x, y));
    let frame = if waits { frame.stroke(Stroke::new(HAIRLINE, p.interview)).shadow(offset_shadow(ui)) } else { frame };
    let inner = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = DEAL_GAP;
            add(ui)
        })
        .inner;
    if waits {
        ui.add_space(OFFSET_SHADOW as f32);
    }
    inner
}

/// The edges of the cards behind the dealt one. The count caps; the words say how many.
pub fn stack_edges(ui: &mut Ui, left: usize) {
    ui.spacing_mut().item_spacing.y = 0.0;
    for edge in 0..left.min(STACK_EDGES_SHOWN) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), STACK_EDGE_HEIGHT), Sense::hover());
        paint_outlined(ui, rect.shrink2(vec2((edge as f32 + 1.0) * STACK_EDGE_STEP, 0.0)), EDGE_CORNERS);
    }
    ui.spacing_mut().item_spacing.y = ITEM_GAP;
}

/// The inverted strip that confirms what just happened.
pub fn undo_bar<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    let (x, y) = UNDO_PADDING;
    ui.add_space(UNDO_GAP - ITEM_GAP);
    Frame::new()
        .fill(p.text)
        .corner_radius(RADIUS_PANEL)
        .inner_margin(Margin::symmetric(x, y))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.visuals_mut().override_text_color = Some(p.sheet);
            ui.spacing_mut().interact_size.y = BUTTON_HEIGHT;
            add(ui)
        })
        .inner
}

/// A button on the undo strip: outlined in the strip's own ink, filled when `on` or hovered.
pub fn undo_button(ui: &mut Ui, text: &str, on: bool, key: Option<&str>) -> Response {
    let p = Palette::of(ui);
    let keys = key.map_or(0.0, |_| KEY_SIZE + BUTTON_GAP);
    let measured = line(ui, text, Type::BUTTON, p.sheet, f32::INFINITY);
    let size = vec2(measured.size().x + BUTTON_PADDING * 2.0 + keys, BUTTON_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let lit = on || response.hovered();
    let ink = if lit { p.text } else { p.sheet };
    if lit {
        ui.painter().rect_filled(rect, RADIUS_BUTTON, p.sheet);
    }
    ui.painter().rect_stroke(rect, RADIUS_BUTTON, Stroke::new(HAIRLINE, p.sheet), StrokeKind::Inside);
    let galley = line(ui, text, Type::BUTTON, ink, f32::INFINITY);
    let at = egui::pos2(rect.left() + BUTTON_PADDING, rect.center().y - galley.size().y / 2.0);
    let right = at.x + galley.size().x;
    ui.painter().galley(at, galley, ink);
    if let Some(key) = key {
        let hint = egui::Rect::from_center_size(egui::pos2(right + BUTTON_GAP + KEY_SIZE / 2.0, rect.center().y), vec2(KEY_SIZE, KEY_SIZE));
        paint_key(ui, hint, key, ink);
    }
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, on, text));
    response
}
