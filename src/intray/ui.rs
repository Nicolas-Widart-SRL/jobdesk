use egui::{Align, Frame, Key, Layout, Margin, Modifiers, ScrollArea, Ui};

use crate::intray::application::{Action, Body, Card, CardKind, Check, GateRow, HORIZON_DAYS, InTray, Mark, Queue, Request, SkipReason, UndoOffer};
use crate::ui_kit::{
    CheckMark, DealLook, Ink, TabInk, Tone, Type, alert, button, card_tab, check_tag, clear_panel, code_box, command_button, day_stamp, deal, divider, horizon as horizon_strip, horizon_count,
    horizon_day, horizon_mark as strip_mark, horizon_return, key_button, lead_line, muted, page_body, panel_body, panel_header, primary_button, primary_key_button, reveal_command, rich,
    side_column, slip, stamp, stamped_head, strong, undo_row, wrapped_height,
};
use crate::ui_kit::tokens::{BUTTON_HEIGHT, DEAL_ASIDE_MIN, TRAY_RAIL};

const GATE_COLUMNS: usize = 3;
const GATE_CELL_GAP: f32 = 7.0;
const STAMP_GAP: f32 = 14.0;
/// Under every score, whatever the take says: the score rests on the job ad and nothing else.
const SCORE_SOURCE: &str = "Job ad only";
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

/// A kind's name, its tab's ink and its colour on the strip.
/// Only what Claude asks of the user takes the stamp, on the tab: those cards hold a paused run. The strip keeps status tones.
fn kind_look(kind: CardKind) -> (&'static str, TabInk, Tone) {
    let toned = |name, tone| (name, TabInk::Tone(tone), tone);
    match kind {
        CardKind::Permission | CardKind::Question => ("Claude asks", TabInk::Stamp, Tone::Interview),
        CardKind::Turn => ("Your turn", TabInk::Stamp, Tone::Turn),
        CardKind::Send => toned("Ready to send", Tone::Drafted),
        CardKind::Triage => toned("New match", Tone::Active),
        CardKind::Prep => toned("Interview prep", Tone::Interview),
        CardKind::Offer => toned("Offer", Tone::Offer),
        CardKind::GuardFlag => toned("Check this ad", Tone::Interview),
        CardKind::FollowUp => toned("Follow up", Tone::Active),
        CardKind::Close => toned("Close it?", Tone::Plain),
    }
}

/// What `n` cards of a kind are called in the day's count: the tag in lower case, except for turns, which are replies.
fn count_noun(kind: CardKind, n: usize) -> String {
    match kind {
        CardKind::Turn => (if n == 1 { "reply" } else { "replies" }).to_owned(),
        _ => kind_look(kind).0.to_lowercase(),
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

fn check_mark(check: Check) -> CheckMark {
    match check {
        Check::Pass => CheckMark::Ok,
        Check::Flag => CheckMark::Unclear,
        Check::Fail => CheckMark::Fails,
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
    close_command_off_top(ui, top);
    let mut pressed = if keys { pressed_key(ui, top) } else { None };

    head(ui, queue);
    page_body(ui, |ui| {
        if let Some(issue) = tray.issue() {
            alert(ui, issue);
        }
        horizon(ui, queue, keys, &mut pressed);
        ui.add_space(12.0);

        side_column("tray_side", TRAY_RAIL).show(ui, |ui| cleared(ui, queue, &mut pressed));
        egui::CentralPanel::default().frame(Frame::new().inner_margin(Margin { right: 7, bottom: 7, ..Default::default() })).show(ui, |ui| match top {
            Some(card) => stack(ui, card, queue.cards.len(), &mut pressed),
            None => {
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| clear(ui, queue, &mut pressed));
            }
        });
    });

    let pressed = pressed?;
    ui.ctx().request_repaint();
    apply(tray, pressed, top, queue)
}

/// The day stamp ticks off today's decisions. Cards set aside with Later count on neither side.
fn head(ui: &mut Ui, queue: &Queue) {
    let tally = queue.tally();
    let date = queue.today.format("%A %-d %B").to_string();
    let spoken = if tally.total == 0 { date } else { format!("{date}. {} of {} decided today.", tally.done, tally.total) };
    let stamp = |ui: &mut Ui| {
        let day = |pattern| queue.today.format(pattern).to_string();
        day_stamp(ui, &day("%a"), &day("%-d"), &day("%b"), tally.total, tally.done, &spoken);
    };
    stamped_head(ui, stamp, &queue.today_headline(), &queue.today_subtitle(&[]));
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
    let tone = mark.active.then(|| kind_look(mark.kind).2);
    if strip_mark(ui, mark.label.as_str(), tone).clicked() {
        *pressed = Some(Pressed::Bring(mark.key.clone()));
    }
}

/// The top card of `pending`, on the visible edges of what is left. The edge count caps, the place counts.
/// The card takes the height left, so its actions stay in view however long its body is.
fn stack(ui: &mut Ui, card: &Card, pending: usize, pressed: &mut Option<Pressed>) {
    let (kind, ink, _) = kind_look(card.kind);
    ui.spacing_mut().item_spacing.y = 0.0;
    let tab = ui
        .horizontal(|ui| {
            let tab = card_tab(ui, kind, ink, card.when.as_str());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(rich(ui, format!("1 of {pending}"), Type::NOTE, Ink::Muted))
                    .on_hover_text("Claude's questions come first, then deadlines in the next two weeks, then the rest.");
            });
            tab
        })
        .inner;
    let look = DealLook { key: &card.key, frame: deal_frame(card.kind), behind: pending - 1 };
    deal(ui, tab, look, |ui| card_face(ui, card), |ui| actions(ui, card, pressed));
}

/// Any string here may come from a posting, a note or a report, so each shows as a plain label.
fn card_face(ui: &mut Ui, card: &Card) {
    match &card.body {
        Body::Triage { score, band, gates, strengths, gaps } => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = STAMP_GAP;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    stamp(ui, &score.to_string(), score_tone(*score));
                    ui.label(rich(ui, SCORE_SOURCE, Type::SMALL, Ink::Muted)).on_hover_text("Scored from the job ad alone.");
                });
                who(ui, card);
            });
            lead_line(ui, band, card.take.as_str());
            checks(ui, gates);
            ui.columns_const(|[left, right]| {
                bullets(left, "Why it fits", strengths);
                bullets(right, "What is missing", gaps);
            });
        }
        body => {
            who(ui, card);
            ui.label(rich(ui, card.take.as_str(), Type::TAKE, Ink::Text));
            match body {
                Body::Lines(lines) => lines_body(ui, lines),
                Body::Code { label, text } => code_body(ui, label, text),
                Body::Options(options) => options_body(ui, options),
                Body::Empty | Body::Triage { .. } => {}
            }
        }
    }
}

/// The company over the role, then where it is.
fn who(ui: &mut Ui, card: &Card) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        if let Some(over) = &card.over {
            muted(ui, over.as_str());
        }
        ui.label(rich(ui, card.title.as_str(), Type::DEAL, Ink::Text));
        muted(ui, card.subtitle.as_str());
    });
}

fn score_tone(score: u8) -> Tone {
    match score {
        75.. => Tone::Hired,
        60..=74 => Tone::Active,
        _ => Tone::Interview,
    }
}

fn checks(ui: &mut Ui, gates: &[GateRow]) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for row in gates.chunks(GATE_COLUMNS) {
            ui.columns(GATE_COLUMNS, |columns| {
                for (ui, gate) in columns.iter_mut().zip(row) {
                    gate_cell(ui, gate);
                }
            });
        }
    });
}

fn gate_cell(ui: &mut Ui, gate: &GateRow) {
    divider(ui);
    ui.add_space(GATE_CELL_GAP);
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            check_tag(ui, check_mark(gate.check));
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(rich(ui, gate.label, Type::SMALL, Ink::Muted));
                    ui.add(egui::Label::new(gate.value.as_str()).truncate());
                });
            });
        });
    });
    ui.add_space(GATE_CELL_GAP);
}

/// Items come from a report, so each is a plain label. An empty list keeps its heading and says so.
fn bullets(ui: &mut Ui, title: &str, items: &[String]) {
    ui.spacing_mut().item_spacing.y = 3.0;
    strong(ui, title);
    if items.is_empty() {
        muted(ui, "None");
    }
    for item in items {
        ui.horizontal(|ui| {
            ui.label(rich(ui, "-", Type::BODY, Ink::Muted));
            ui.add(egui::Label::new(rich(ui, item.as_str(), Type::BODY, Ink::Text)).wrap());
        });
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

/// The keyed actions lead, then the rest. The note on what Y does sits on the right, or under them when the row is full.
/// The command shows only on request, as plain text, and showing it starts nothing.
fn actions(ui: &mut Ui, card: &Card, pressed: &mut Option<Pressed>) {
    if let Some(primary) = &card.primary
        && primary_key_button(ui, &primary.label, "Y").clicked()
    {
        *pressed = Some(Pressed::Act(primary.clone()));
    }
    if card.can_skip && key_button(ui, "Skip", "N").clicked() {
        *pressed = Some(Pressed::Skip);
    }
    if key_button(ui, "Later", "L").clicked() {
        *pressed = Some(Pressed::Later);
    }
    for action in &card.others {
        if button(ui, &action.label).clicked() {
            *pressed = Some(Pressed::Act(action.clone()));
        }
    }
    let beside = ui.available_size_before_wrap().x >= DEAL_ASIDE_MIN;
    if !beside {
        ui.end_row();
    }
    ui.with_layout(Layout::top_down(Align::Max), |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        if beside && card.command.is_none() {
            // A note on its own centres on the buttons beside it.
            let height = wrapped_height(ui, &card.note, Type::NOTE, ui.available_width());
            ui.add_space(((BUTTON_HEIGHT - height) / 2.0).max(0.0));
        }
        ui.add(egui::Label::new(rich(ui, card.note.as_str(), Type::NOTE, Ink::Muted)).halign(Align::Max).wrap());
        if let Some(command) = &card.command {
            command_line(ui, card, command);
        }
    });
}

/// Names the card whose command line is open.
fn command_open_id() -> egui::Id {
    egui::Id::NULL.with("tray_command_open")
}

/// The line stays open only while its card stays on top: `show` closes it when the top card changes.
fn command_line(ui: &mut Ui, card: &Card, command: &str) {
    let id = command_open_id();
    let mut open = ui.data(|data| data.get_temp::<String>(id)).is_some_and(|key| key == card.key);
    reveal_command(ui, command, &mut open);
    ui.data_mut(|data| {
        if open {
            data.insert_temp(id, card.key.clone());
        } else {
            data.remove::<String>(id);
        }
    });
}

/// Another card on top, or an empty tray, closes the line, so a card back after Later starts closed.
fn close_command_off_top(ui: &Ui, top: Option<&Card>) {
    let id = command_open_id();
    let shown = ui.data(|data| data.get_temp::<String>(id));
    if shown.is_some_and(|key| top.is_none_or(|card| card.key != key)) {
        ui.data_mut(|data| data.remove::<String>(id));
    }
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
