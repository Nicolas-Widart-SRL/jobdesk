use egui::{Button, Frame, Key, Margin, Modifiers, Panel, RichText, ScrollArea, Sense, Shadow, Stroke, Ui};

use crate::intray::application::{Action, Body, Card, CardKind, Check, GateRow, InTray, Mark, Queue, Request, SkipReason, UndoOffer};
use crate::ui_kit::{Palette, Tone, command_button, divider, faint_mono, muted, panel, panel_body, panel_header, primary_button, semibold, stack_edge, stamp, strong, tag};

const HORIZON_DAYS: u64 = 14;
const MOST_EDGES: usize = 6;
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
        CardKind::Send => ("Send", Tone::Drafted),
        CardKind::Triage => ("Triage", Tone::Closed),
        CardKind::Prep => ("Prep", Tone::Interview),
        CardKind::Offer => ("Offer", Tone::Offer),
        CardKind::GuardFlag => ("Guard flag", Tone::Interview),
        CardKind::FollowUp => ("Follow-up", Tone::Active),
        CardKind::Close => ("Close", Tone::Plain),
    }
}

fn waits_for_reading(kind: CardKind) -> bool {
    matches!(kind, CardKind::Permission | CardKind::Question | CardKind::GuardFlag)
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

    ui.label(RichText::new(queue.headline()).size(24.0).family(semibold()));
    muted(ui, "Paused runs come first, then deadlines, then the rest.");
    ui.add_space(12.0);
    if let Some(issue) = tray.issue() {
        ui.label(RichText::new(issue).color(Palette::of(ui).closed));
    }
    horizon(ui, queue, &mut pressed);
    ui.add_space(12.0);

    let offer = tray.undo_offer();
    Panel::right("tray_side").exact_size(280.0).resizable(false).frame(Frame::new().inner_margin(Margin { left: 12, ..Default::default() })).show(ui, |ui| cleared(ui, queue));
    egui::CentralPanel::default().frame(Frame::new().inner_margin(Margin { right: 7, bottom: 7, ..Default::default() })).show(ui, |ui| {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            match top {
                Some(card) => stack(ui, card, queue.cards.len() - 1, &mut pressed),
                None => clear(ui, queue, &mut pressed),
            }
            if let Some(offer) = &offer {
                undo_bar(ui, offer, &mut pressed);
            }
        });
    });

    let pressed = pressed?;
    ui.ctx().request_repaint();
    apply(tray, pressed, top, queue)
}

fn apply(tray: &mut InTray, pressed: Pressed, top: Option<&Card>, queue: &Queue) -> Option<Request> {
    let today = queue.today;
    match pressed {
        Pressed::Undo => tray.undo(),
        Pressed::Reason(reason) => tray.give_reason(reason),
        Pressed::Bring(key) => tray.pull_forward(&key, today),
        Pressed::BringAll => tray.bring_back_all(today),
        Pressed::Later => tray.later(top?, today),
        Pressed::Skip => tray.skip(top?, today),
        Pressed::Act(action) => return tray.follow(top?, &action, today),
    }
    None
}

fn horizon(ui: &mut Ui, queue: &Queue, pressed: &mut Option<Pressed>) {
    let p = Palette::of(ui);
    let days: Vec<_> = (0..HORIZON_DAYS).map(|offset| queue.today + chrono::Days::new(offset)).collect();
    Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).corner_radius(8).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.columns(days.len(), |columns| {
            for (index, (ui, day)) in columns.iter_mut().zip(&days).enumerate() {
                let fill = if index == 0 { p.raised } else { p.panel };
                Frame::new().fill(fill).inner_margin(Margin::symmetric(6, 6)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(58.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(RichText::new(day.format("%a %-d").to_string()).small().color(if index == 0 { p.text } else { p.muted }));
                        for mark in queue.marks.iter().filter(|mark| mark.on == *day) {
                            horizon_mark(ui, mark, pressed);
                        }
                    });
                });
            }
        });
    });
}

fn horizon_mark(ui: &mut Ui, mark: &Mark, pressed: &mut Option<Pressed>) {
    let p = Palette::of(ui);
    let color = if mark.active { kind_tag(mark.kind).1.color(&p) } else { p.faint };
    let mut text = RichText::new(mark.label.as_str()).small().color(color);
    if !mark.active {
        text = text.strikethrough();
    }
    let label = egui::Label::new(text).wrap().sense(if mark.active { Sense::click() } else { Sense::hover() });
    if ui.add(label).clicked() {
        *pressed = Some(Pressed::Bring(mark.key.clone()));
    }
}

/// The top card on the visible edges of what is left. The edge count caps, the headline counts.
fn stack(ui: &mut Ui, card: &Card, left: usize, pressed: &mut Option<Pressed>) {
    let p = Palette::of(ui);
    let (kind, tone) = kind_tag(card.kind);
    let waits = waits_for_reading(card.kind);
    ui.spacing_mut().item_spacing.y = 0.0;
    Frame::new().fill(p.panel).corner_radius(egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }).inner_margin(Margin::symmetric(14, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(kind).family(semibold()).color(tone.color(&p)));
            muted(ui, card.when.as_str());
        });
    });
    let deal = Frame::new().fill(p.panel).corner_radius(egui::CornerRadius { nw: 0, ne: 4, sw: 4, se: 4 }).inner_margin(Margin::symmetric(20, 18));
    let deal = if waits { deal.stroke(Stroke::new(1.0, p.interview)).shadow(Shadow { offset: [5, 5], blur: 0, spread: 0, color: p.interview }) } else { deal };
    deal.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 10.0;
        card_face(ui, card, pressed);
    });
    if waits {
        ui.add_space(5.0);
    }
    for edge in 0..left.min(MOST_EDGES) {
        stack_edge(ui, (edge as f32 + 1.0) * 5.0);
    }
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.add_space(4.0);
    if left > 0 {
        faint_mono(ui, format!("{left} behind this one"));
    }
}

fn card_face(ui: &mut Ui, card: &Card, pressed: &mut Option<Pressed>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(RichText::new(card.title.as_str()).size(20.0).family(semibold()));
        muted(ui, card.subtitle.as_str());
    });
    ui.label(RichText::new(card.take.as_str()).size(14.0));
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
        let width = ui.available_width();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            tag(ui, text, tone);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_max_width(width);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(RichText::new(gate.label).small().color(Palette::of(ui).muted));
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
    let p = Palette::of(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.label(RichText::new(label).small().color(p.muted));
        Frame::new().fill(p.bg).corner_radius(3).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ScrollArea::vertical().id_salt("card_code").max_height(CODE_HEIGHT).auto_shrink([false, true]).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(text).monospace()).wrap());
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
            && primary_button(ui, &format!("{}   Y", primary.label)).clicked()
        {
            *pressed = Some(Pressed::Act(primary.clone()));
        }
        for action in &card.others {
            if ui.button(action.label.as_str()).clicked() {
                *pressed = Some(Pressed::Act(action.clone()));
            }
        }
        if card.can_skip && ui.button("Skip   N").clicked() {
            *pressed = Some(Pressed::Skip);
        }
        if ui.button("Later   L").clicked() {
            *pressed = Some(Pressed::Later);
        }
        muted(ui, card.note.as_str());
    });
}

fn clear(ui: &mut Ui, queue: &Queue, pressed: &mut Option<Pressed>) {
    panel(ui, |ui| {
        Frame::new().inner_margin(Margin::symmetric(24, 40)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = if queue.set_aside.is_empty() { "Tray clear" } else { "Nothing else for now" };
            ui.label(RichText::new(title).size(24.0).family(semibold()));
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
    });
}

fn undo_bar(ui: &mut Ui, offer: &UndoOffer, pressed: &mut Option<Pressed>) {
    let p = Palette::of(ui);
    ui.add_space(8.0);
    Frame::new().fill(p.raised).stroke(Stroke::new(1.0, p.line)).corner_radius(4).inner_margin(Margin::symmetric(14, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(offer.text.as_str());
            if offer.asks_reason {
                for reason in SkipReason::ALL {
                    let on = offer.reason == Some(reason);
                    let text = RichText::new(reason.label()).color(if on { p.on_primary } else { p.text });
                    let button = Button::new(text).fill(if on { p.primary } else { p.panel });
                    if ui.add(button).clicked() {
                        *pressed = Some(Pressed::Reason(reason));
                    }
                }
            }
            if ui.button("Undo   U").clicked() {
                *pressed = Some(Pressed::Undo);
            }
        });
    });
}

fn cleared(ui: &mut Ui, queue: &Queue) {
    panel(ui, |ui| {
        let count = (!queue.cleared.is_empty()).then(|| queue.cleared.len().to_string());
        panel_header(ui, "Cleared today", count.as_deref());
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            if queue.cleared.is_empty() {
                muted(ui, "Nothing yet.");
            }
            for entry in &queue.cleared {
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(entry.name.as_str()).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| muted(ui, entry.label.as_str()));
                });
            }
        });
    });
}
