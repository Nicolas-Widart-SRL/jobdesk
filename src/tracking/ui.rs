use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use egui::{Frame, ScrollArea, Ui};

use crate::tracking::application::{ApplicationCard, ApplicationStatus, DeadlineState, FOLLOW_UP_DAYS, MAX_FOLLOW_UPS, Pipeline, PipelineLane, STALE_DAYS, StatusBucket};
use crate::ui_kit::{
    CardLook, Ink, ScreenState, Subtitle, Title, Tone, Type, command_action, divider, empty_note, empty_state, desk_label, index_card, lane as lane_frame, lane_title, muted, note, open_link, page_body, page_head, panel_body,
    paper_with_footer, primary_button, primary_command_action, rich, side_column, small_command_action, stamp_square, strong, tag,
};
use crate::ui_kit::tokens::{BOARD_GAP, NOTE_GAP, SCREEN_GAP};

#[derive(Default)]
pub struct TrackerScreen {
    selected: Option<(String, String)>,
}

pub fn bucket_tone(bucket: StatusBucket) -> Tone {
    match bucket {
        StatusBucket::Drafted => Tone::Drafted,
        StatusBucket::Active => Tone::Active,
        StatusBucket::Interview => Tone::Interview,
        StatusBucket::Offer => Tone::Offer,
        StatusBucket::Hired => Tone::Hired,
        StatusBucket::Closed => Tone::Closed,
    }
}

/// A lane's name in plain words.
fn lane_name(bucket: StatusBucket) -> &'static str {
    match bucket {
        StatusBucket::Drafted => "Not sent yet",
        bucket => bucket.label(),
    }
}

fn day(date: NaiveDate) -> String {
    date.format("%a %-d %b").to_string()
}

fn ago(days: i64) -> String {
    match days {
        0 => "today".into(),
        1 => "yesterday".into(),
        days => format!("{days} days ago"),
    }
}

fn times(count: usize) -> String {
    match count {
        1 => "once".into(),
        2 => "twice".into(),
        count => format!("{count} times"),
    }
}

/// The line under a card's role: what comes next, or how it ended.
fn card_meta(card: &ApplicationCard) -> String {
    if card.is_final {
        return if card.status_label == ApplicationStatus::Withdrawn.label() { "You withdrew".into() } else { card.status_label.clone() };
    }
    match card.bucket {
        StatusBucket::Drafted => match (card.deadline_state, card.deadline) {
            (Some(DeadlineState::Passed), _) => "Deadline passed".into(),
            (_, Some(deadline)) => format!("Send by {}", day(deadline)),
            _ => card.status_label.clone(),
        },
        StatusBucket::Offer => match (card.deadline_state, card.deadline) {
            (Some(DeadlineState::Passed), _) => "Reply date passed".into(),
            (_, Some(deadline)) => format!("Reply by {}", day(deadline)),
            _ => card.status_label.clone(),
        },
        StatusBucket::Interview => match card.outcome.as_ref().and_then(|outcome| outcome.stages.iter().find(|stage| !stage.reached)) {
            Some(stage) => stage.date.map_or_else(|| stage.name.clone(), |date| format!("{} {}", stage.name, day(date))),
            None => card.status_label.clone(),
        },
        StatusBucket::Closed => card.status_label.clone(),
        // Quiet days restart with each dated note, so only the applied date says when it was sent.
        // A recent follow-up is the newer news, so it reads in place of the sent date.
        _ => match (card.days_quiet, card.days_followed_up, card.days_sent) {
            (Some(days), ..) if days >= FOLLOW_UP_DAYS && card.follow_ups_sent > 0 => format!("No reply in {days} days, followed up {}", times(card.follow_ups_sent)),
            (Some(days), ..) if days >= FOLLOW_UP_DAYS => format!("No reply in {days} days"),
            (_, Some(days), _) => format!("Followed up {}", ago(days)),
            (_, None, Some(days)) => format!("Sent {}", ago(days)),
            (_, None, None) => card.status_label.clone(),
        },
    }
}

/// The role of every application with a card in Today, by company, both lowercased as a Today card key is.
type Waiting = HashMap<String, HashSet<String>>;

/// Case does not tell applications apart, so a row renamed only in case still waits.
fn card_waits(card: &ApplicationCard, waiting: &Waiting) -> bool {
    waiting.get(&card.company.to_lowercase()).is_some_and(|roles| roles.contains(&card.role.to_lowercase()))
}

impl TrackerScreen {
    /// `waiting` holds the company and role of every application with a card in Today.
    /// Returns the archive folder of a dossier the user asked to open.
    /// While the tracker cannot be read, the board is hidden, never shown as empty.
    pub fn show(&mut self, ui: &mut Ui, state: &ScreenState, pipeline: &Pipeline, waiting: &Waiting) -> Option<String> {
        let open = pipeline.open_count();
        let waits = pipeline.cards().filter(|card| card_waits(card, waiting)).count();
        let mut subtitle = format!("{open} open, {} closed.", pipeline.cards().count() - open);
        if waits > 0 {
            subtitle.push_str(&format!(" {waits} of them wait for you in Today."));
        }
        let subtitle = if state.failed() { Some("Not shown while the tracker cannot be read.") } else { (!pipeline.is_empty()).then_some(subtitle.as_str()) };
        page_head(ui, Title::Slab("Applications"), subtitle.map(Subtitle::Text), |ui| {
            primary_command_action(ui, "Record a result…", "/outcome");
            command_action(ui, "Check mail for replies…", "/gmail-sync");
        });
        page_body(ui, |ui| {
            (state.alerts)(ui);
            if state.failed() {
                let body = "Jobdesk changed nothing in your folder. The board comes back by itself once the tracker file is fixed, because Jobdesk watches it.";
                empty_note(ui, "Your applications are hidden, not gone", body);
                return None;
            }
            self.body(ui, pipeline, waiting)
        })
    }

    fn body(&mut self, ui: &mut Ui, pipeline: &Pipeline, waiting: &Waiting) -> Option<String> {
        let mut open = None;
        if pipeline.is_empty() {
            empty_state(ui, "No applications yet", "Your tracker appears here once your first application is recorded.", |ui| {
                command_action(ui, "Draft an application…", "/apply");
            });
            return None;
        }
        action_bar(ui, pipeline);
        let selected = self.selected.as_ref().and_then(|(company, role)| pipeline.cards().find(|card| &card.company == company && &card.role == role));
        if let Some(card) = selected {
            side_column("application_details", 320.0).show(ui, |ui| {
                paper_with_footer(ui, |ui| open = details(ui, card), |ui| actions(ui, card));
            });
        }
        if pipeline.cards().any(|card| card_waits(card, waiting)) {
            let frame = Frame::new().inner_margin(egui::Margin { top: NOTE_GAP.1 as i8, ..Default::default() });
            egui::Panel::bottom("applications_legend").show_separator_line(false).frame(frame).show(ui, |ui| {
                ui.horizontal(|ui| {
                    stamp_square(ui, "Pink edge");
                    desk_label(ui, "A pink edge means the application waits for you in Today.", Type::NOTE, Ink::Muted);
                });
            });
        }
        egui::CentralPanel::default().frame(Frame::new()).show(ui, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.board(ui, pipeline, waiting));
        });
        open
    }

    fn board(&mut self, ui: &mut Ui, pipeline: &Pipeline, waiting: &Waiting) {
        let lanes: Vec<&PipelineLane> = pipeline.lanes.iter().filter(|lane| !lane.cards.is_empty() || lane.bucket != StatusBucket::Hired).collect();
        // Every lane takes the height of the tallest, measured on the frame before.
        let tallest_id = ui.scope_id().with("tallest_lane");
        let tallest: f32 = ui.ctx().data(|data| data.get_temp(tallest_id)).unwrap_or(0.0);
        let mut measured = 0.0_f32;
        ui.spacing_mut().item_spacing.x = BOARD_GAP;
        ui.columns(lanes.len(), |columns| {
            for (ui, lane) in columns.iter_mut().zip(lanes) {
                lane_frame(ui, |ui| {
                    ui.set_min_height(tallest);
                    let used = ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            lane_title(ui, lane_name(lane.bucket), bucket_tone(lane.bucket));
                            ui.label(rich(ui, lane.cards.len().to_string(), Type::BODY, Ink::Faint));
                        });
                        for card in &lane.cards {
                            if self.card(ui, card, card_waits(card, waiting)) {
                                self.selected = Some((card.company.clone(), card.role.clone()));
                            }
                        }
                    });
                    measured = measured.max(used.response.rect.height());
                });
            }
        });
        if measured != tallest {
            ui.ctx().data_mut(|data| data.insert_temp(tallest_id, measured));
            ui.ctx().request_repaint();
        }
    }

    fn card(&self, ui: &mut Ui, card: &ApplicationCard, waits: bool) -> bool {
        let on = self.selected.as_ref().is_some_and(|(company, role)| company == &card.company && role == &card.role);
        let meta = card_meta(card);
        let spoken = format!("{}, {}, {meta}", card.company, card.role);
        index_card(ui, CardLook { on, waits, faded: card.is_final }, &spoken, |ui| {
            ui.add(egui::Label::new(rich(ui, card.company.as_str(), Type::STRONG, Ink::Text)).truncate());
            ui.add(egui::Label::new(rich(ui, card.role.as_str(), Type::BODY, Ink::Muted)).truncate());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if let Some(fit) = card.fit_rating {
                        ui.label(rich(ui, format!("fit {fit}"), Type::SMALL, Ink::Muted));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                        let (style, ink) = if waits { (Type::TAG, Ink::Status(Tone::Turn)) } else { (Type::SMALL, Ink::Faint) };
                        ui.add(egui::Label::new(rich(ui, meta, style, ink)).wrap());
                    });
                });
            });
        })
        .clicked()
    }
}

fn applications(count: usize) -> String {
    if count == 1 { "1 application has".into() } else { format!("{count} applications have") }
}

fn action_bar(ui: &mut Ui, pipeline: &Pipeline) {
    let (due, stale) = (pipeline.follow_ups_due(), pipeline.stale_count());
    if due == 0 && stale == 0 {
        return;
    }
    let mut clauses = Vec::new();
    if due > 0 {
        clauses.push(format!("{} had no reply for {FOLLOW_UP_DAYS} days or more. A short follow-up often gets one.", applications(due)));
    }
    if stale > 0 {
        clauses.push(format!("{} been quiet for {STALE_DAYS} days or more and can be closed.", applications(stale)));
    }
    note(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = NOTE_GAP.0;
            ui.label(clauses.join(" "));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if stale > 0 {
                    small_command_action(ui, "Close quiet ones…", "/outcome stale");
                }
                if due > 0 {
                    small_command_action(ui, "Draft follow-ups…", "/outcome followup");
                }
            });
        });
    });
    ui.add_space(SCREEN_GAP);
}

fn details(ui: &mut Ui, card: &ApplicationCard) -> Option<String> {
    let mut open = None;
    panel_body(ui, |ui| {
        ui.label(rich(ui, card.company.as_str(), Type::SECTION, Ink::Text));
        muted(ui, card.role.as_str());
        tag(ui, card.status_label.as_str(), bucket_tone(card.bucket));
        if card.archive_folder.is_some() && primary_button(ui, "Open dossier").clicked() {
            open = card.archive_folder.clone();
        }
    });
    divider(ui);
    panel_body(ui, |ui| {
        fact(ui, "Date", &card.applied_on);
        fact(ui, "Sector", &card.sector);
        fact(ui, "Channel", &card.channel);
        fact(ui, "Contact", &card.contact_person);
        if let Some(deadline) = card.deadline {
            fact(ui, "Deadline", &deadline.to_string());
        }
        if let Some(fit) = card.fit_rating {
            fact(ui, "Fit", &fit.to_string());
        }
        if let Some(days) = card.days_quiet {
            fact(ui, "Days quiet", &days.to_string());
        }
        fact(ui, "Follow-ups", &format!("{} of {MAX_FOLLOW_UPS}", card.follow_ups_sent));
    });
    if !card.notes.trim().is_empty() {
        divider(ui);
        panel_body(ui, |ui| {
            strong(ui, "Notes");
            muted(ui, card.notes.as_str());
        });
    }
    open
}

/// What the user can do with the application, pinned under its details.
fn actions(ui: &mut Ui, card: &ApplicationCard) {
    small_command_action(ui, "Record a result…", &format!("/outcome {}", card.company.to_lowercase()));
    small_command_action(ui, "Prepare for the interview…", &format!("/interview {}", card.company.to_lowercase()));
    open_link(ui, "Open job ad", card.source.as_ref());
}

fn fact(ui: &mut Ui, label: &str, value: &str) {
    if value.trim().is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        muted(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(value));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracking::application::ReachedStage;

    fn applied(days_quiet: i64, days_sent: i64, follow_ups_sent: usize) -> ApplicationCard {
        ApplicationCard { days_followed_up: (follow_ups_sent > 0).then_some(days_quiet), ..plain(days_quiet, days_sent, follow_ups_sent) }
    }

    fn plain(days_quiet: i64, days_sent: i64, follow_ups_sent: usize) -> ApplicationCard {
        ApplicationCard {
            company: "Acme".into(), role: "Lead".into(), sector: String::new(), channel: String::new(),
            status_label: "Applied".into(), bucket: StatusBucket::Active, applied_on: String::new(), deadline: None, deadline_state: None,
            contact_person: String::new(), fit_rating: None, notes: String::new(), source: None, days_quiet: Some(days_quiet), days_sent: Some(days_sent), days_followed_up: None,
            follow_ups_sent, follow_up_due: false, stale: false, reached: ReachedStage::Applied,
            is_rejection: false, is_final: false, cv_file: String::new(), cover_letter_file: String::new(),
            archive_folder: None, outcome: None,
        }
    }

    #[test]
    fn sent_counts_from_the_applied_date() {
        assert_eq!(card_meta(&applied(-9, 5, 0)), "Sent 5 days ago", "a note dated ahead of today");
        assert_eq!(card_meta(&ApplicationCard { days_quiet: Some(2), ..plain(2, 30, 0) }), "Sent 30 days ago", "a dated note that is not a follow-up");
        assert_eq!(card_meta(&applied(1, 1, 0)), "Sent yesterday");
        assert_eq!(card_meta(&applied(12, 30, 1)), "No reply in 12 days, followed up once");
        assert_eq!(card_meta(&applied(15, 15, 0)), "No reply in 15 days");
    }

    #[test]
    fn a_recent_follow_up_shows_in_place_of_the_sent_date() {
        assert_eq!(card_meta(&applied(3, 30, 1)), "Followed up 3 days ago");
        assert_eq!(card_meta(&applied(0, 30, 2)), "Followed up today");
    }

    #[test]
    fn an_offer_reads_its_reply_date_until_it_passes() {
        let offer = ApplicationCard { status_label: "Offer".into(), bucket: StatusBucket::Offer, ..plain(2, 9, 0) };
        let deadline = NaiveDate::from_ymd_opt(2026, 3, 20);
        assert_eq!(card_meta(&ApplicationCard { deadline, deadline_state: Some(DeadlineState::Passed), ..offer.clone() }), "Reply date passed");
        assert_eq!(card_meta(&ApplicationCard { deadline, deadline_state: None, ..offer.clone() }), "Reply by Fri 20 Mar");
        assert_eq!(card_meta(&offer), "Offer", "no deadline");
    }

    #[test]
    fn a_custom_closed_status_keeps_its_label() {
        let on_hold = ApplicationCard { status_label: "On hold".into(), bucket: StatusBucket::Closed, ..plain(12, 30, 0) };
        assert_eq!(card_meta(&on_hold), "On hold");
    }

    #[test]
    fn a_case_only_rename_still_waits() {
        let waiting = Waiting::from([("acme".to_owned(), HashSet::from(["lead".to_owned()]))]);
        assert!(card_waits(&ApplicationCard { company: "ACME".into(), role: "lead".into(), ..plain(1, 1, 0) }, &waiting));
        assert!(card_waits(&plain(1, 1, 0), &waiting));
        assert!(!card_waits(&ApplicationCard { role: "Architect".into(), ..plain(1, 1, 0) }, &waiting));
    }
}
