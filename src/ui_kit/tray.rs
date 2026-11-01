//! The in-tray's parts: one decision at a time, on top of a visible stack of what is left.

use egui::{Align, Color32, CornerRadius, Frame, Label, Layout, Margin, Rect, Response, RichText, Role, ScrollArea, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, WidgetInfo, pos2, vec2};

use crate::ui_kit::Palette;
use crate::ui_kit::controls::{focus_ring, small_button, small_key_button, small_toggle};
use crate::ui_kit::marks::solid_tag;
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::surfaces::{divider, inset, offset_shadow, paint_outlined};
use crate::ui_kit::text::{line, rich};
use crate::ui_kit::tokens::{
    AGENDA_DAY_WIDTH, BUTTON_GAP, CARD_TAB_GAP, CARD_TAB_HEIGHT, CARD_TAB_PADDING, CHIP_HEIGHT, CHIP_PADDING, DEAL_CORNERS, DEAL_FOOT_PADDING, DEAL_GAP, DEAL_PADDING, EDGE_CORNERS, FOOT_CORNERS, HAIRLINE, HORIZON_CELL_HEIGHT,
    HORIZON_NOTE_PADDING, HORIZON_PADDING, HORIZON_TIP_WIDTH, ITEM_GAP, OFFSET_SHADOW, PICK_ROW_HEIGHT, PICK_ROW_INLINE, RADIUS_CARD, RADIUS_PANEL, STACK_EDGE_HEIGHT, STACK_EDGE_STEP,
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
    let galley = line(ui, text, Type::TAG, p.text, (room - CHIP_PADDING * 2.0).max(0.0));
    let size = vec2((galley.size().x + CHIP_PADDING * 2.0).min(room), CHIP_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let edge = if response.hovered() { p.text } else { p.line };
    ui.painter().rect_filled(rect, RADIUS_CARD, p.sheet);
    ui.painter().rect_stroke(rect, RADIUS_CARD, Stroke::new(HAIRLINE, edge), StrokeKind::Inside);
    let at = pos2(rect.left() + CHIP_PADDING, rect.center().y - galley.size().y / 2.0);
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

/// What fills a card's tab: the card itself with the kind in a status tone, or the stamp for what Claude asks of the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabInk {
    Tone(Tone),
    Stamp,
}

/// Where a card's tab sits and what fills it, so the card under it can open its top edge there.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct CardTab {
    rect: Rect,
    fill: Color32,
}

/// The folder tab on top of the dealt card. Hand it to `deal`, which joins the two.
pub fn card_tab(ui: &mut Ui, title: &str, ink: TabInk, when: &str) -> CardTab {
    let p = Palette::of(ui);
    let (fill, title_ink, when_ink) = match ink {
        TabInk::Tone(tone) => (p.card, tone.color(&p), p.muted),
        TabInk::Stamp => (p.stamp, p.on_stamp, p.on_stamp),
    };
    let (x, y) = CARD_TAB_PADDING;
    let shown = Frame::new().fill(fill).corner_radius(TAB_TOP).inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = CARD_TAB_GAP;
        ui.set_min_height(CARD_TAB_HEIGHT - 2.0 * y as f32);
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).font(Type::STRONG.font()).color(title_ink));
            ui.label(RichText::new(when).font(Type::BODY.font()).color(when_ink));
        });
    });
    let rect = shown.response.rect;
    if let TabInk::Tone(_) = ink {
        ui.painter().rect_stroke(rect, TAB_TOP, Stroke::new(HAIRLINE, p.line), StrokeKind::Inside);
    }
    CardTab { rect, fill }
}

/// How the dealt card sits. `key` names it, so the next card's body opens at its top.
/// `frame` rings one that has no default and needs reading first, in the colour of why. `behind` counts the cards under it.
#[derive(Debug, Clone, Copy)]
pub struct DealLook<'a> {
    pub key: &'a str,
    pub frame: Option<Tone>,
    pub behind: usize,
}

/// The card being read, under its tab and on the edges of the cards behind it. It takes the height left in `ui`:
/// the body scrolls, and the footer with the card's actions stays pinned under it.
pub fn deal<R>(ui: &mut Ui, tab: CardTab, look: DealLook, body: impl FnOnce(&mut Ui) -> R, footer: impl FnOnce(&mut Ui)) -> R {
    let p = Palette::of(ui);
    let ring = look.frame.map(|tone| tone.color(&p));
    let shadow = if ring.is_some() { OFFSET_SHADOW as f32 } else { 0.0 };
    let edges = look.behind.min(STACK_EDGES_SHOWN) as f32 * STACK_EDGE_HEIGHT;
    // In a short window the body gives way first, so the footer and the edges stay inside it.
    let height = (ui.available_height() - edges - shadow).max(0.0);
    let frame = Frame::new().fill(p.card).stroke(Stroke::new(HAIRLINE, ring.unwrap_or(p.line))).corner_radius(DEAL_CORNERS);
    let frame = match ring {
        Some(color) => frame.shadow(offset_shadow(ui, color)),
        None => frame,
    };
    let (x, y) = DEAL_PADDING;
    // The stroke sits inside the allocated height, on both edges.
    let content = (height - 2.0 * HAIRLINE).max(0.0);
    let shown = ui.allocate_ui(vec2(ui.available_width(), height), |ui| {
        frame.show(ui, |ui| {
            let inner = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), content));
            ui.advance_cursor_after_rect(inner);
            // The footer goes in first, at the foot of the card, so the body takes what it leaves.
            // Its height is known only once laid out: when it differs from the height kept for this card,
            // the pass is discarded and laid out again before anything shows.
            let kept = ui.scope_id().with(("deal_footer", look.key));
            let foot_height = ui.data(|data| data.get_temp::<f32>(kept)).unwrap_or(0.0).min(content);
            let foot_rect = Rect::from_min_max(pos2(inner.left(), inner.bottom() - foot_height), pos2(inner.right(), inner.bottom() + content));
            let mut foot = ui.new_child(UiBuilder::new().id_salt("deal_footer").max_rect(foot_rect).layout(Layout::top_down(Align::Min)));
            foot.shrink_clip_rect(inner);
            deal_footer(&mut foot, footer);
            let laid = foot.min_rect().height().min(content);
            if (laid - foot_height).abs() > 0.5 {
                ui.data_mut(|data| data.insert_temp(kept, laid));
                ui.ctx().request_discard("a card's footer changed height");
            }
            let body_rect = Rect::from_min_max(inner.min, pos2(inner.right(), foot_rect.top()));
            let mut body_ui = ui.new_child(UiBuilder::new().id_salt("deal_body").max_rect(body_rect).layout(Layout::top_down(Align::Min)));
            body_ui.shrink_clip_rect(body_rect);
            ScrollArea::vertical().id_salt(("deal_body", look.key)).auto_shrink([false, false]).show(&mut body_ui, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(x, y))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = DEAL_GAP;
                        body(ui)
                    })
                    .inner
            })
            .inner
        })
    });
    // The tab's foot and the card's top edge give way to the tab's fill, so the two read as one sheet.
    let card = shown.inner.response.rect;
    let join = Rect::from_x_y_ranges((tab.rect.left() + HAIRLINE)..=(tab.rect.right() - HAIRLINE), (tab.rect.bottom() - HAIRLINE)..=(card.top() + HAIRLINE));
    ui.painter().rect_filled(join, 0.0, tab.fill);
    if ring.is_some() {
        ui.add_space(shadow);
    }
    stack_edges(ui, look.behind);
    shown.inner.inner
}

/// The row under a card's body that holds its actions, ruled off on top.
pub fn deal_footer(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    divider(ui);
    let (x, y) = DEAL_FOOT_PADDING;
    Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(BUTTON_GAP, BUTTON_GAP);
            add(ui);
        });
    });
}

/// What closes an agenda row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgendaEnd<'a> {
    Nothing,
    /// The row is the card being read.
    Now,
    /// The row's card is decided: the row reads faint and cannot be clicked.
    Done,
    /// How late the row is, such as "2 days late".
    Late(&'a str),
    Button(&'a str),
}

/// What was pressed on an agenda row: the row itself, or its button.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgendaPress {
    pub row: bool,
    pub button: bool,
}

/// One dated line in a list of what is coming: the day, what happens, and how it ends. Ruled off from the row above unless `first`.
/// `text` may come from a posting, so it is painted as plain text, cut to the room left.
pub fn agenda_row(ui: &mut Ui, day: &str, text: &str, end: AgendaEnd, first: bool) -> AgendaPress {
    let p = Palette::of(ui);
    if !first {
        divider(ui);
    }
    let done = end == AgendaEnd::Done;
    let sense = if done { Sense::hover() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), PICK_ROW_HEIGHT), sense);
    if !done && response.hovered() {
        ui.painter().rect_filled(rect, 0.0, p.raised);
    }
    let left = rect.left() + PICK_ROW_INLINE;
    let right = rect.right() - PICK_ROW_INLINE;
    let mut end_ui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_x_y_ranges(left..=right, rect.y_range())).layout(egui::Layout::right_to_left(egui::Align::Center)));
    let mut pressed = AgendaPress::default();
    match end {
        AgendaEnd::Nothing => {}
        AgendaEnd::Now => {
            solid_tag(&mut end_ui, "Now", Tone::Turn);
        }
        AgendaEnd::Done => {
            end_ui.label(rich(ui, "Done", Type::SMALL, Ink::Faint));
        }
        AgendaEnd::Late(late) => {
            end_ui.label(rich(ui, late, Type::TAG, Tone::Interview));
        }
        AgendaEnd::Button(label) => pressed.button = small_button(&mut end_ui, label).clicked(),
    }
    let used = end_ui.min_rect().width();
    let (ink, day_ink) = if done { (p.faint, p.faint) } else { (p.text, p.muted) };
    let dated = line(ui, day, Type::NOTE, day_ink, AGENDA_DAY_WIDTH);
    let text_left = left + AGENDA_DAY_WIDTH;
    let room = (right - used - BUTTON_GAP - text_left).max(0.0);
    let said = line(ui, text, Type::BODY, ink, room);
    let centred = |height: f32| rect.center().y - height / 2.0;
    let (dated_at, said_at) = (pos2(left, centred(dated.size().y)), pos2(text_left, centred(said.size().y)));
    ui.painter().galley(dated_at, dated, day_ink);
    ui.painter().galley(said_at, said, ink);
    if !done {
        focus_ring(ui, &response, 0);
    }
    let role = if done { Role::Label } else { Role::Button };
    let spoken = match end {
        AgendaEnd::Done => format!("{day}, {text}, done"),
        AgendaEnd::Late(late) => format!("{day}, {text}, {late}"),
        _ => format!("{day}, {text}"),
    };
    response.widget_info(|| WidgetInfo::labeled(role, true, &spoken));
    pressed.row = response.clicked();
    pressed
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
    fn a_done_agenda_row_cannot_be_clicked() {
        let presses = Rc::new(RefCell::new(Vec::new()));
        let seen = presses.clone();
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([400.0, 200.0]).with_context_setup(setup).build_ui(move |ui| {
            ui.set_max_width(SLIP);
            let rows = [("Sun 1", "1 comes back", AgendaEnd::Button("Bring back")), ("Tue 3", "Harbor Bank deadline", AgendaEnd::Done), ("Thu 5", "Meridian Rail closes", AgendaEnd::Now)];
            for (index, (day, text, end)) in rows.into_iter().enumerate() {
                let pressed = agenda_row(ui, day, text, end, index == 0);
                if pressed != AgendaPress::default() {
                    seen.borrow_mut().push((index, pressed));
                }
            }
        });
        harness.step();
        harness.get_by_label("Tue 3, Harbor Bank deadline, done").click();
        harness.run();
        assert_eq!(*presses.borrow(), Vec::new());
        harness.get_by_label("Thu 5, Meridian Rail closes").click();
        harness.run();
        harness.get_by_label("Bring back").click();
        harness.run();
        assert_eq!(*presses.borrow(), vec![(2, AgendaPress { row: true, button: false }), (0, AgendaPress { row: false, button: true })]);
    }

    #[test]
    fn four_reasons_fit_one_line_in_the_rail() {
        let mut harness = harness(Rc::default());
        harness.step();
        assert_eq!(harness.get_by_label("Rate").rect().top(), harness.get_by_label("Location").rect().top());
    }
}
