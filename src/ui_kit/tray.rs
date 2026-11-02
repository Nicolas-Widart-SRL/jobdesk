//! Today's parts: one decision at a time, on top of a visible stack of what is left, beside what is done and what is coming.

use egui::{Area, Color32, CornerRadius, Frame, Margin, Order, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};

use crate::ui_kit::Palette;
use crate::ui_kit::controls::{interactive_row, pick_look, pointer_over, pressed, small_button, small_key_button, small_toggle};
use crate::ui_kit::marks::solid_tag;
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::surfaces::{divider, footed, footer_row, inset, offset_shadow, paint_outlined, surface};
use crate::ui_kit::text::{add_cut_text, add_galley, add_text, cut_hover, line, paint_text};
use crate::ui_kit::tokens::{
    AGENDA_DAY_WIDTH, AGENDA_TIP_WIDTH, BLANK_TAB_WIDTH, BUTTON_GAP, CARD_TAB_GAP, CARD_TAB_HEIGHT, CARD_TAB_PADDING, DEAL_CORNERS, DEAL_FOOT_PADDING,
    DEAL_GAP, DEAL_PADDING, EDGE_CORNERS, FOOT_CORNERS, HAIRLINE, ITEM_GAP, MOTION_SLIDE, OFFSET_SHADOW, PICK_ROW_HEIGHT, PICK_ROW_INLINE, STACK_EDGE_HEIGHT,
    STACK_EDGE_STEP, STACK_EDGES_SHOWN, TAB_TOP, TOGGLE_GAP, Type, UNDO_ROW_PADDING,
};

/// Where a card's tab sits and what fills it, so the card under it can open its top edge there.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct CardTab {
    rect: Rect,
    fill: Color32,
}

/// The folder tab on top of the dealt card. Hand it to `deal`, which joins the two.
/// The card fills it, with the kind in its status tone. `Turn`, for what Claude asks of the user, fills it with the stamp.
pub fn card_tab(ui: &mut Ui, title: &str, tone: Tone, when: &str) -> CardTab {
    let p = Palette::of(ui);
    let (fill, title_ink, when_ink) = match tone {
        Tone::Turn => (p.turn, p.on_turn, p.on_turn),
        tone => (p.card, tone.color(&p), p.muted),
    };
    let (x, y) = CARD_TAB_PADDING;
    let shown = Frame::new().fill(fill).corner_radius(TAB_TOP).inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = CARD_TAB_GAP;
        ui.set_min_height(CARD_TAB_HEIGHT - 2.0 * y as f32);
        ui.horizontal(|ui| {
            add_galley(ui, line(ui, title, Type::STRONG, title_ink, f32::INFINITY));
            add_galley(ui, line(ui, when, Type::BODY, when_ink, f32::INFINITY));
        });
    });
    let rect = shown.response.rect;
    if tone != Tone::Turn {
        ui.painter().rect_stroke(rect, TAB_TOP, Stroke::new(HAIRLINE, p.line), StrokeKind::Inside);
    }
    CardTab { rect, fill }
}

/// An empty tab, `BLANK_TAB_WIDTH` wide, over a card that is still loading.
pub fn blank_card_tab(ui: &mut Ui) -> CardTab {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(BLANK_TAB_WIDTH, CARD_TAB_HEIGHT), Sense::hover());
    ui.painter().rect_filled(rect, TAB_TOP, p.card);
    ui.painter().rect_stroke(rect, TAB_TOP, Stroke::new(HAIRLINE, p.line), StrokeKind::Inside);
    CardTab { rect, fill: p.card }
}

/// How the dealt card sits. `key` names it, so a card that comes on top, new or back, opens its body at the top.
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
    let shown = ui.allocate_ui(vec2(ui.available_width(), height), |ui| {
        frame.show(ui, |ui| {
            // A card that comes on top, new or back, opens its body at the top.
            let dealt = ui.scope_id().with("deal_shown");
            let fresh = ui.data(|data| data.get_temp::<String>(dealt)).as_deref() != Some(look.key);
            if fresh {
                ui.data_mut(|data| data.insert_temp(dealt, look.key.to_owned()));
            }
            let body = |ui: &mut Ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(x, y))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = DEAL_GAP;
                        body(ui)
                    })
                    .inner
            };
            footed(ui, true, fresh, body, |ui| deal_footer(ui, footer))
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

/// A copy of a decided card over `room`, `part` of the way off: slid toward the rail and faded.
/// It sits on a layer that takes no input, so only the card under it can be pressed.
pub fn departing(ui: &Ui, room: Rect, part: f32, add: impl FnOnce(&mut Ui)) {
    let area = Area::new(ui.scope_id().with("departing")).order(Order::Foreground).interactable(false).constrain(false);
    area.fixed_pos(room.min + vec2(MOTION_SLIDE * part, 0.0)).show(ui.ctx(), |ui| {
        ui.set_opacity(1.0 - part);
        ui.set_min_size(room.size());
        ui.set_max_size(room.size());
        add(ui);
    });
}

/// The row under a card's body that holds its actions, ruled off on top.
pub fn deal_footer(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    footer_row(ui, DEAL_FOOT_PADDING, add);
}

/// What closes an agenda row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgendaEnd<'a> {
    Nothing,
    /// The row is the card being read: it cannot be clicked.
    Now,
    /// The row's card is decided: the row reads faint and cannot be clicked.
    Done,
    /// How late the row is, such as "2 days late".
    Late(&'a str),
    /// The card being read, and how late it is: it cannot be clicked.
    LateNow(&'a str),
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
    let (pressed, response) = agenda_line(ui, day, text, end, first);
    let role = if matches!(end, AgendaEnd::Done | AgendaEnd::Now | AgendaEnd::LateNow(_)) { Role::Label } else { Role::Button };
    let spoken = match end {
        AgendaEnd::Done => format!("{day}, {text}, done"),
        AgendaEnd::Now => format!("{day}, {text}, now"),
        AgendaEnd::Late(late) => format!("{day}, {text}, {late}"),
        AgendaEnd::LateNow(late) => format!("{day}, {text}, {late}, now"),
        _ => format!("{day}, {text}"),
    };
    response.widget_info(|| WidgetInfo::labeled(role, true, &spoken));
    pressed
}

/// The first line of a list of what is coming: what comes back on `day`, with `button` to bring it back now.
/// `label` is what a screen reader hears for the row. `tip` shows on hover, and on keyboard focus while `active`.
pub fn agenda_return(ui: &mut Ui, day: &str, text: &str, button: &str, label: &str, active: bool, tip: impl FnOnce(&mut Ui)) -> AgendaPress {
    let (pressed, response) = agenda_line(ui, day, text, AgendaEnd::Button(button), true);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
    let tip = |ui: &mut Ui| {
        ui.set_max_width(AGENDA_TIP_WIDTH);
        tip(ui)
    };
    if active && response.has_focus() {
        response.show_tooltip_ui(tip);
    } else {
        response.on_hover_ui(tip);
    }
    pressed
}

/// Paints an agenda row and hands back its response, for the caller to name.
fn agenda_line(ui: &mut Ui, day: &str, text: &str, end: AgendaEnd, first: bool) -> (AgendaPress, Response) {
    let p = Palette::of(ui);
    if !first {
        divider(ui);
    }
    let done = end == AgendaEnd::Done;
    // A decided row and the card being read have nothing to open: they take no click, light no fill and keep the arrow.
    let inert = done || matches!(end, AgendaEnd::Now | AgendaEnd::LateNow(_));
    let sense = if inert { Sense::hover() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), PICK_ROW_HEIGHT), sense);
    let fill = if inert { Color32::TRANSPARENT } else { pick_look(&p, false, pointer_over(&response), pressed(&response)).fill };
    ui.painter().rect_filled(rect, 0.0, fill);
    let left = rect.left() + PICK_ROW_INLINE;
    let right = rect.right() - PICK_ROW_INLINE;
    // The row lights up under its end, so the end stands on a surface, never on the desk.
    let mut end_ui =
        ui.new_child(surface().max_rect(Rect::from_x_y_ranges(left..=right, rect.y_range())).layout(egui::Layout::right_to_left(egui::Align::Center)));
    let mut pressed = AgendaPress::default();
    match end {
        AgendaEnd::Nothing => {}
        AgendaEnd::Now => {
            solid_tag(&mut end_ui, "Now", Tone::Turn);
        }
        AgendaEnd::Done => {
            add_text(&mut end_ui, "Done", Type::SMALL, Ink::Faint);
        }
        AgendaEnd::Late(late) => {
            add_text(&mut end_ui, late, Type::TAG, Tone::Interview);
        }
        // Right to left: the tag closes the row, and the lateness reads before it.
        AgendaEnd::LateNow(late) => {
            solid_tag(&mut end_ui, "Now", Tone::Turn);
            add_text(&mut end_ui, late, Type::TAG, Tone::Interview);
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
    paint_text(ui, dated_at, dated, day_ink, fill);
    let response = cut_hover(response, &said, text);
    paint_text(ui, said_at, said, ink, fill);
    if !inert {
        interactive_row(ui, &response, 0);
    }
    pressed.row = response.clicked();
    (pressed, response)
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
                    add_cut_text(ui, name, Type::STRONG, Ink::Text);
                });
            });
        });
        add_text(ui, note, Type::BODY, Ink::Muted);
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
            let rows = [
                ("Sun 1", "1 comes back", AgendaEnd::Button("Bring back")),
                ("Tue 3", "Harbor Bank deadline", AgendaEnd::Done),
                ("Thu 5", "Meridian Rail closes", AgendaEnd::Now),
            ];
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
        harness.get_by_label("Thu 5, Meridian Rail closes, now").click();
        harness.run();
        assert_eq!(*presses.borrow(), Vec::new());
        harness.get_by_label("Bring back").click();
        harness.run();
        assert_eq!(*presses.borrow(), vec![(0, AgendaPress { row: false, button: true })]);
    }

    #[test]
    fn four_reasons_fit_one_line_in_the_rail() {
        let mut harness = harness(Rc::default());
        harness.step();
        assert_eq!(harness.get_by_label("Rate").rect().top(), harness.get_by_label("Location").rect().top());
    }
}
