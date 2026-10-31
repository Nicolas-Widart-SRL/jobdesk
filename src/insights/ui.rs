use egui::{Align, Frame, Layout, Margin, ScrollArea, Ui, vec2};

use crate::insights::application::{SearchInsights, SkillGap, SkillGapView, WorkflowBand};
use crate::ui_kit::tokens::{SCREEN_GAP, SPLIT_GAP, SPLIT_SIDE_BASIS, TABLE_CELL_PADDING, TABLE_HEAD_PADDING};
use crate::ui_kit::{
    FlowNode, Ink, Subtitle, Title, Tone, Type, command_action, command_button, divider, flow_band, list_row, muted, note, page_body, page_head, panel, panel_body, panel_header,
    primary_command_action, rich, slip, small_command_action, strong, table_head, tag, tally_strip,
};

/// Shares of the table's width: gap, priority, change, source.
const GAP_COLUMNS: [f32; 4] = [0.34, 0.14, 0.12, 0.40];
const ORDER_NUMBER_WIDTH: f32 = 12.0;

pub fn show(ui: &mut Ui, report: &SearchInsights, band: &WorkflowBand, gaps: Option<&SkillGapView>) {
    page_head(ui, Title::Slab("Insights"), Some(Subtitle::Text("The whole workflow, search health and skill gaps, read live from your files")), |ui| {
        primary_command_action(ui, "Find skill gaps", "/upskill");
        command_action(ui, "Rebuild dashboard", "/html-report");
    });
    page_body(ui, |ui| {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            workflow(ui, band);
            ui.add_space(SCREEN_GAP);
            if report.can_calibrate() {
                calibration(ui, report);
                ui.add_space(SCREEN_GAP);
            }
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = SCREEN_GAP;
                ui.columns(4, |columns| {
                    status_chart(&mut columns[0], report);
                    funnel(&mut columns[1], report);
                    counts(&mut columns[2], "By channel", &report.by_channel);
                    counts(&mut columns[3], "By sector", &report.by_sector);
                });
            });
            ui.add_space(SCREEN_GAP);
            skill_gaps(ui, gaps);
        });
    });
}

fn workflow(ui: &mut Ui, band: &WorkflowBand) {
    let text = Some(Ink::Text);
    let nodes = [
        FlowNode {
            label: "Profile",
            figure: format!("{}/{}", band.profile_filled, band.profile_total),
            marks: vec![(band.profile_filled, text), (band.profile_total.saturating_sub(band.profile_filled), None)],
            note: "skill files filled".to_owned(),
        },
        FlowNode {
            label: "Scrape",
            figure: band.new_postings.to_string(),
            marks: vec![(band.new_postings, text), (band.seen.saturating_sub(band.new_postings), None)],
            note: format!("new of {} seen", band.seen),
        },
        FlowNode { label: "Rank", figure: band.shortlisted.to_string(), marks: vec![(band.shortlisted, text)], note: "on shortlist".to_owned() },
        FlowNode { label: "Apply", figure: band.drafted.to_string(), marks: vec![(band.drafted, Some(Tone::Drafted.into()))], note: "drafted, not sent".to_owned() },
        FlowNode {
            label: "In process",
            figure: (band.active + band.interview + band.offer).to_string(),
            marks: vec![(band.active, Some(Tone::Active.into())), (band.interview, Some(Tone::Interview.into())), (band.offer, Some(Tone::Offer.into()))],
            note: format!("{} applied, {} interview, {} offer", band.active, band.interview, band.offer),
        },
        FlowNode { label: "Outcome", figure: band.closed.to_string(), marks: vec![(band.closed, Some(Tone::Closed.into()))], note: "closed".to_owned() },
    ];
    flow_band(ui, &nodes);
}

fn calibration(ui: &mut Ui, report: &SearchInsights) {
    note(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("{} resolved outcomes can calibrate your fit scoring.", report.resolved));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| small_command_action(ui, "Calibrate", "/setup"));
        });
    });
}

/// A label with its count on the right.
fn counted_row(ui: &mut Ui, label: impl FnOnce(&mut Ui), count: usize) {
    ui.horizontal(|ui| {
        label(ui);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(count.to_string()));
    });
}

/// A count over a fat tally of one mark per item.
fn tallied(ui: &mut Ui, label: &str, count: usize, ink: Ink) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        counted_row(ui, |ui| { ui.label(label); }, count);
        tally_strip(ui, &[(count, Some(ink))], true);
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
            let summary = rows.iter().map(|(label, value, _)| format!("{value} {label}")).collect::<Vec<_>>().join(", ");
            let segments: Vec<_> = rows.iter().map(|(_, value, tone)| (*value, Some(Ink::from(*tone)))).collect();
            tally_strip(ui, &segments, true).on_hover_text(summary);
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            for (label, value, tone) in rows {
                counted_row(ui, |ui| { tag(ui, label, tone); }, value);
            }
        });
    });
}

fn funnel(ui: &mut Ui, report: &SearchInsights) {
    let note = report.rejection_rate.map(|rate| format!("rejection rate {:.0}%", rate * 100.0));
    panel(ui, |ui| {
        panel_header(ui, "Funnel", note.as_deref());
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            tallied(ui, "Applied", report.sent, Tone::Active.into());
            tallied(ui, "Interview", report.reached_interview, Tone::Interview.into());
            tallied(ui, "Offer", report.reached_offer, Tone::Offer.into());
            tallied(ui, "Hired", report.reached_hired, Tone::Hired.into());
        });
    });
}

fn counts(ui: &mut Ui, title: &str, rows: &[(String, usize)]) {
    panel(ui, |ui| {
        panel_header(ui, title, None);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            if rows.is_empty() {
                muted(ui, "No applications sent yet.");
            }
            for (label, count) in rows.iter().take(5) {
                tallied(ui, label, *count, Ink::Text);
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
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = SPLIT_GAP;
        let main = (ui.available_width() - SPLIT_GAP - SPLIT_SIDE_BASIS).max(0.0);
        ui.allocate_ui_with_layout(vec2(main, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(main);
            panel(ui, |ui| {
                panel_header(ui, "Skill gaps", Some(&report.file_name));
                gap_table(ui, report);
            });
        });
        ui.allocate_ui_with_layout(vec2(SPLIT_SIDE_BASIS, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(SPLIT_SIDE_BASIS);
            study_order(ui, report);
        });
    });
}

fn gap_table(ui: &mut Ui, report: &SkillGapView) {
    let width = ui.available_width();
    let widths = GAP_COLUMNS.map(|share| share * width);
    ui.spacing_mut().item_spacing.y = 0.0;
    table_row(ui, &widths, TABLE_HEAD_PADDING, |ui, column| {
        table_head(ui, ["Gap", "Priority", "Change", "Source"][column]);
    });
    for gap in &report.gaps {
        divider(ui);
        let is_new = report.new_gaps.iter().any(|name| same_skill(name, &gap.skill));
        table_row(ui, &widths, TABLE_CELL_PADDING, |ui, column| gap_cell(ui, gap, is_new, column));
    }
}

fn gap_cell(ui: &mut Ui, gap: &SkillGap, is_new: bool, column: usize) {
    match column {
        0 => {
            strong(ui, gap.skill.as_str());
        }
        1 => {
            tag(ui, gap.priority.as_str(), priority_tone(&gap.priority));
        }
        2 if is_new => {
            muted(ui, "New");
        }
        2 => {}
        _ => {
            muted(ui, format!("{} · {}", gap.kind, gap.source));
        }
    }
}

fn table_row(ui: &mut Ui, widths: &[f32], (x, y): (f32, f32), mut cell: impl FnMut(&mut Ui, usize)) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (column, width) in widths.iter().enumerate() {
            ui.allocate_ui_with_layout(vec2(*width, 0.0), Layout::top_down(Align::Min), |ui| {
                Frame::new().inner_margin(Margin::symmetric(x as i8, y as i8)).show(ui, |ui| {
                    ui.set_width(width - 2.0 * x);
                    cell(ui, column);
                });
            });
        }
    });
}

fn study_order(ui: &mut Ui, report: &SkillGapView) {
    slip(ui, |ui| {
        panel_header(ui, "Study order", report.total_time.as_deref());
        for (index, item) in report.study_order.iter().enumerate() {
            list_row(ui, index == 0, |ui| {
                ui.horizontal(|ui| {
                    let number = rich(ui, (index + 1).to_string(), Type::BODY, Ink::Faint);
                    let width = ui.label(number).rect.width();
                    ui.add_space((ORDER_NUMBER_WIDTH - width).max(0.0));
                    ui.label(item.topic.as_str());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| muted(ui, item.time.as_str()));
                });
            });
        }
        for (index, gap) in report.gaps_closed.iter().enumerate() {
            list_row(ui, index == 0 && report.study_order.is_empty(), |ui| {
                ui.horizontal(|ui| {
                    ui.label(gap.as_str());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| muted(ui, "Gap closed"));
                });
            });
        }
    });
}

fn same_skill(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn priority_tone(priority: &str) -> Tone {
    match priority.to_lowercase().as_str() {
        "critical" => Tone::Closed,
        "high" => Tone::Interview,
        "medium" => Tone::Active,
        _ => Tone::Plain,
    }
}
