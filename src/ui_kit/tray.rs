//! The in-tray's parts: one decision at a time, on top of a visible stack of what is left.

use egui::{CornerRadius, Frame, Label, Margin, Rect, Response, Role, Sense, Shape, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};

use crate::ui_kit::Palette;
use crate::ui_kit::controls::{small_key_button, small_toggle};
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::surfaces::{inset, offset_shadow, paint_outlined};
use crate::ui_kit::text::{line, rich};
use crate::ui_kit::tokens::{
    CARD_TAB_GAP, CARD_TAB_HEIGHT, CARD_TAB_PADDING, DEAL_CORNERS, DEAL_GAP, DEAL_PADDING, EDGE_CORNERS, FOOT_CORNERS, HAIRLINE, HORIZON_CELL_HEIGHT, HORIZON_CHIP_HEIGHT,
    HORIZON_CHIP_PADDING, HORIZON_NOTE_PADDING, HORIZON_PADDING, HORIZON_TIP_WIDTH, ITEM_GAP, OFFSET_SHADOW, RADIUS_CARD, RADIUS_PANEL, STACK_EDGE_HEIGHT, STACK_EDGE_STEP,
    STACK_EDGES_SHOWN, TAB_TOP, TOGGLE_GAP, Type, UNDO_ROW_PADDING,
};

/// A strip of day cells, one column each. The first cell is today. `note` reads across the third to the last day.
pub fn horizon<T>(ui: &mut Ui, days: &[T], note: Option<&str>, mut cell: impl FnMut(&mut Ui, &T, bool)) {
    let p = Palette::of(ui);
    let (x, top, bottom) = HORIZON_PADDING;
    Frame::new().fill(p.panel).corner_radius(RADIUS_PANEL).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let fill = ui.painter().add(Shape::Noop);
        let mut rects = Vec::with_capacity(days.len());
        ui.columns(days.len(), |columns| {
            for (index, (ui, day)) in columns.iter_mut().zip(days).enumerate() {
                let today = index == 0;
                let shown = Frame::new().inner_margin(Margin { left: x, right: x, top, bottom }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(HORIZON_CELL_HEIGHT - (top + bottom) as f32);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        cell(ui, day, today);
                    });
                });
                rects.push(shown.response.rect);
            }
        });
        let Some(first) = rects.first() else { return };
        let strip = rects.iter().fold(*first, |strip, rect| strip.union(*rect));
        let corners = CornerRadius { nw: RADIUS_PANEL, sw: RADIUS_PANEL, ne: 0, se: 0 };
        ui.painter().set(fill, Shape::rect_filled(Rect::from_x_y_ranges(first.x_range(), strip.y_range()), corners, p.raised));
        for rect in &rects[1..] {
            ui.painter().vline(rect.left(), strip.y_range(), Stroke::new(HAIRLINE, p.line));
        }
        if let (Some(note), true) = (note, rects.len() > 2) {
            let galley = line(ui, note, Type::SMALL, p.muted, f32::INFINITY);
            let across = rects[2].left()..=rects[rects.len() - 1].right();
            let centre = pos2((across.start() + across.end()) / 2.0, strip.center().y);
            let text_rect = Rect::from_center_size(centre, galley.size());
            let (pad_x, pad_y) = HORIZON_NOTE_PADDING;
            ui.painter().rect_filled(text_rect.expand2(vec2(pad_x, pad_y)), 0.0, p.panel);
            ui.painter().galley(text_rect.min, galley, p.muted);
            let response = ui.interact(text_rect, ui.scope_id().with("horizon_note"), Sense::hover());
            response.widget_info(|| WidgetInfo::labeled(Role::Label, true, note));
        }
    });
}

/// Today's pending count, the overdue line and one line per kind, in the cell's own gap.
pub fn horizon_count(ui: &mut Ui, count: usize, overdue: usize, kinds: &[(usize, &str)]) {
    let ink = if count == 0 { Ink::Muted } else { Ink::Text };
    ui.add(Label::new(rich(ui, format!("{count} to decide"), Type::TAG, ink)).truncate());
    if overdue > 0 {
        ui.add(Label::new(rich(ui, format!("{overdue} overdue"), Type::TAG, Tone::Interview)).truncate());
    }
    for (n, kind) in kinds {
        ui.add(Label::new(rich(ui, format!("{n} {kind}"), Type::SMALL, Ink::Muted)).truncate());
    }
}

/// The chip that brings back the cards set aside. `label` is what a screen reader hears, `tip` shows on hover,
/// and on keyboard focus while `active`.
/// In a narrow column the chip keeps to the column and its text elides.
pub fn horizon_return(ui: &mut Ui, text: &str, label: &str, active: bool, tip: impl FnOnce(&mut Ui)) -> Response {
    let p = Palette::of(ui);
    let room = ui.available_width();
    let galley = line(ui, text, Type::TAG, p.text, (room - HORIZON_CHIP_PADDING * 2.0).max(0.0));
    let size = vec2((galley.size().x + HORIZON_CHIP_PADDING * 2.0).min(room), HORIZON_CHIP_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let edge = if response.hovered() { p.text } else { p.line };
    ui.painter().rect_filled(rect, RADIUS_CARD, p.sheet);
    ui.painter().rect_stroke(rect, RADIUS_CARD, Stroke::new(HAIRLINE, edge), StrokeKind::Inside);
    let at = pos2(rect.left() + HORIZON_CHIP_PADDING, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, p.text);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
    let tip = |ui: &mut Ui| {
        ui.set_max_width(HORIZON_TIP_WIDTH);
        tip(ui)
    };
    if active && response.has_focus() {
        response.show_tooltip_ui(tip);
        return response;
    }
    response.on_hover_ui(tip)
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

/// The card being read. `frame` rings one that has no default and needs reading first, in the colour of why.
pub fn deal<R>(ui: &mut Ui, frame: Option<Tone>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    let (x, y) = DEAL_PADDING;
    let plain = Frame::new().fill(p.panel).corner_radius(DEAL_CORNERS).inner_margin(Margin::symmetric(x, y));
    let ring = frame.map(|tone| tone.color(&p));
    let framed = match ring {
        Some(color) => plain.stroke(Stroke::new(HAIRLINE, color)).shadow(offset_shadow(ui, color)),
        None => plain,
    };
    let inner = framed
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = DEAL_GAP;
            add(ui)
        })
        .inner;
    if ring.is_some() {
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

/// What was pressed on an undo row: the Undo button, or the toggle at that index.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UndoPress {
    pub undo: bool,
    pub toggle: Option<usize>,
}

/// The raised head row of a slip's list. `name` comes from a posting, so it shows as a plain label.
/// `label` is what a screen reader hears for the row. `last` rounds the foot when nothing follows the row.
pub fn undo_row(ui: &mut Ui, name: &str, note: &str, label: &str, toggles: &[(&str, bool)], last: bool) -> UndoPress {
    let p = Palette::of(ui);
    let (top, bottom) = UNDO_ROW_PADDING;
    let corners = if last { FOOT_CORNERS } else { CornerRadius::ZERO };
    let x = inset(ui);
    let mut pressed = UndoPress::default();
    let shown = Frame::new().fill(p.raised).corner_radius(corners).inner_margin(Margin { left: x, right: x, top, bottom }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = ITEM_GAP;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                pressed.undo = small_key_button(ui, "Undo", "U").clicked();
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(Label::new(rich(ui, name, Type::STRONG, Ink::Text)).truncate());
                });
            });
        });
        ui.label(rich(ui, note, Type::BODY, Ink::Muted));
        if !toggles.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(TOGGLE_GAP, TOGGLE_GAP);
                for (index, (text, on)) in toggles.iter().enumerate() {
                    if small_toggle(ui, text, *on).clicked() {
                        pressed.toggle = Some(index);
                    }
                }
            });
        }
    });
    // A hover-only interact leaves the buttons' own hover alone.
    ui.interact(shown.response.rect, ui.scope_id().with("undo_row"), Sense::hover()).widget_info(|| WidgetInfo::labeled(Role::Status, true, label));
    pressed
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;
    use crate::ui_kit::{panel_body, slip};

    const SLIP: f32 = crate::ui_kit::tokens::TRAY_RAIL - crate::ui_kit::tokens::GUTTER as f32;
    const LABEL: &str = "Last decision: Skipped, Quarry Labs. Press U to undo.";

    fn harness(presses: Rc<RefCell<UndoPress>>) -> Harness<'static> {
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        Harness::builder().with_size([400.0, 300.0]).with_context_setup(setup).build_ui(move |ui| {
            ui.set_max_width(SLIP);
            slip(ui, |ui| {
                let toggles = [("Rate", false), ("Scope", true), ("Domain", false), ("Location", false)];
                let pressed = undo_row(ui, "Quarry Labs", "Skipped. Why? Optional.", LABEL, &toggles, true);
                if pressed != UndoPress::default() {
                    *presses.borrow_mut() = pressed;
                }
                panel_body(ui, |_| {});
            });
        })
    }

    #[test]
    fn a_picked_toggle_comes_back_by_index() {
        let presses = Rc::new(RefCell::new(UndoPress::default()));
        let mut harness = harness(presses.clone());
        harness.step();
        harness.get_by_label("Scope").click();
        harness.run();
        assert_eq!(*presses.borrow(), UndoPress { undo: false, toggle: Some(1) });
        *presses.borrow_mut() = UndoPress::default();
        harness.get_by_label("Undo").click();
        harness.run();
        assert_eq!(*presses.borrow(), UndoPress { undo: true, toggle: None });
    }

    #[test]
    fn the_row_reads_as_one_label_and_a_toggle_says_it_is_on() {
        let mut harness = harness(Rc::default());
        harness.step();
        harness.get_by_label(LABEL);
        // egui reports a selected button to a screen reader as toggled.
        assert_eq!(harness.get_by_label("Scope").accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        assert_eq!(harness.get_by_label("Rate").accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
    }

    #[test]
    fn four_reasons_fit_one_line_in_the_rail() {
        let mut harness = harness(Rc::default());
        harness.step();
        assert_eq!(harness.get_by_label("Rate").rect().top(), harness.get_by_label("Location").rect().top());
    }
}
