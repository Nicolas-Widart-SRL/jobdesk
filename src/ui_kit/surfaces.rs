use egui::{CornerRadius, Frame, Margin, Panel, Rect, Response, Sense, Shadow, Stroke, StrokeKind, Ui, UiBuilder, vec2};

use crate::ui_kit::Palette;
use crate::ui_kit::palette::Ink;
use crate::ui_kit::text::{line, rich};
use crate::ui_kit::tokens::{
    BODY_PADDING, CARD_PADDING, CLEAR_PADDING, CODE_PADDING, GUTTER, HAIRLINE, HEADER_PADDING, INSET, ITEM_GAP, LANE_GAP, LANE_PADDING, NOTE_PADDING, OFFSET_SHADOW, PICK_PADDING,
    PROMPT_PADDING, RADIUS_CARD, RADIUS_PANEL, ROW_GAP, ROW_PADDING, SECTION_RULE, STEP_GAP, STEP_HEIGHT, STEP_PADDING, Type,
};

fn margin((x, y): (i8, i8)) -> Margin {
    Margin::symmetric(x, y)
}

/// The desk plane, under everything.
pub fn desk_frame(ui: &Ui) -> Frame {
    Frame::new().fill(Palette::of(ui).bg)
}

/// The sheet plane, where a screen's content sits.
pub fn sheet_frame(ui: &Ui) -> Frame {
    Frame::new().fill(Palette::of(ui).sheet)
}

/// A fixed column beside a screen's content. Planes and gutters separate it, never a line.
pub fn side_column(id: &'static str, width: f32) -> Panel {
    Panel::right(id).exact_size(width).resizable(false).show_separator_line(false).frame(Frame::new().inner_margin(Margin { left: GUTTER, ..Default::default() }))
}

/// A fixed column on the left of a screen's content, such as a list to pick from.
pub fn list_column(id: &'static str, width: f32) -> Panel {
    Panel::left(id).exact_size(width).resizable(false).show_separator_line(false).frame(Frame::new().inner_margin(Margin { right: GUTTER, ..Default::default() }))
}

/// A strip along the bottom of a column, for input.
pub fn input_strip(id: &'static str) -> Panel {
    Panel::bottom(id).show_separator_line(false).frame(Frame::new().inner_margin(Margin { top: ROW_GAP as i8, ..Default::default() }))
}

/// Stacked sections set their own padding, so nothing adds a gap between them.
fn stacked<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.spacing_mut().item_spacing.y = 0.0;
    add(ui)
}

/// A section: a heavy rule on the sheet, with no fill of its own.
pub fn panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let color = Palette::of(ui).text;
    let shown = Frame::new().inner_margin(Margin { top: SECTION_RULE as i8, ..Default::default() }).show(ui, |ui| stacked(ui, add));
    let mut rule = shown.response.rect;
    rule.set_height(SECTION_RULE);
    ui.painter().rect_filled(rule, 0.0, color);
    shown.inner
}

/// A side slip: the only kind of section with a fill.
pub fn slip<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = Palette::of(ui).panel;
    Frame::new().fill(fill).corner_radius(RADIUS_PANEL).show(ui, |ui| stacked(ui, add)).inner
}

pub fn panel_header(ui: &mut Ui, title: &str, note: Option<&str>) {
    Frame::new().inner_margin(Margin::symmetric(INSET, HEADER_PADDING)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let heading = rich(ui, title, Type::SECTION, Ink::Text);
            ui.label(heading);
            if let Some(note) = note {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let note = rich(ui, note, Type::NOTE, Ink::Muted);
                    ui.add(egui::Label::new(note).truncate());
                });
            }
        });
    });
    divider(ui);
}

pub fn divider(ui: &mut Ui) {
    let color = Palette::of(ui).line;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HAIRLINE), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color);
}

pub fn panel_body<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .inner_margin(Margin::symmetric(INSET, BODY_PADDING))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP;
            add(ui)
        })
        .inner
}

/// One row of a list inside a panel, ruled off from the row above unless `first`.
pub fn list_row<R>(ui: &mut Ui, first: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !first {
        divider(ui);
    }
    Frame::new()
        .inner_margin(Margin::symmetric(INSET, ROW_PADDING))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP;
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A whole row that picks something, such as a run in a list.
pub fn pick_row(ui: &mut Ui, on: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let p = Palette::of(ui);
    ui.scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
        let lit = on || ui.response().hovered();
        let frame = Frame::new().corner_radius(RADIUS_PANEL).inner_margin(margin(PICK_PADDING));
        let frame = if lit { frame.fill(p.panel) } else { frame };
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    })
    .response
}

/// A filled line across a screen, for a short message and its actions.
pub fn note<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = Palette::of(ui).panel;
    Frame::new()
        .fill(fill)
        .corner_radius(RADIUS_PANEL)
        .inner_margin(margin(NOTE_PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// The empty state of a whole screen.
pub fn clear_panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = Palette::of(ui).panel;
    Frame::new()
        .fill(fill)
        .corner_radius(RADIUS_PANEL)
        .inner_margin(margin(CLEAR_PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Commands and untrusted text: plain, in a recessed box.
pub fn code_box<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = Palette::of(ui).field;
    Frame::new()
        .fill(fill)
        .corner_radius(RADIUS_CARD)
        .inner_margin(margin(CODE_PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A hard offset shadow in the status colour: this needs reading before anything else.
pub(in crate::ui_kit) fn offset_shadow(ui: &Ui) -> Shadow {
    Shadow { offset: [OFFSET_SHADOW, OFFSET_SHADOW], blur: 0, spread: 0, color: Palette::of(ui).interview }
}

/// A run's question or permission request, set apart from the sheet.
pub fn prompt_box<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    Frame::new()
        .fill(p.sheet)
        .stroke(Stroke::new(HAIRLINE, p.interview))
        .shadow(offset_shadow(ui))
        .corner_radius(RADIUS_CARD)
        .inner_margin(margin(PROMPT_PADDING))
        .outer_margin(Margin { right: OFFSET_SHADOW, bottom: OFFSET_SHADOW, ..Default::default() })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// The hairline around a rendered document page.
pub fn page_frame<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let border = Palette::of(ui).line;
    Frame::new().stroke(Stroke::new(HAIRLINE, border)).corner_radius(1).show(ui, add).inner
}

/// A folder of index cards on a board.
pub fn lane<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let (x, top, bottom) = LANE_PADDING;
    let fill = Palette::of(ui).panel;
    Frame::new()
        .fill(fill)
        .corner_radius(RADIUS_PANEL)
        .inner_margin(Margin { left: x, right: x, top, bottom })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = LANE_GAP;
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// One application on a board. Clicking anywhere on it counts.
pub fn index_card(ui: &mut Ui, on: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let p = Palette::of(ui);
    ui.scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
        let fill = if on || ui.response().hovered() { p.raised } else { p.sheet };
        Frame::new().fill(fill).corner_radius(RADIUS_CARD).inner_margin(margin(CARD_PADDING)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 2.0;
            add(ui);
        });
    })
    .response
}

/// Where a run has got to. Steps before `current` are done; `live` lights the current one.
pub fn step_rail(ui: &mut Ui, labels: &[&str], current: usize, live: bool) {
    let p = Palette::of(ui);
    ui.spacing_mut().item_spacing = vec2(STEP_GAP, STEP_GAP);
    ui.horizontal_wrapped(|ui| {
        for (index, label) in labels.iter().enumerate() {
            let now = index == current && live;
            let done = index < current;
            let (style, ink) = if now {
                (Type::STEP_NOW, p.on_primary)
            } else if done {
                (Type::STEP_DONE, p.text)
            } else {
                (Type::STEP, p.muted)
            };
            let galley = line(ui, &format!("{index} {label}"), style, ink, f32::INFINITY);
            let size = vec2(galley.size().x + STEP_PADDING * 2.0, STEP_HEIGHT);
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let corners = CornerRadius {
                nw: if index == 0 { RADIUS_PANEL } else { 0 },
                sw: if index == 0 { RADIUS_PANEL } else { 0 },
                ne: if index + 1 == labels.len() { RADIUS_PANEL } else { 0 },
                se: if index + 1 == labels.len() { RADIUS_PANEL } else { 0 },
            };
            ui.painter().rect_filled(rect, corners, if now { p.primary } else { p.panel });
            let at = Rect::from_center_size(rect.center(), galley.size()).min;
            ui.painter().galley(at, galley, ink);
        }
    });
}

pub(in crate::ui_kit) fn paint_outlined(ui: &Ui, rect: Rect, radius: CornerRadius) {
    let p = Palette::of(ui);
    ui.painter().rect(rect, radius, p.raised, Stroke::new(HAIRLINE, p.line), StrokeKind::Inside);
}
