use chrono::NaiveDate;
use egui::{Align, Frame, Key, Layout, Margin, Modifiers, Rect, ScrollArea, Ui};

use crate::intray::application::{Action, AgendaRow, Body, Card, CardKind, GateRow, HORIZON_WORDS, InTray, Queue, Request, Returning, RowState, SkipReason, UndoOffer};
use crate::ui_kit::{
    AgendaEnd, CheckMark, DealLook, Ink, Tone, Type, agenda_return, agenda_row, blank_card_tab, button, card_skeleton, card_tab, check_tag, code_box, command_action, day_stamp, departing,
    deal, divider, fat_tally, desk_label, key_button, lead_line, link, muted, page_body, panel_body, panel_header, paper, primary_command_action, primary_key_button, reveal_command, rich, side_column, skeleton,
    skeleton_lines, slip, stamp, stamped_head, strong, undo_row, wrapped_height,
};
use crate::ui_kit::tokens::{BUTTON_HEIGHT, DEAL_ASIDE_MIN, MOTION, MOTION_TIME, OFFER_GAP, OFFER_PADDING, OFFER_WIDTH, PICK_ROW_INLINE, RAIL_GAP, SKELETON_GAP, SKELETON_LINE, TRAY_RAIL};

const GATE_COLUMNS: usize = 3;
const GATE_CELL_GAP: f32 = 7.0;
const STAMP_GAP: f32 = 14.0;
/// Under every score, whatever the take says: the score rests on the job ad and nothing else.
const SCORE_SOURCE: &str = "Job ad only";
const CODE_HEIGHT: f32 = 150.0;
/// The room above the line that closes Coming up.
const CLOSING_GAP: f32 = 6.0;

enum Pressed {
    Act(Action),
    Later,
    Skip,
    Undo,
    Reason(SkipReason),
    Bring(String),
    BringAll,
}

/// A kind's name and its tab's tone.
/// Only what Claude asks of the user takes the stamp, `Turn`: those cards hold a paused or waiting run.
fn kind_look(kind: CardKind) -> (&'static str, Tone) {
    match kind {
        CardKind::Permission | CardKind::Question => ("Claude asks", Tone::Turn),
        CardKind::Turn => ("Your turn", Tone::Turn),
        CardKind::Send => ("Ready to send", Tone::Drafted),
        CardKind::Triage => ("New match", Tone::Active),
        CardKind::Prep => ("Interview prep", Tone::Interview),
        CardKind::Offer => ("Offer", Tone::Offer),
        CardKind::GuardFlag => ("Check this ad", Tone::Interview),
        CardKind::FollowUp => ("Follow up", Tone::Active),
        CardKind::Close => ("Close it?", Tone::Plain),
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

/// What the Today screen keeps between frames.
#[derive(Default)]
pub struct TodayScreen {
    /// The card whose command line is open. It closes when another card comes on top.
    command_open: Option<String>,
    /// A copy of the card just decided, while it slides off. The queue has already moved past it.
    departing: Option<Departing>,
}

/// The decided card as it was dealt, with its place count and when it was decided.
struct Departing {
    card: Card,
    pending: usize,
    since: f64,
}

/// What Today takes from outside the tray.
pub struct TodayContext<'a> {
    /// The screens whose source could not be read, so their cards are missing from this list.
    pub missing: &'a [&'a str],
    /// Which dates Coming up lacks because their source could not be read, as a sentence.
    pub missing_dates: Option<&'a str>,
    /// Draws the alerts for sources that could not be read, under the head.
    pub alerts: &'a dyn Fn(&mut Ui),
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
pub fn show(ui: &mut Ui, screen: &mut TodayScreen, tray: &mut InTray, queue: &Queue, context: &TodayContext, keys: bool) -> Option<Request> {
    let top = queue.cards.first();
    if let Some(top) = top {
        tray.read(&top.key);
    }
    // Another card on top, or an empty tray, closes the line, so a card back after Later starts closed.
    if screen.command_open.as_ref().is_some_and(|key| top.is_none_or(|card| &card.key != key)) {
        screen.command_open = None;
    }
    let mut pressed = if keys { pressed_key(ui, top) } else { None };
    let now = ui.input(|input| input.time);
    // Undo, or an action that leaves the card on top, drops the copy at once.
    if screen.departing.as_ref().is_some_and(|gone| now - gone.since >= f64::from(MOTION_TIME) || top.is_some_and(|card| card.key == gone.card.key)) {
        screen.departing = None;
    }

    head(ui, queue, context.missing);
    page_body(ui, |ui| {
        (context.alerts)(ui);
        side_column("tray_side", TRAY_RAIL).show(ui, |ui| rail(ui, queue, keys, context.missing_dates, &mut pressed));
        egui::CentralPanel::default().frame(Frame::new().inner_margin(Margin { right: 7, bottom: 7, ..Default::default() })).show(ui, |ui| {
            let room = ui.available_rect_before_wrap();
            match top {
                Some(card) => stack(ui, card, queue.cards.len(), &mut screen.command_open, &mut pressed),
                None => {
                    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| clear(ui, queue, &mut pressed));
                }
            }
            if let Some(gone) = &screen.departing {
                departed(ui, gone, room, now);
            }
        });
    });

    if screen.departing.is_some() {
        ui.ctx().request_repaint();
    }
    let pressed = pressed?;
    ui.ctx().request_repaint();
    if let (true, Some(card), Pressed::Act(_) | Pressed::Later | Pressed::Skip) = (MOTION, top, &pressed) {
        screen.departing = Some(Departing { card: card.clone(), pending: queue.cards.len(), since: now });
    }
    apply(tray, pressed, top, queue)
}

/// The day stamp ticks off today's decisions. Cards set aside with Later count on neither side.
fn head(ui: &mut Ui, queue: &Queue, missing: &[&str]) {
    let tally = queue.tally();
    let stamp = |ui: &mut Ui| dated_stamp(ui, queue.today, tally.total, tally.done);
    stamped_head(ui, stamp, &queue.today_headline(missing), &queue.today_subtitle(missing));
}

/// With nothing dealt, the stamp's ring is empty and it reads as the date alone.
pub fn dated_stamp(ui: &mut Ui, today: NaiveDate, total: usize, done: usize) {
    let date = today.format("%A %-d %B").to_string();
    let spoken = if total == 0 { date } else { format!("{date}. {done} of {total} decided today.") };
    let day = |pattern| today.format(pattern).to_string();
    day_stamp(ui, &day("%a"), &day("%-d"), &day("%b"), total, done, &spoken);
}

/// Today while the first snapshot loads: an empty stamp, then grey blocks where the card and the rail will be.
/// It takes no keys, so nothing can be decided before the cards are known.
pub fn loading(ui: &mut Ui, today: NaiveDate) {
    stamped_head(ui, |ui| dated_stamp(ui, today, 0, 0), "Reading your job search…", "Jobs, applications and tasks, from your folder.");
    page_body(ui, |ui| {
        side_column("tray_side", TRAY_RAIL).show(ui, loading_rail);
        egui::CentralPanel::default().frame(Frame::new().inner_margin(Margin { right: 7, bottom: 7, ..Default::default() })).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let tab = blank_card_tab(ui);
            let look = DealLook { key: "loading", frame: None, behind: 0 };
            deal(ui, tab, look, loading_face, |ui| {
                ui.label(rich(ui, "Your first card appears in a moment.", Type::BODY, Ink::Text));
            });
        });
    });
}

/// The shape of a new match: score and names, the verdict, the checks, then why it fits and what is missing.
fn loading_face(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = STAMP_GAP;
        card_skeleton(ui, 68.0, 56.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = SKELETON_GAP;
            card_skeleton(ui, 170.0, SKELETON_LINE);
            card_skeleton(ui, 370.0, 20.0);
            card_skeleton(ui, 232.0, SKELETON_LINE);
        });
    });
    card_skeleton(ui, 528.0, SKELETON_LINE);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for _ in 0..2 {
            ui.columns(GATE_COLUMNS, |columns| {
                for ui in columns {
                    divider(ui);
                    ui.add_space(GATE_CELL_GAP);
                    skeleton_lines(ui, &[108.0, 189.0], card_skeleton);
                    ui.add_space(GATE_CELL_GAP);
                }
            });
        }
    });
    ui.columns_const(|[left, right]| {
        skeleton_lines(left, &[125.0, 333.0, 291.0], card_skeleton);
        skeleton_lines(right, &[125.0, 250.0], card_skeleton);
    });
}

/// Done today and Coming up, with grey lines for rows.
fn loading_rail(ui: &mut Ui) {
    for (index, (title, widths)) in [("Done today", &[238.0, 158.0][..]), ("Coming up", &[211.0, 185.0, 198.0][..])].into_iter().enumerate() {
        if index > 0 {
            ui.add_space(RAIL_GAP);
        }
        slip(ui, |ui| {
            panel_header(ui, title, None);
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                skeleton_lines(ui, widths, skeleton);
            });
        });
    }
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

/// The top card of `pending`, on the visible edges of what is left. The edge count caps, the place counts.
/// The card takes the height left, so its actions stay in view however long its body is.
fn stack(ui: &mut Ui, card: &Card, pending: usize, command_open: &mut Option<String>, pressed: &mut Option<Pressed>) {
    let (kind, ink) = kind_look(card.kind);
    ui.spacing_mut().item_spacing.y = 0.0;
    let tab = ui
        .horizontal(|ui| {
            let tab = card_tab(ui, kind, ink, card.when.as_str());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                desk_label(ui, format!("1 of {pending}"), Type::NOTE, Ink::Muted).on_hover_ui(|ui| {
                    ui.label(format!("Claude's questions come first, then deadlines in the next {}, then the rest.", HORIZON_WORDS.1));
                });
            });
            tab
        })
        .inner;
    let look = DealLook { key: &card.key, frame: deal_frame(card.kind), behind: pending - 1 };
    deal(ui, tab, look, |ui| card_face(ui, card), |ui| actions(ui, card, command_open, pressed));
}

/// The decided card, drawn from its copy over the next one. Its actions go nowhere.
fn departed(ui: &Ui, gone: &Departing, room: Rect, now: f64) {
    let part = ((now - gone.since) as f32 / MOTION_TIME).clamp(0.0, 1.0);
    departing(ui, room, part, |ui| stack(ui, &gone.card, gone.pending, &mut None, &mut None));
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
            if let Some(mark) = CheckMark::of(gate.check) {
                check_tag(ui, mark);
            }
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
fn actions(ui: &mut Ui, card: &Card, command_open: &mut Option<String>, pressed: &mut Option<Pressed>) {
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
            // The line stays open only while its card stays on top: `show` closes it when the top card changes.
            let mut open = command_open.as_deref() == Some(card.key.as_str());
            reveal_command(ui, command, &mut open);
            *command_open = open.then(|| card.key.clone());
        }
    });
}

/// Nothing is left to decide, so the panel offers new work. Both buttons only open the launcher: the user starts the run there.
fn clear(ui: &mut Ui, queue: &Queue, pressed: &mut Option<Pressed>) {
    let height = ui.available_height();
    paper(ui, false, |ui| {
        ui.set_min_height(height - 2.0);
        Frame::new().inner_margin(Margin::same(OFFER_PADDING)).show(ui, |ui| {
            ui.set_max_width(OFFER_WIDTH);
            ui.spacing_mut().item_spacing.y = OFFER_GAP;
            ui.label(rich(ui, "Want more to look at?", Type::SLIP_TITLE, Ink::Text));
            let intro = "Jobdesk can search your job sites for new ads and score them against your profile. Each opens as a task, and you start it there.";
            ui.label(rich(ui, intro, Type::BODY, Ink::Muted));
            ui.horizontal(|ui| {
                primary_command_action(ui, "Find new jobs", "/scrape");
                if queue.unranked > 0 {
                    let jobs = if queue.unranked == 1 { "1 new job".to_owned() } else { format!("{} new jobs", queue.unranked) };
                    command_action(ui, &format!("Score {jobs}"), "/rank");
                }
            });
            if !queue.set_aside.is_empty() {
                let n = queue.set_aside.len();
                let (cards, verb, back) = if n == 1 { ("card", "comes", "Bring it back now") } else { ("cards", "come", "Bring them back now") };
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(rich(ui, format!("{n} {cards} set aside with Later {verb} back tomorrow. "), Type::NOTE, Ink::Muted));
                    if link(ui, back).clicked() {
                        *pressed = Some(Pressed::BringAll);
                    }
                });
            }
        });
    });
}

/// Done today over Coming up. The rail sits outside the card's scroll, so the undo row at its top stays in view.
/// It scrolls on its own when a long log pushes Coming up past the window. `active` is off while a dialog covers the tray.
fn rail(ui: &mut Ui, queue: &Queue, active: bool, missing_dates: Option<&str>, pressed: &mut Option<Pressed>) {
    ScrollArea::vertical().id_salt("tray_rail").auto_shrink([false, false]).show(ui, |ui| {
        done_today(ui, queue, pressed);
        ui.add_space(RAIL_GAP);
        coming_up(ui, queue, active, missing_dates, pressed);
    });
}

/// The last decision heads the log while it can be undone, so its row is not drawn twice.
/// Later is never logged: its row heads the log without counting, and the tally leaves the card out.
fn done_today(ui: &mut Ui, queue: &Queue, pressed: &mut Option<Pressed>) {
    slip(ui, |ui| {
        let tally = queue.tally();
        let count = (tally.total > 0).then(|| format!("{} of {}", tally.done, tally.total));
        panel_header(ui, "Done today", count.as_deref());
        let rows = queue.log_under();
        let last = rows.is_empty();
        let empty = queue.undo.is_none() && last;
        if tally.total > 0 || empty {
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                if tally.total > 0 {
                    fat_tally(ui, tally.done, tally.total, Ink::Text);
                }
                if empty {
                    ui.add(egui::Label::new(rich(ui, "Each decision lands here. U undoes the last one.", Type::NOTE, Ink::Muted)).wrap());
                }
            });
        }
        if let Some(offer) = &queue.undo {
            undo(ui, offer, last, pressed);
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

/// Rows and the return row only reorder the queue: none opens the launcher or starts a run.
/// When a source is missing, a line after the closing says which dates are. It replaces "No deadlines…", which it would contradict.
fn coming_up(ui: &mut Ui, queue: &Queue, active: bool, missing_dates: Option<&str>, pressed: &mut Option<Pressed>) {
    let agenda = queue.agenda();
    slip(ui, |ui| {
        panel_header(ui, "Coming up", Some(HORIZON_WORDS.0));
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(returning) = &agenda.returning {
                return_row(ui, returning, active, pressed);
            }
            for (index, row) in agenda.rows.iter().enumerate() {
                agenda_line(ui, row, index == 0 && agenda.returning.is_none(), pressed);
            }
            if agenda.returning.is_some() || !agenda.rows.is_empty() {
                divider(ui);
                ui.add_space(CLOSING_GAP);
            }
            let closing = (missing_dates.is_none() || !agenda.rows.is_empty()).then_some(agenda.closing.as_str());
            for line in closing.into_iter().chain(missing_dates) {
                // The line starts under the rows' day column.
                ui.horizontal(|ui| {
                    ui.add_space(PICK_ROW_INLINE);
                    ui.add(egui::Label::new(rich(ui, line, Type::NOTE, Ink::Muted)).wrap());
                });
            }
        });
    });
}

/// Names come from postings, so they show as plain labels only.
fn return_row(ui: &mut Ui, returning: &Returning, active: bool, pressed: &mut Option<Pressed>) {
    let n = returning.count;
    let (verb, pronoun) = if n == 1 { ("comes", "it") } else { ("come", "them") };
    let label = format!("{n} set aside, back tomorrow: {}. Bring {pronoun} back now.", returning.names.join(", "));
    let day = returning.on.format("%a %-d").to_string();
    let press = agenda_return(ui, &day, &format!("{n} {verb} back"), "Bring back", &label, active, |ui| {
        strong(ui, "Set aside with Later");
        for name in &returning.names {
            ui.label(name.as_str());
        }
        ui.add_space(4.0);
        ui.label(rich(ui, format!("Back in the tray tomorrow. Bring {pronoun} back to decide today."), Type::NOTE, Ink::Muted));
    });
    if press.row || press.button {
        *pressed = Some(Pressed::BringAll);
    }
}

/// A row pulls its card back on top. The row being read and a decided row take no click. The text names a company, so it shows as plain text.
fn agenda_line(ui: &mut Ui, row: &AgendaRow, first: bool, pressed: &mut Option<Pressed>) {
    let late = match row.state {
        RowState::Late(1) => "1 day late".to_owned(),
        RowState::Late(days) => format!("{days} days late"),
        _ => String::new(),
    };
    let end = match row.state {
        RowState::Late(_) => AgendaEnd::Late(&late),
        RowState::Now => AgendaEnd::Now,
        RowState::Done => AgendaEnd::Done,
        RowState::Waiting => AgendaEnd::Nothing,
    };
    let press = agenda_row(ui, &row.on.format("%a %-d").to_string(), &row.text, end, first);
    if press.row {
        *pressed = Some(Pressed::Bring(row.key.clone()));
    }
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
