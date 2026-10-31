use egui::{Frame, Panel, RichText, ScrollArea, Sense, Stroke, Ui, UiBuilder};

use crate::tracking::application::{ApplicationCard, DeadlineState, FOLLOW_UP_DAYS, MAX_FOLLOW_UPS, Pipeline, PipelineLane, STALE_DAYS, StatusBucket};
use crate::ui_kit::{Palette, Tone, command_button, divider, muted, open_link, panel, panel_body, strong, tag};

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
    pub fn show(&mut self, ui: &mut Ui, pipeline: &Pipeline) {
        if pipeline.is_empty() {
            panel(ui, |ui| {
                panel_body(ui, |ui| {
                    strong(ui, "No applications yet");
                    muted(ui, "The tracker appears when /apply records your first application.");
                    command_button(ui, "/apply");
                });
            });
            return;
        }
        action_bar(ui, pipeline);
        let selected = self.selected.as_ref().and_then(|(company, role)| pipeline.cards().find(|card| &card.company == company && &card.role == role));
        if let Some(card) = selected {
            Panel::right("application_details").exact_size(320.0).resizable(false).frame(Frame::new().inner_margin(egui::Margin { left: 12, ..Default::default() })).show(ui, |ui| {
                panel(ui, |ui| ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| details(ui, card)));
            });
        }
        egui::CentralPanel::default().frame(Frame::new()).show(ui, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.board(ui, pipeline));
        });
    }

    fn board(&mut self, ui: &mut Ui, pipeline: &Pipeline) {
        let lanes: Vec<&PipelineLane> = pipeline.lanes.iter().filter(|lane| !lane.cards.is_empty() || lane.bucket != StatusBucket::Hired).collect();
        ui.columns(lanes.len(), |columns| {
            for (ui, lane) in columns.iter_mut().zip(lanes) {
                ui.horizontal(|ui| {
                    tag(ui, lane.bucket.label(), bucket_tone(lane.bucket));
                    ui.monospace(lane.cards.len().to_string());
                });
                for card in &lane.cards {
                    if self.card(ui, card) {
                        self.selected = Some((card.company.clone(), card.role.clone()));
                    }
                }
            }
        });
    }

    fn card(&self, ui: &mut Ui, card: &ApplicationCard) -> bool {
        let p = Palette::of(ui);
        let on = self.selected.as_ref().is_some_and(|(company, role)| company == &card.company && role == &card.role);
        ui.scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
            let hovered = ui.response().hovered();
            Frame::new()
                .fill(p.panel)
                .stroke(Stroke::new(1.0, if on || hovered { p.faint } else { p.line }))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(12, 9))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.add(egui::Label::new(RichText::new(card.company.as_str()).family(crate::ui_kit::semibold())).truncate());
                    ui.add(egui::Label::new(RichText::new(card.role.as_str()).color(p.muted)).truncate());
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(card_meta(card)).small().color(p.faint)).truncate());
                        if let Some(fit) = card.fit_rating {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(fit.to_string()).monospace().color(p.faint)));
                        }
                    });
                });
        })
        .response
        .clicked()
    }
}

fn action_bar(ui: &mut Ui, pipeline: &Pipeline) {
    let (due, stale) = (pipeline.follow_ups_due(), pipeline.stale_count());
    if due == 0 && stale == 0 {
        return;
    }
    panel(ui, |ui| {
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(format!("{due} quiet {FOLLOW_UP_DAYS}+ days can get a follow-up. {stale} quiet {STALE_DAYS}+ days ready to resolve."));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    command_button(ui, "/outcome stale");
                    command_button(ui, "/outcome followup");
                });
            });
        });
    });
    ui.add_space(10.0);
}

fn details(ui: &mut Ui, card: &ApplicationCard) {
    panel_body(ui, |ui| {
        ui.label(RichText::new(card.company.as_str()).size(15.0).family(crate::ui_kit::semibold()));
        muted(ui, card.role.as_str());
        tag(ui, card.status_label.as_str(), bucket_tone(card.bucket));
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
