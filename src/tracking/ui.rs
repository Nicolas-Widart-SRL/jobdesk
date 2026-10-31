use egui::{Frame, ScrollArea, Ui};

use crate::tracking::application::{ApplicationCard, DeadlineState, FOLLOW_UP_DAYS, MAX_FOLLOW_UPS, Pipeline, PipelineLane, STALE_DAYS, StatusBucket};
use crate::ui_kit::{Ink, Tone, Type, command_button, divider, index_card, lane as lane_frame, lane_title, muted, note, open_link, panel, panel_body, primary_button, rich, side_column, slip, strong, tag};

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

fn card_meta(card: &ApplicationCard) -> String {
    match (card.deadline_state, card.days_quiet) {
        (Some(DeadlineState::Passed), _) => "Deadline passed".into(),
        (Some(DeadlineState::ClosingIn(days)), _) => format!("Deadline in {days}d"),
        (_, Some(days)) if !card.is_final => format!("{days}d quiet, {} of {MAX_FOLLOW_UPS} follow-ups", card.follow_ups_sent),
        _ => card.status_label.clone(),
    }
}

impl TrackerScreen {
    /// Returns the archive folder of a dossier the user asked to open.
    pub fn show(&mut self, ui: &mut Ui, pipeline: &Pipeline) -> Option<String> {
        let mut open = None;
        if pipeline.is_empty() {
            panel(ui, |ui| {
                panel_body(ui, |ui| {
                    strong(ui, "No applications yet");
                    muted(ui, "The tracker appears when /apply records your first application.");
                    command_button(ui, "/apply");
                });
            });
            return None;
        }
        action_bar(ui, pipeline);
        let selected = self.selected.as_ref().and_then(|(company, role)| pipeline.cards().find(|card| &card.company == company && &card.role == role));
        if let Some(card) = selected {
            side_column("application_details", 320.0).show(ui, |ui| {
                slip(ui, |ui| ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| open = details(ui, card)));
            });
        }
        egui::CentralPanel::default().frame(Frame::new()).show(ui, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.board(ui, pipeline));
        });
        open
    }

    fn board(&mut self, ui: &mut Ui, pipeline: &Pipeline) {
        let lanes: Vec<&PipelineLane> = pipeline.lanes.iter().filter(|lane| !lane.cards.is_empty() || lane.bucket != StatusBucket::Hired).collect();
        ui.columns(lanes.len(), |columns| {
            for (ui, lane) in columns.iter_mut().zip(lanes) {
                lane_frame(ui, |ui| {
                    ui.horizontal(|ui| {
                        lane_title(ui, lane.bucket.label(), bucket_tone(lane.bucket));
                        ui.monospace(lane.cards.len().to_string());
                    });
                    for card in &lane.cards {
                        if self.card(ui, card) {
                            self.selected = Some((card.company.clone(), card.role.clone()));
                        }
                    }
                });
            }
        });
    }

    fn card(&self, ui: &mut Ui, card: &ApplicationCard) -> bool {
        let on = self.selected.as_ref().is_some_and(|(company, role)| company == &card.company && role == &card.role);
        index_card(ui, on, |ui| {
            ui.add(egui::Label::new(rich(ui, card.company.as_str(), Type::STRONG, Ink::Text)).truncate());
            ui.add(egui::Label::new(rich(ui, card.role.as_str(), Type::BODY, Ink::Muted)).truncate());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(fit) = card.fit_rating {
                        ui.label(rich(ui, fit.to_string(), Type::FIT, Ink::Text));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(rich(ui, card_meta(card), Type::SMALL, Ink::Faint)).truncate());
                    });
                });
            });
        })
        .clicked()
    }
}

fn action_bar(ui: &mut Ui, pipeline: &Pipeline) {
    let (due, stale) = (pipeline.follow_ups_due(), pipeline.stale_count());
    if due == 0 && stale == 0 {
        return;
    }
    note(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("{due} quiet {FOLLOW_UP_DAYS}+ days can get a follow-up. {stale} quiet {STALE_DAYS}+ days ready to resolve."));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                command_button(ui, "/outcome stale");
                command_button(ui, "/outcome followup");
            });
        });
    });
    ui.add_space(10.0);
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
    divider(ui);
    panel_body(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            command_button(ui, &format!("/outcome {}", card.company.to_lowercase()));
            command_button(ui, &format!("/interview {}", card.company.to_lowercase()));
            open_link(ui, "Open posting", card.source.as_ref());
        });
    });
    open
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
