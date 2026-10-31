use egui::{ScrollArea, Ui};

use crate::insights::application::{SearchInsights, SkillGapView};
use crate::ui_kit::{Ink, Tone, command_button, doughnut, figure, measure, muted, note, panel, panel_body, panel_header, slip, strong, tag};

pub fn show(ui: &mut Ui, report: &SearchInsights, gaps: Option<&SkillGapView>) {
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        stats(ui, report);
        ui.add_space(12.0);
        if report.can_calibrate() {
            calibration(ui, report);
            ui.add_space(12.0);
        }
        ui.columns(3, |columns| {
            status_chart(&mut columns[0], report);
            funnel(&mut columns[1], report);
            breakdown(&mut columns[2], report);
        });
        ui.add_space(12.0);
        skill_gaps(ui, gaps);
    });
}

fn calibration(ui: &mut Ui, report: &SearchInsights) {
    note(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("{} resolved outcomes can calibrate your fit scoring.", report.resolved));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| command_button(ui, "/setup"));
        });
    });
}

fn stats(ui: &mut Ui, report: &SearchInsights) {
    let tally = &report.tally;
    let cards = [
        ("Sent", report.sent, Ink::Text),
        ("Drafted", tally.drafted, Tone::Drafted.into()),
        ("Active", tally.active, Tone::Active.into()),
        ("Interview", tally.interview, Tone::Interview.into()),
        ("Offer", tally.offer, Tone::Offer.into()),
        ("Closed", tally.closed, Tone::Closed.into()),
    ];
    ui.columns(cards.len(), |columns| {
        for (ui, (label, value, ink)) in columns.iter_mut().zip(cards) {
            slip(ui, |ui| {
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    figure(ui, value.to_string(), ink);
                    muted(ui, label);
                });
            });
        }
    });
}

fn status_chart(ui: &mut Ui, report: &SearchInsights) {
    let tally = &report.tally;
    let rows = [
        ("Active", tally.active, Tone::Active),
        ("Interview", tally.interview, Tone::Interview),
        ("Offer", tally.offer, Tone::Offer),
        ("Hired", tally.hired, Tone::Hired),
        ("Closed", tally.closed, Tone::Closed),
    ];
    panel(ui, |ui| {
        panel_header(ui, "Status", Some(&format!("{} sent", report.sent)));
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let summary = rows.iter().map(|(label, value, _)| format!("{value} {label}")).collect::<Vec<_>>().join(", ");
                let segments: Vec<_> = rows.iter().map(|(_, value, tone)| (*value, *tone)).collect();
                doughnut(ui, 120.0, &segments).on_hover_text(summary);
                ui.vertical(|ui| {
                    for (label, value, tone) in rows {
                        ui.horizontal(|ui| {
                            tag(ui, label, tone);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.monospace(value.to_string()));
                        });
                    }
                });
            });
        });
    });
}

fn funnel(ui: &mut Ui, report: &SearchInsights) {
    let note = report.past_screen_rate().map(|rate| format!("{:.0}% past screen", rate * 100.0));
    panel(ui, |ui| {
        panel_header(ui, "Funnel", note.as_deref());
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            measure(ui, "Applied", report.sent, report.sent, Tone::Active);
            measure(ui, "Interview", report.reached_interview, report.sent, Tone::Interview);
            measure(ui, "Offer", report.reached_offer, report.sent, Tone::Offer);
            measure(ui, "Hired", report.reached_hired, report.sent, Tone::Hired);
            if let Some(rate) = report.rejection_rate {
                muted(ui, format!("Rejection rate {:.0}% of resolved", rate * 100.0));
            }
        });
    });
}

fn breakdown(ui: &mut Ui, report: &SearchInsights) {
    panel(ui, |ui| {
        panel_header(ui, "By sector and channel", None);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            if report.sent == 0 {
                muted(ui, "No applications sent yet.");
            }
            for (heading, rows, limit) in [("Sector", &report.by_sector, 4), ("Channel", &report.by_channel, 3)] {
                if !rows.is_empty() {
                    muted(ui, heading);
                }
                for (label, count) in rows.iter().take(limit) {
                    measure(ui, label, *count, report.sent, Ink::Text);
                }
            }
        });
    });
}

fn skill_gaps(ui: &mut Ui, report: Option<&SkillGapView>) {
    let Some(report) = report else {
        panel(ui, |ui| {
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                strong(ui, "No skill gap report yet");
                muted(ui, "Run the analysis once you have ranked jobs or tracked applications.");
                command_button(ui, "/upskill");
            });
        });
        return;
    };
    if report.gaps.is_empty() {
        panel(ui, |ui| {
            panel_header(ui, "Skill gap report", Some(&report.file_name));
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.monospace(report.raw.as_str());
            });
        });
        return;
    }
    ui.columns_const(|[left, right]| {
        panel(left, |ui| {
            panel_header(ui, "Skill gaps", Some(&report.file_name));
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Grid::new("skill_gaps").num_columns(3).spacing([16.0, 6.0]).striped(false).show(ui, |ui| {
                    for gap in &report.gaps {
                        tag(ui, gap.priority.as_str(), priority_tone(&gap.priority));
                        strong(ui, gap.skill.as_str());
                        muted(ui, format!("{} · {}", gap.kind, gap.source));
                        ui.end_row();
                    }
                });
            });
        });
        panel(right, |ui| {
            panel_header(ui, "Study order", report.total_time.as_deref());
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                for (index, item) in report.study_order.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.monospace((index + 1).to_string());
                        ui.label(item.topic.as_str());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| muted(ui, item.time.as_str()));
                    });
                }
                for gap in &report.new_gaps {
                    muted(ui, format!("New: {gap}"));
                }
                for gap in &report.gaps_closed {
                    muted(ui, format!("Closed: {gap}"));
                }
            });
        });
    });
}

fn priority_tone(priority: &str) -> Tone {
    match priority.to_lowercase().as_str() {
        "critical" => Tone::Closed,
        "high" => Tone::Interview,
        "medium" => Tone::Active,
        _ => Tone::Plain,
    }
}
