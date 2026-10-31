use egui::{RichText, ScrollArea, Ui};

use crate::insights::application::{SearchInsights, SkillGapView};
use crate::ui_kit::{Palette, Tone, command_button, doughnut, measure, muted, panel, panel_body, panel_header, semibold, strong, tag};

pub fn show(ui: &mut Ui, report: &SearchInsights, gaps: Option<&SkillGapView>) {
    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        stats(ui, report);
        ui.add_space(12.0);
        ui.columns(3, |columns| {
            status_chart(&mut columns[0], report);
            funnel(&mut columns[1], report);
            breakdown(&mut columns[2], report);
        });
        ui.add_space(12.0);
        skill_gaps(ui, gaps);
    });
}

fn stats(ui: &mut Ui, report: &SearchInsights) {
    let p = Palette::of(ui);
    let tally = &report.tally;
    let cards = [
        ("Sent", report.sent, p.text),
        ("Drafted", tally.drafted, p.drafted),
        ("Active", tally.active, p.active),
        ("Interview", tally.interview, p.interview),
        ("Offer", tally.offer, p.offer),
        ("Closed", tally.closed, p.closed),
    ];
    ui.columns(cards.len(), |columns| {
        for (ui, (label, value, color)) in columns.iter_mut().zip(cards) {
            panel(ui, |ui| {
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(value.to_string()).monospace().size(24.0).color(color));
                    muted(ui, label);
                });
            });
        }
    });
}

fn status_chart(ui: &mut Ui, report: &SearchInsights) {
    let p = Palette::of(ui);
    let tally = &report.tally;
    let rows = [
        ("Active", tally.active, p.active, Tone::Active),
        ("Interview", tally.interview, p.interview, Tone::Interview),
        ("Offer", tally.offer, p.offer, Tone::Offer),
        ("Hired", tally.hired, p.hired, Tone::Hired),
        ("Closed", tally.closed, p.closed, Tone::Closed),
    ];
    panel(ui, |ui| {
        panel_header(ui, "Status", Some(&format!("{} sent", report.sent)));
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let summary = rows.iter().map(|(label, value, ..)| format!("{value} {label}")).collect::<Vec<_>>().join(", ");
                let segments: Vec<_> = rows.iter().map(|(_, value, color, _)| (*value, *color)).collect();
                doughnut(ui, 120.0, &segments).on_hover_text(summary);
                ui.vertical(|ui| {
                    for (label, value, _, tone) in rows {
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
    let p = Palette::of(ui);
    let note = report.past_screen_rate().map(|rate| format!("{:.0}% past screen", rate * 100.0));
    panel(ui, |ui| {
        panel_header(ui, "Funnel", note.as_deref());
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            measure(ui, "Applied", report.sent, report.sent, p.active);
            measure(ui, "Interview", report.reached_interview, report.sent, p.interview);
            measure(ui, "Offer", report.reached_offer, report.sent, p.offer);
            measure(ui, "Hired", report.reached_hired, report.sent, p.hired);
            if let Some(rate) = report.rejection_rate {
                muted(ui, format!("Rejection rate {:.0}% of resolved", rate * 100.0));
            }
        });
    });
}

fn breakdown(ui: &mut Ui, report: &SearchInsights) {
    let p = Palette::of(ui);
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
                    measure(ui, label, *count, report.sent, p.text);
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
                        ui.label(RichText::new(gap.skill.as_str()).family(semibold()));
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
