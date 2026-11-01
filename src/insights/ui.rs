use egui::{Align, Layout, ScrollArea, Ui};
use egui_extras::{Column, TableBuilder};

use crate::insights::application::{GapRow, SearchInsights, SkillGapView, WorkflowBand, gap_rows};
use crate::shared_kernel::Source;
use crate::ui_kit::tokens::{SCREEN_GAP, SKELETON_GAP, SKELETON_LINE, SPLIT_SIDE_BASIS, TABLE_CELL_PADDING, TABLE_HEAD_HEIGHT, TAG_HEIGHT};
use crate::ui_kit::{
    CellRule, FlowNode, Ink, ScreenState, Subtitle, Title, Tone, Type, command_action, empty_note, empty_state, flow_band, hidden_note, list_row, muted, note, page_body, page_head,
    panel, panel_body, panel_header, primary_command_action, rich, skeleton, skeleton_lines, slip, small_command_action, split, table_cell, table_head, tag, tally_strip, wrapped_height,
};

/// Shares of the table's width: gap, priority, change, source.
const GAP_COLUMNS: [f32; 4] = [0.34, 0.14, 0.12, 0.40];
const ORDER_NUMBER_WIDTH: f32 = 12.0;
/// What a part fed by the tracker says while the tracker cannot be read. An open dossier says it too.
pub const TRACKER_HIDDEN: &str = "Hidden while the tracker cannot be read.";
const JOBS_HIDDEN: &str = "Hidden while your saved jobs cannot be read.";
/// The band's stages, in order.
const STAGES: [&str; 6] = ["Profile", "Find", "Score", "Apply", "In progress", "Result"];
const PANELS: [&str; 4] = ["Status", "Funnel", "By channel", "By sector"];

/// A failed source loads as nothing, so the parts it feeds say they are hidden instead of showing zeros.
/// Until the first read, the band and the panels show their names over grey lines.
pub fn show(ui: &mut Ui, state: &ScreenState, report: &SearchInsights, band: &WorkflowBand, gaps: Option<&SkillGapView>) {
    let tracker = state.cannot_read(Source::Applications).then_some(TRACKER_HIDDEN);
    // With no report to show, the skill gap section offers this action itself, so the head does not repeat it.
    // The loading view offers nothing below, so the head keeps it while reading.
    let offered_below = !state.reading() && gaps.is_none() && !state.cannot_read(Source::SkillGaps);
    let subtitle = if state.reading() { "Reading your jobs, applications and reports…" } else { "The whole workflow, search health and skill gaps, read live from your files" };
    page_head(ui, Title::Slab("Insights"), Some(Subtitle::Text(subtitle)), |ui| {
        if !offered_below {
            primary_command_action(ui, "Find skill gaps", "/upskill");
        }
        command_action(ui, "Rebuild dashboard", "/html-report");
    });
    page_body(ui, |ui| {
        (state.alerts)(ui);
        if state.reading() {
            loading(ui);
            return;
        }
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            workflow(ui, band, tracker, state.cannot_read(Source::Jobs).then_some(JOBS_HIDDEN));
            ui.add_space(SCREEN_GAP);
            // A failed tracker loads with no outcomes, so it never calibrates.
            if report.can_calibrate() {
                calibration(ui, report);
                ui.add_space(SCREEN_GAP);
            }
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = SCREEN_GAP;
                ui.columns(4, |columns| {
                    status_chart(&mut columns[0], report, tracker);
                    funnel(&mut columns[1], report, tracker);
                    counts(&mut columns[2], "By channel", &report.by_channel, tracker);
                    counts(&mut columns[3], "By sector", &report.by_sector, tracker);
                });
            });
            ui.add_space(SCREEN_GAP);
            if state.cannot_read(Source::SkillGaps) {
                empty_note(ui, "Skill gap report", "Your skill gap reports are hidden, not gone. They come back by themselves once the report file is fixed.");
            } else {
                skill_gaps(ui, gaps);
            }
        });
    });
}

/// The screen before the first read: the band's stages and the panels by name, over grey lines where their counts go.
fn loading(ui: &mut Ui) {
    slip(ui, |ui| {
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.columns(STAGES.len(), |columns| {
                for (ui, stage) in columns.iter_mut().zip(STAGES) {
                    ui.spacing_mut().item_spacing.y = SKELETON_GAP;
                    ui.label(rich(ui, stage, Type::TAB, Ink::Muted));
                    skeleton(ui, 36.0, 28.0);
                    skeleton(ui, 110.0, SKELETON_LINE);
                }
            });
        });
    });
    ui.add_space(SCREEN_GAP);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = SCREEN_GAP;
        ui.columns(PANELS.len(), |columns| {
            for (ui, title) in columns.iter_mut().zip(PANELS) {
                panel(ui, |ui| {
                    panel_header(ui, title, None);
                    panel_body(ui, |ui| {
                        ui.set_width(ui.available_width());
                        skeleton_lines(ui, &[160.0, 120.0, 140.0, 100.0], skeleton);
                    });
                });
            }
        });
    });
    ui.add_space(SCREEN_GAP);
    panel(ui, |ui| {
        panel_header(ui, "Skill gaps", None);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            skeleton_lines(ui, &[420.0, 360.0, 390.0], skeleton);
        });
    });
}

/// `tracker` and `jobs` say why the stages they feed are hidden, if they are.
fn workflow(ui: &mut Ui, band: &WorkflowBand, tracker: Option<&str>, jobs: Option<&str>) {
    let text = Some(Ink::Text);
    let nodes = [
        FlowNode {
            label: "Profile",
            figure: format!("{}/{}", band.profile_filled, band.profile_total),
            marks: vec![(band.profile_filled, text), (band.profile_total.saturating_sub(band.profile_filled), None)],
            note: format!("{} of {} profile parts filled", band.profile_filled, band.profile_total),
            hidden: None,
        },
        FlowNode {
            label: "Find",
            figure: band.new_postings.to_string(),
            marks: vec![(band.new_postings, text), (band.seen.saturating_sub(band.new_postings), None)],
            note: format!("{} new of {} found", band.new_postings, band.seen),
            hidden: jobs,
        },
        FlowNode { label: "Score", figure: band.shortlisted.to_string(), marks: vec![(band.shortlisted, text)], note: "on shortlist".to_owned(), hidden: jobs },
        FlowNode {
            label: "Apply",
            figure: band.drafted.to_string(),
            marks: vec![(band.drafted, Some(Tone::Drafted.into()))],
            note: "drafted, not sent".to_owned(),
            hidden: tracker,
        },
        FlowNode {
            label: "In progress",
            figure: (band.active + band.interview + band.offer).to_string(),
            marks: vec![(band.active, Some(Tone::Active.into())), (band.interview, Some(Tone::Interview.into())), (band.offer, Some(Tone::Offer.into()))],
            note: format!("{} applied, {} interview, {} offer", band.active, band.interview, band.offer),
            hidden: tracker,
        },
        FlowNode {
            label: "Result",
            figure: (band.hired + band.closed).to_string(),
            marks: vec![(band.hired, Some(Tone::Hired.into())), (band.closed, Some(Tone::Closed.into()))],
            note: format!("{} hired, {} closed", band.hired, band.closed),
            hidden: tracker,
        },
    ];
    flow_band(ui, &nodes);
}

fn calibration(ui: &mut Ui, report: &SearchInsights) {
    note(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("{} resolved outcomes can calibrate your fit scoring.", report.resolved));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| small_command_action(ui, "Tune scoring", "/setup"));
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

/// A panel fed by a source that could not be read: its title, then why it is hidden, and no count.
fn hidden_panel(ui: &mut Ui, title: &str, why: &str) {
    panel(ui, |ui| {
        panel_header(ui, title, None);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            hidden_note(ui, why);
        });
    });
}

fn status_chart(ui: &mut Ui, report: &SearchInsights, hidden: Option<&str>) {
    if let Some(why) = hidden {
        hidden_panel(ui, "Status", why);
        return;
    }
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

fn funnel(ui: &mut Ui, report: &SearchInsights, hidden: Option<&str>) {
    if let Some(why) = hidden {
        hidden_panel(ui, "Funnel", why);
        return;
    }
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
            muted(ui, "Applications not sent yet are not counted.");
        });
    });
}

fn counts(ui: &mut Ui, title: &str, rows: &[(String, usize)], hidden: Option<&str>) {
    if let Some(why) = hidden {
        hidden_panel(ui, title, why);
        return;
    }
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
        empty_state(ui, "No skill gap report yet", "Skill gaps are drawn from your ranked jobs and tracked applications. Run the analysis once you have some.", |ui| {
            command_action(ui, "Find skill gaps", "/upskill");
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
    split(ui, SPLIT_SIDE_BASIS, |main, side| {
        panel(main, |ui| {
            panel_header(ui, "Skill gaps", Some(&report.file_name));
            gap_table(ui, &gap_rows(report));
        });
        study_order(side, report);
    });
}

fn gap_table(ui: &mut Ui, rows: &[GapRow]) {
    let total = ui.available_width();
    let widths = GAP_COLUMNS.map(|share| (total * share).floor());
    let source_width = total - widths[0] - widths[1] - widths[2];
    // Gap and source wrap, so each row is as tall as its taller one.
    let room = |width: f32| width - TABLE_CELL_PADDING.0 * 2.0;
    let heights: Vec<f32> = rows
        .iter()
        .map(|row| {
            let tallest = [(&row.skill, Type::STRONG, widths[0]), (&row.source, Type::BODY, source_width)]
                .into_iter()
                .map(|(text, style, width)| wrapped_height(ui, text, style, room(width)))
                .fold(TAG_HEIGHT, f32::max);
            tallest + TABLE_CELL_PADDING.1 * 2.0
        })
        .collect();
    let last = rows.len().saturating_sub(1);
    // Cells pad themselves, so the rules run unbroken.
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    TableBuilder::new(ui)
        .id_salt("insights_skill_gaps")
        .vscroll(false)
        .striped(false)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(widths[0]).clip(true))
        .column(Column::exact(widths[1]).clip(true))
        .column(Column::exact(widths[2]).clip(true))
        .column(Column::remainder().clip(true))
        .header(TABLE_HEAD_HEIGHT, |mut header| {
            for title in ["Gap", "Priority", "Change", "Source"] {
                header.col(|ui| {
                    table_cell(ui, CellRule::Head, |ui| table_head(ui, title));
                });
            }
        })
        .body(|body| {
            body.heterogeneous_rows(heights.into_iter(), |mut line| {
                let index = line.index();
                let row = &rows[index];
                let rule = if index == last { CellRule::Last } else { CellRule::Row };
                line.col(|ui| {
                    table_cell(ui, rule, |ui| ui.add(egui::Label::new(rich(ui, row.skill.as_str(), Type::STRONG, Ink::Text)).wrap()));
                });
                line.col(|ui| {
                    table_cell(ui, rule, |ui| {
                        if !row.priority.is_empty() {
                            tag(ui, row.priority.as_str(), priority_tone(&row.priority));
                        }
                    });
                });
                line.col(|ui| {
                    table_cell(ui, rule, |ui| {
                        if row.is_new {
                            muted(ui, "New");
                        }
                    });
                });
                line.col(|ui| {
                    table_cell(ui, rule, |ui| ui.add(egui::Label::new(rich(ui, row.source.as_str(), Type::BODY, Ink::Muted)).wrap()));
                });
            });
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

fn priority_tone(priority: &str) -> Tone {
    match priority.to_lowercase().as_str() {
        "critical" => Tone::Closed,
        "high" => Tone::Interview,
        "medium" => Tone::Active,
        _ => Tone::Plain,
    }
}
