use egui::{Align, Frame, Layout, Margin, ScrollArea, Ui, vec2};
use egui_extras::{Column, TableBuilder};

use crate::ui_kit::tokens::{INSET, ITEM_GAP, ROW_ITEM_GAP, SMALL_BUTTON_HEIGHT, SPLIT_GAP, SPLIT_SIDE_BASIS, TABLE_CELL_PADDING, TABLE_HEAD_HEIGHT, TAG_HEIGHT};
use crate::ui_kit::{
    CellRule, Ink, Subtitle, Title, Tone, Type, button, danger_command_button, divider, enabled_when, faint_mono, list_row, muted, page_body, page_head, panel, panel_body, panel_header, panel_header_tone,
    rich, slip, small_button, small_command_action, split, table_cell, table_head, tag,
};
use crate::workspace::application::{EnvironmentChecks, PortalRow, ProfileState, ToolCheck, WorkspaceView};

/// Environment rows sit closer than other list rows.
const TOOL_ROW_PADDING: i8 = 6;
/// Shares of the portal table's width: portal, jobs seen, state.
const PORTAL_COLUMNS: [f32; 3] = [0.5, 0.3, 0.2];

/// A profile row holds the plain title over the file name.
const PROFILE_ROW_HEIGHT: f32 = 38.0;

/// A row of fixed height, so its text centres on the tags and buttons beside it.
fn line(ui: &mut Ui, height: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
        add(ui);
    });
}

fn row(ui: &mut Ui, first: bool, add: impl FnOnce(&mut Ui)) {
    list_row(ui, first, |ui| line(ui, SMALL_BUTTON_HEIGHT, add));
}

fn right<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) {
    ui.with_layout(Layout::right_to_left(Align::Center), add);
}

/// What the user asked the shell to do from this screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceAction {
    ChangeFolder,
}

/// `name` is the checkout's folder name, never its full path.
pub fn show(ui: &mut Ui, name: &str, view: &WorkspaceView, environment: &mut EnvironmentChecks) -> Option<WorkspaceAction> {
    environment.ensure_started();
    let mut action = None;
    page_head(ui, Title::Slab("Setup"), Some(Subtitle::Typed(name)), |ui| {
        if button(ui, "Change folder").clicked() {
            action = Some(WorkspaceAction::ChangeFolder);
        }
    });
    page_body(ui, |ui| {
        muted(ui, "Jobdesk reads your folder and never changes it. Only a task you start can change it, through Claude Code on this computer.");
        ui.add_space(SPLIT_GAP);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            split(ui, SPLIT_SIDE_BASIS, |main, side| {
                profile(main, view);
                tools(side, environment, view.has_salary_data);
            });
            ui.add_space(SPLIT_GAP);
            split(ui, SPLIT_SIDE_BASIS, |main, side| {
                portals(main, view);
                templates(side, view);
                side.add_space(12.0);
                sync(side, view);
                side.add_space(12.0);
                reset(side);
            });
        });
    });
    action
}

fn profile(ui: &mut Ui, view: &WorkspaceView) {
    panel(ui, |ui| {
        panel_header(ui, "Profile", Some(".claude/skills/job-application-assistant"));
        for (index, file) in view.profile.iter().enumerate() {
            list_row(ui, index == 0, |ui| line(ui, PROFILE_ROW_HEIGHT, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(profile_title(file.name.as_str()));
                    faint_mono(ui, file.name.as_str());
                });
                right(ui, |ui| {
                    match file.state {
                        ProfileState::Filled => small_command_action(ui, "Look for more in your documents\u{2026}", "/expand"),
                        ProfileState::NeedsSetup { .. } | ProfileState::Missing => small_command_action(ui, "Update", "/setup"),
                    };
                    match file.state {
                        ProfileState::Filled => tag(ui, "Filled", Tone::Hired),
                        ProfileState::NeedsSetup { placeholders } => tag(ui, format!("{placeholders} to fill"), Tone::Interview),
                        ProfileState::Missing => tag(ui, "Missing", Tone::Closed),
                    };
                });
            }));
        }
    });
}

fn portal_table(ui: &mut Ui, portals: &[PortalRow]) {
    let total = ui.available_width();
    let last = portals.len().saturating_sub(1);
    // Cells pad themselves, so the rules run unbroken.
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        TableBuilder::new(ui)
            .id_salt("workspace_portals")
            .vscroll(false)
            .striped(false)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact((total * PORTAL_COLUMNS[0]).floor()).clip(true))
            .column(Column::exact((total * PORTAL_COLUMNS[1]).floor()).clip(true))
            .column(Column::remainder().clip(true))
            .header(TABLE_HEAD_HEIGHT, |mut header| {
                for title in ["Portal", "Jobs seen", "State"] {
                    header.col(|ui| {
                        table_cell(ui, CellRule::Head, |ui| table_head(ui, title));
                    });
                }
            })
            .body(|body| {
                body.rows(TAG_HEIGHT + TABLE_CELL_PADDING.1 * 2.0, portals.len(), |mut line| {
                    let index = line.index();
                    let portal = &portals[index];
                    let rule = if index == last { CellRule::Last } else { CellRule::Row };
                    line.col(|ui| {
                        table_cell(ui, rule, |ui| ui.monospace(portal.name.as_str()));
                    });
                    line.col(|ui| {
                        table_cell(ui, rule, |ui| ui.monospace(portal.jobs_seen.to_string()));
                    });
                    line.col(|ui| {
                        table_cell(ui, rule, |ui| if portal.enabled { tag(ui, "Enabled", Tone::Hired) } else { tag(ui, "Disabled", Tone::Plain) });
                    });
                });
            });
    });
}

fn portals(ui: &mut Ui, view: &WorkspaceView) {
    panel(ui, |ui| {
        panel_header(ui, "Portals", Some(".agents/skills"));
        if view.portals.is_empty() {
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                muted(ui, "No portals installed.");
            });
        } else {
            portal_table(ui, &view.portals);
        }
        Frame::new().inner_margin(Margin::symmetric(0, 12)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = ITEM_GAP;
                small_command_action(ui, "Check portal health", "/scrape health");
                small_command_action(ui, "Add portal", "/add-portal");
            });
        });
    });
}

/// A row of the Environment slip: the name in mono (or plain words), a note, then a tag.
fn tool_row(ui: &mut Ui, first: bool, name: ToolName, note: &str, mark: impl FnOnce(&mut Ui)) {
    if !first {
        divider(ui);
    }
    Frame::new().inner_margin(Margin::symmetric(INSET, TOOL_ROW_PADDING)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        line(ui, TAG_HEIGHT, |ui| {
            match name {
                ToolName::Mono(name) => ui.monospace(name),
                ToolName::Plain(text) => ui.label(text),
            };
            right(ui, |ui| {
                mark(ui);
                muted(ui, note);
            });
        });
    });
}

/// The tool's name as the row shows it.
enum ToolName<'a> {
    Mono(&'a str),
    Plain(String),
}

/// What a profile file holds, in words. An unknown file shows its own name.
fn profile_title(file: &str) -> &str {
    match file {
        "01-candidate-profile.md" => "Your profile",
        "02-behavioral-profile.md" => "How you work",
        "03-writing-style.md" => "Your writing style",
        "04-job-evaluation.md" => "How jobs are judged",
        "05-cv-templates.md" => "CV templates",
        "06-cover-letter-templates.md" => "Cover letter templates",
        "07-interview-prep.md" => "Interview prep",
        other => other,
    }
}

fn tool_mark(ui: &mut Ui, check: &ToolCheck) {
    match (&check.version, check.spec.required) {
        (Some(version), _) => {
            tag(ui, "Ready", Tone::Hired).on_hover_text(version);
        }
        (None, true) => {
            tag(ui, "Missing", Tone::Closed);
        }
        (None, false) => {
            tag(ui, "Optional", Tone::Plain);
        }
    }
}

/// "2 of 3 ready": only required tools count, so a missing optional one never reads as a shortfall.
fn ready_note(results: &[ToolCheck]) -> String {
    let required = results.iter().filter(|check| check.spec.required);
    let ready = required.clone().filter(|check| check.version.is_some()).count();
    format!("{ready} of {} ready", required.count())
}

fn tools(ui: &mut Ui, environment: &mut EnvironmentChecks, has_salary_data: bool) {
    let running = environment.is_running();
    let mut recheck = false;
    let checks: &EnvironmentChecks = environment;
    let results = checks.results();
    let note = results.map(ready_note);
    slip(ui, |ui| {
        panel_header(ui, "Environment", note.as_deref());
        match results {
            Some(results) => {
                for (index, check) in results.iter().enumerate() {
                    if check.version.is_none() && check.spec.required {
                        // A shortfall reads as what it costs the user, then what is missing.
                        tool_row(ui, index == 0, ToolName::Plain(format!("{}: missing {}", check.spec.purpose, check.spec.name)), "", |ui| tool_mark(ui, check));
                        continue;
                    }
                    let note = if check.spec.name == "claude" { checks.version_of("claude").unwrap_or(check.spec.purpose) } else { check.spec.purpose };
                    tool_row(ui, index == 0, ToolName::Mono(check.spec.name), note, |ui| tool_mark(ui, check));
                }
            }
            None => {
                tool_row(ui, true, ToolName::Mono("Checking tools"), "", |_| {});
            }
        }
        tool_row(ui, false, ToolName::Mono("salary_data.json"), "Salary figures", |ui| {
            if has_salary_data {
                tag(ui, "Ready", Tone::Hired);
            } else {
                tag(ui, "Optional", Tone::Plain);
            }
        });
        panel_body(ui, |ui| {
            ui.horizontal(|ui| {
                // A running check is its own reason: the label says so, and no hover is needed.
                recheck = enabled_when(ui, !running, "Checking the tools now.", |ui| small_button(ui, if running { "Checking…" } else { "Check again" })).clicked();
            });
        });
    });
    if recheck {
        environment.recheck();
    }
}

fn templates(ui: &mut Ui, view: &WorkspaceView) {
    let label = |custom: bool, stock: &str| if custom { "Custom".to_owned() } else { stock.to_owned() };
    slip(ui, |ui| {
        panel_header(ui, "Templates", None);
        row(ui, true, |ui| {
            ui.label(format!("CV: {}", label(view.templates.custom_cv, "moderncv, banking")));
            right(ui, |ui| tag(ui, "Active", Tone::Hired));
        });
        row(ui, false, |ui| {
            ui.label(format!("Letter: {}", label(view.templates.custom_letter, "cover.cls")));
            right(ui, |ui| tag(ui, "Active", Tone::Hired));
        });
        for name in &view.templates.registered {
            row(ui, false, |ui| {
                faint_mono(ui, name);
            });
        }
        panel_body(ui, |ui| small_command_action(ui, "Add template", "/add-template"));
    });
}

fn sync(ui: &mut Ui, view: &WorkspaceView) {
    slip(ui, |ui| {
        panel_header(ui, "Sync", None);
        row(ui, true, |ui| {
            ui.label(view.sync.gmail_last_sync.as_deref().map_or("Gmail, not synced yet".to_owned(), |last| format!("Gmail, last sync {last}")));
            right(ui, |ui| small_command_action(ui, "Check now", "/gmail-sync"));
        });
        row(ui, false, |ui| {
            ui.label(if view.sync.notion_set_up { "Notion, connected" } else { "Notion, not connected" });
            right(ui, |ui| small_command_action(ui, "Sync now", "/notion-sync"));
        });
    });
}

fn reset(ui: &mut Ui) {
    slip(ui, |ui| {
        panel_header_tone(ui, "Reset", None, Tone::Closed);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                right(ui, |ui| {
                    danger_command_button(ui, "/reset");
                    ui.add_space(ITEM_GAP);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        let text = rich(ui, "Clear profile data, documents, or both.", Type::BODY, Ink::Muted);
                        ui.add(egui::Label::new(text).wrap());
                    });
                });
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::application::TOOLS;

    #[test]
    fn only_required_tools_count_towards_ready() {
        let checks: Vec<ToolCheck> = TOOLS.iter().map(|spec| ToolCheck { spec: *spec, version: (spec.name != "bun" && spec.required).then(|| "1.0".to_owned()) }).collect();
        assert_eq!(ready_note(&checks), "4 of 5 ready", "pdftotext is optional and missing");
    }
}
