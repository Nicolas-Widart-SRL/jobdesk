use egui::{Frame, Key, Margin, Modifiers, ScrollArea, Ui};

use crate::intray::application::{Action, Body, Card, CardKind, Check, GateRow, HORIZON_DAYS, InTray, Mark, Queue, Request, SkipReason, UndoOffer};
use crate::ui_kit::{
    Ink, Subtitle, Title, Tone, Type, alert, button, card_tab, clear_panel, code_box, command_button, deal, divider, faint_mono, horizon as horizon_strip, horizon_count, horizon_day,
    horizon_mark as strip_mark, horizon_return,
    key_button, muted, page_body, page_head, panel_body, panel_header, primary_button, primary_key_button, rich, side_column, slip, stack_edges, stamp, strong, tag, undo_row,
};
use crate::ui_kit::tokens::TRAY_RAIL;

const GATE_COLUMNS: usize = 3;
const CODE_HEIGHT: f32 = 150.0;

enum Pressed {
    Act(Action),
    Later,
    Skip,
    Undo,
    Reason(SkipReason),
    Bring(String),
    BringAll,
}

fn kind_tag(kind: CardKind) -> (&'static str, Tone) {
    match kind {
        CardKind::Permission => ("Permission", Tone::Interview),
        CardKind::Question => ("Question", Tone::Interview),
        CardKind::Turn => ("Your turn", Tone::Turn),
        CardKind::Send => ("Send", Tone::Drafted),
        CardKind::Triage => ("Triage", Tone::Closed),
        CardKind::Prep => ("Prep", Tone::Interview),
        CardKind::Offer => ("Offer", Tone::Offer),
        CardKind::GuardFlag => ("Guard flag", Tone::Interview),
        CardKind::FollowUp => ("Follow-up", Tone::Active),
        CardKind::Close => ("Close", Tone::Plain),
    }
}

/// What `n` cards of a kind are called in the day's count: the tag in lower case, except for turns, which are replies.
fn count_noun(kind: CardKind, n: usize) -> String {
    match kind {
        CardKind::Turn => (if n == 1 { "reply" } else { "replies" }).to_owned(),
        _ => kind_tag(kind).0.to_lowercase(),
    }
}

/// The frame a card is dealt in: cards that wait on the user to read and decide get one.
fn deal_frame(kind: CardKind) -> Option<Tone> {
    match kind {
        CardKind::Turn => Some(Tone::Turn),
        CardKind::Permission | CardKind::Question | CardKind::GuardFlag => Some(Tone::Interview),
        _ => None,
    }
}

fn check_tag(check: Check) -> (&'static str, Tone) {
    match check {
        Check::Pass => ("Pass", Tone::Hired),
        Check::Flag => ("Flag", Tone::Interview),
        Check::Fail => ("Fail", Tone::Closed),
    }
}

/// Keys never fire while a text field has focus. Y does nothing on a card with no default.
fn pressed_key(ui: &Ui, top: Option<&Card>) -> Option<Pressed> {
    if ui.ctx().egui_wants_keyboard_input() {
        return None;
    }
    let press = |key| ui.ctx().input_mut(|input| input.consume_key(Modifiers::NONE, key));
    if press(Key::U) {
        return Some(Pressed::Undo);
    }
    let top = top?;
    if press(Key::Y) {
        return top.primary.clone().map(Pressed::Act);
    }
    if press(Key::L) {
        return Some(Pressed::Later);
    }
    (press(Key::N) && top.can_skip).then_some(Pressed::Skip)
}

/// Draws the tray and applies what the user did. `keys` is off while a dialog covers the window.
pub fn show(ui: &mut Ui, tray: &mut InTray, queue: &Queue, keys: bool) -> Option<Request> {
    let top = queue.cards.first();
    if let Some(top) = top {
        tray.read(&top.key);
    }
    let mut pressed = if keys { pressed_key(ui, top) } else { None };

    let headline = queue.headline();
    page_head(ui, Title::Lead(&headline), Some(Subtitle::Text("Paused runs come first, then deadlines, then the rest.")), |_| {});
    page_body(ui, |ui| {
        if let Some(issue) = tray.issue() {
            alert(ui, issue);
        }
        horizon(ui, queue, keys, &mut pressed);
        ui.add_space(12.0);

        side_column("tray_side", TRAY_RAIL).show(ui, |ui| cleared(ui, queue, &mut pressed));
        egui::CentralPanel::default().frame(Frame::new().inner_margin(Margin { right: 7, bottom: 7, ..Default::default() })).show(ui, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match top {
                Some(card) => stack(ui, card, queue.cards.len() - 1, &mut pressed),
                None => clear(ui, queue, &mut pressed),
            });
        });
    });

    let pressed = pressed?;
    ui.ctx().request_repaint();
    apply(tray, pressed, top, queue)
}

fn apply(tray: &mut InTray, pressed: Pressed, top: Option<&Card>, queue: &Queue) -> Option<Request> {
    let today = queue.today;
    match pressed {
        Pressed::Undo => tray.undo(today),
        Pressed::Reason(reason) => tray.give_reason(reason, today),
        Pressed::Bring(key) => tray.pull_forward(&key, today),
        Pressed::BringAll => tray.bring_back_all(today),
        Pressed::Later => tray.later(top?, today),
        Pressed::Skip => tray.skip(top?, today),
        Pressed::Act(action) => return tray.follow(top?, &action, today),
    }
    None
}

/// `active` is off while a dialog covers the tray, so a focused chip keeps its tip to itself.
fn horizon(ui: &mut Ui, queue: &Queue, active: bool, pressed: &mut Option<Pressed>) {
    let days: Vec<_> = (0..HORIZON_DAYS as u64).map(|offset| queue.today + chrono::Days::new(offset)).collect();
    let note = queue.horizon_note();
    let tomorrow = queue.today + chrono::Days::new(1);
    horizon_strip(ui, &days, note.as_deref(), |ui, day, today| {
        horizon_day(ui, &day.format("%a %-d").to_string(), today);
        for mark in queue.marks.iter().filter(|mark| mark.on == *day) {
            horizon_mark(ui, mark, pressed);
        }
        if today {
            let kinds: Vec<(usize, String)> = queue.by_kind().into_iter().map(|(n, kind)| (n, kind.map_or("other".to_owned(), |kind| count_noun(kind, n)))).collect();
            let kinds: Vec<(usize, &str)> = kinds.iter().map(|(n, kind)| (*n, kind.as_str())).collect();
            horizon_count(ui, queue.cards.len(), queue.overdue(), &kinds);
        } else if *day == tomorrow && !queue.set_aside.is_empty() {
            return_chip(ui, queue, active, pressed);
        }
    });
}

/// Names come from postings, so they show as plain labels only.
fn return_chip(ui: &mut Ui, queue: &Queue, active: bool, pressed: &mut Option<Pressed>) {
    let n = queue.set_aside.len();
    let names = queue.set_aside_distinct();
    let pronoun = if n == 1 { "it" } else { "them" };
    let label = format!("{n} set aside, back tomorrow: {}. Bring {pronoun} back now.", names.join(", "));
    let chip = horizon_return(ui, &format!("{n} back"), &label, active, |ui| {
        strong(ui, "Set aside with Later");
        for name in &names {
            ui.label(*name);
        }
        ui.add_space(4.0);
        ui.label(rich(ui, format!("Back in the tray tomorrow. Click to bring {pronoun} back now."), Type::NOTE, Ink::Muted));
    });
    if chip.clicked() {
        *pressed = Some(Pressed::BringAll);
    }
}

fn horizon_mark(ui: &mut Ui, mark: &Mark, pressed: &mut Option<Pressed>) {
    let tone = mark.active.then(|| kind_tag(mark.kind).1);
    if strip_mark(ui, mark.label.as_str(), tone).clicked() {
        *pressed = Some(Pressed::Bring(mark.key.clone()));
    }
}

/// The top card on the visible edges of what is left. The edge count caps, the headline counts.
fn stack(ui: &mut Ui, card: &Card, left: usize, pressed: &mut Option<Pressed>) {
    let (kind, tone) = kind_tag(card.kind);
    ui.spacing_mut().item_spacing.y = 0.0;
    card_tab(ui, kind, tone, card.when.as_str());
    deal(ui, deal_frame(card.kind), |ui| card_face(ui, card, pressed));
    stack_edges(ui, left);
    ui.add_space(4.0);
    if left > 0 {
        faint_mono(ui, format!("{left} behind this one"));
    }
}

fn card_face(ui: &mut Ui, card: &Card, pressed: &mut Option<Pressed>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(rich(ui, card.title.as_str(), Type::DEAL, Ink::Text));
        muted(ui, card.subtitle.as_str());
    });
    ui.label(rich(ui, card.take.as_str(), Type::TAKE, Ink::Text));
    match &card.body {
        Body::Empty => {}
        Body::Triage { score, band, gates, strengths, gaps } => triage_body(ui, *score, band, gates, strengths, gaps),
        Body::Lines(lines) => lines_body(ui, lines),
        Body::Code { label, text } => code_body(ui, label, text),
        Body::Options(options) => options_body(ui, options),
    }
    actions(ui, card, pressed);
    faint_mono(ui, card.trace.as_str());
}

fn triage_body(ui: &mut Ui, score: u8, band: &str, gates: &[GateRow], strengths: &[String], gaps: &[String]) {
    let tone = match score {
        75.. => Tone::Hired,
        60..=74 => Tone::Active,
        _ => Tone::Interview,
    };
    ui.horizontal(|ui| {
        stamp(ui, &score.to_string(), tone);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            strong(ui, band);
            muted(ui, "Triage score, from posting text only");
        });
    });
    for row in gates.chunks(GATE_COLUMNS) {
        ui.columns(GATE_COLUMNS, |columns| {
            for (ui, gate) in columns.iter_mut().zip(row) {
                gate_cell(ui, gate);
            }
        });
    }
    ui.columns_const(|[left, right]| {
        bullets(left, "Strengths", strengths);
        bullets(right, "Gaps", gaps);
    });
}

fn gate_cell(ui: &mut Ui, gate: &GateRow) {
    let (text, tone) = check_tag(gate.check);
    divider(ui);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            tag(ui, text, tone);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(rich(ui, gate.label, Type::SMALL, Ink::Muted));
                    ui.add(egui::Label::new(gate.value.as_str()).truncate());
                });
            });
        });
    });
}

fn bullets(ui: &mut Ui, title: &str, items: &[String]) {
    ui.spacing_mut().item_spacing.y = 3.0;
    strong(ui, title);
    for item in items {
        muted(ui, item);
    }
}

fn lines_body(ui: &mut Ui, lines: &[(&'static str, String)]) {
    egui::Grid::new("card_lines").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
        for (label, value) in lines {
            muted(ui, *label);
            ui.add(egui::Label::new(value.as_str()).wrap());
            ui.end_row();
        }
    });
}

/// Run input and posting text stay plain text in a bounded box.
fn code_body(ui: &mut Ui, label: &str, text: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.label(rich(ui, label, Type::SMALL, Ink::Muted));
        code_box(ui, |ui| {
            ScrollArea::vertical().id_salt("card_code").max_height(CODE_HEIGHT).auto_shrink([false, true]).show(ui, |ui| {
                ui.add(egui::Label::new(rich(ui, text, Type::MONO, Ink::Text)).wrap());
            });
        });
    });
}

fn options_body(ui: &mut Ui, options: &[(String, String)]) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for (label, description) in options {
            strong(ui, label.as_str());
            muted(ui, description.as_str());
        }
    });
}

fn actions(ui: &mut Ui, card: &Card, pressed: &mut Option<Pressed>) {
    ui.horizontal_wrapped(|ui| {
        if let Some(primary) = &card.primary
            && primary_key_button(ui, &primary.label, "Y").clicked()
        {
            *pressed = Some(Pressed::Act(primary.clone()));
        }
        for action in &card.others {
            if button(ui, &action.label).clicked() {
                *pressed = Some(Pressed::Act(action.clone()));
            }
        }
        if card.can_skip && key_button(ui, "Skip", "N").clicked() {
            *pressed = Some(Pressed::Skip);
        }
        if key_button(ui, "Later", "L").clicked() {
            *pressed = Some(Pressed::Later);
        }
        muted(ui, card.note.as_str());
    });
}

fn clear(ui: &mut Ui, queue: &Queue, pressed: &mut Option<Pressed>) {
    clear_panel(ui, |ui| {
        let title = if queue.set_aside.is_empty() { "Tray clear" } else { "Nothing else for now" };
        ui.label(rich(ui, title, Type::CLEAR, Ink::Text));
        if !queue.set_aside.is_empty() {
            muted(ui, format!("Set aside: {}. They return tomorrow. A paused run stays paused until you answer.", queue.set_aside_names()));
            if primary_button(ui, "Bring them back").clicked() {
                *pressed = Some(Pressed::BringAll);
            }
        } else if queue.unranked > 0 {
            muted(ui, format!("{} new postings wait for scoring.", queue.unranked));
            command_button(ui, "/rank");
        } else {
            muted(ui, "Search your portals for new postings.");
            command_button(ui, "/scrape");
        }
    });
}

/// The last decision heads the log while it can be undone, so its row is not drawn twice.
/// Later is never logged: its row heads the rail without counting. The rail sits outside the card's scroll, so the row stays in view.
fn cleared(ui: &mut Ui, queue: &Queue, pressed: &mut Option<Pressed>) {
    slip(ui, |ui| {
        let count = (!queue.cleared.is_empty()).then(|| queue.cleared.len().to_string());
        panel_header(ui, "Cleared today", count.as_deref());
        let rows = queue.log_under();
        let last = rows.is_empty();
        match &queue.undo {
            Some(offer) => undo(ui, offer, last, pressed),
            None if last => panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                muted(ui, "Nothing yet.");
            }),
            None => {}
        }
        if last {
            return;
        }
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            for entry in rows {
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(entry.name.as_str()).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| muted(ui, entry.label.as_str()));
                });
            }
        });
    });
}

/// The reasons show only on a skip. `last` rounds the row's foot when no log row follows it.
fn undo(ui: &mut Ui, offer: &UndoOffer, last: bool, pressed: &mut Option<Pressed>) {
    let reasons = SkipReason::ALL.map(|reason| (reason.label(), offer.reason == Some(reason)));
    let toggles: &[(&str, bool)] = if offer.asks_reason { &reasons } else { &[] };
    let press = undo_row(ui, &offer.name, &offer.note, &offer.spoken, toggles, last);
    if press.undo {
        *pressed = Some(Pressed::Undo);
    } else if let Some(index) = press.toggle {
        *pressed = Some(Pressed::Reason(SkipReason::ALL[index]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_are_counted_as_replies() {
        assert_eq!(count_noun(CardKind::Turn, 1), "reply");
        assert_eq!(count_noun(CardKind::Turn, 2), "replies");
        assert_eq!(count_noun(CardKind::GuardFlag, 2), "guard flag");
    }
}
