use egui::{Frame, RichText, ScrollArea, Sense, Stroke, Ui, UiBuilder};

use crate::overview::application::{AttentionKind, Destination, Overview, WorkflowStage};
use crate::ui_kit::{Palette, Tone, command_button, divider, faint_mono, measure, muted, panel, panel_body, panel_header, semibold, tag};

const ATTENTION_ROW_HEIGHT: f32 = 36.0;

pub struct FunnelCounts {
    pub applied: usize,
    pub interview: usize,
    pub offer: usize,
    pub hired: usize,
}

/// Returns the screen the user asked to open, if any.
pub fn show(ui: &mut Ui, overview: &Overview, funnel: &FunnelCounts) -> Option<Destination> {
    let mut destination = None;
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        destination = workflow(ui, &overview.stages);
        ui.add_space(12.0);
        if overview.can_calibrate {
            panel(ui, |ui| {
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(format!("{} resolved outcomes can calibrate your fit scoring.", overview.resolved));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| command_button(ui, "/setup"));
                    });
                });
            });
            ui.add_space(12.0);
        }
        ui.columns_const(|[left, right]| {
            attention(left, overview);
            funnel_panel(right, funnel);
        });
    });
    destination
}

fn workflow(ui: &mut Ui, stages: &[WorkflowStage]) -> Option<Destination> {
    let p = Palette::of(ui);
    let mut destination = None;
    ui.columns(stages.len(), |columns| {
        for (ui, stage) in columns.iter_mut().zip(stages) {
            let clicked = ui
                .scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
                    let hovered = ui.response().hovered();
                    Frame::new()
                        .fill(p.panel)
                        .stroke(Stroke::new(1.0, if hovered { p.faint } else { p.line }))
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(14, 12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 2.0;
                            muted(ui, stage.name);
                            ui.label(RichText::new(stage.count.to_string()).monospace().size(26.0));
                            muted(ui, stage.detail.as_str());
                            faint_mono(ui, stage.command);
                        });
                })
                .response
                .clicked();
            if clicked {
                destination = Some(stage.destination);
            }
        }
    });
    destination
}

fn attention_tag(kind: AttentionKind) -> (&'static str, Tone) {
    match kind {
        AttentionKind::DeadlinePassed => ("Deadline passed", Tone::Closed),
        AttentionKind::ClosingSoon => ("Closing soon", Tone::Closed),
        AttentionKind::NotSent => ("Not sent", Tone::Drafted),
        AttentionKind::FollowUpDue => ("Follow up", Tone::Active),
        AttentionKind::Stale => ("Stale", Tone::Plain),
        AttentionKind::GuardFlag => ("Guard flag", Tone::Interview),
    }
}

fn attention(ui: &mut Ui, overview: &Overview) {
    panel(ui, |ui| {
        panel_header(ui, "Needs attention", Some(&overview.attention.len().to_string()));
        if overview.attention.is_empty() {
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                muted(ui, "Nothing needs you right now.");
            });
        }
        for (index, item) in overview.attention.iter().enumerate() {
            if index > 0 {
                divider(ui);
            }
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                let row = egui::vec2(ui.available_width(), ATTENTION_ROW_HEIGHT);
                ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    command_button(ui, &item.command);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let (text, tone) = attention_tag(item.kind);
                        tag(ui, text, tone);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.add(egui::Label::new(RichText::new(item.title.as_str()).family(semibold())).truncate());
                            ui.add(egui::Label::new(RichText::new(item.why.as_str()).color(Palette::of(ui).muted)).truncate());
                        });
                    });
                });
            });
        }
    });
}

fn funnel_panel(ui: &mut Ui, funnel: &FunnelCounts) {
    let p = Palette::of(ui);
    panel(ui, |ui| {
        panel_header(ui, "Funnel", None);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            measure(ui, "Applied", funnel.applied, funnel.applied, p.active);
            measure(ui, "Interview", funnel.interview, funnel.applied, p.interview);
            measure(ui, "Offer", funnel.offer, funnel.applied, p.offer);
            measure(ui, "Hired", funnel.hired, funnel.applied, p.hired);
        });
    });
}
