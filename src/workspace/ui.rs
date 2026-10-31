use egui::{Align, Frame, Layout, Margin, ScrollArea, Ui, vec2};

use crate::ui_kit::tokens::{INSET, ITEM_GAP, ROW_ITEM_GAP, SMALL_BUTTON_HEIGHT, SPLIT_GAP, SPLIT_SIDE_BASIS, TABLE_CELL_PADDING, TABLE_HEAD_PADDING, TAG_HEIGHT};
use crate::ui_kit::{
    Ink, Subtitle, Title, Tone, Type, button, danger_command_button, divider, faint_mono, list_row, muted, page_body, page_head, panel, panel_body, panel_header, panel_header_tone, rich,
    slip, small_button, small_command_action, table_head, table_rule, tag,
};
use crate::workspace::application::{EnvironmentChecks, PortalRow, ProfileState, ToolCheck, WorkspaceView};

/// Environment rows sit closer than other list rows.
const TOOL_ROW_PADDING: i8 = 6;
/// Shares of the portal table's width: portal, jobs seen, state.
const PORTAL_COLUMNS: [f32; 3] = [0.5, 0.3, 0.2];

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

/// A wide main column and a side column of slips, side by side.
fn split(ui: &mut Ui, main: impl FnOnce(&mut Ui), side: impl FnOnce(&mut Ui)) {
    let side_width = SPLIT_SIDE_BASIS.min((ui.available_width() - SPLIT_GAP) / 2.0);
    let main_width = ui.available_width() - SPLIT_GAP - side_width;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = SPLIT_GAP;
        for (width, add) in [(main_width, Box::new(main) as Box<dyn FnOnce(&mut Ui)>), (side_width, Box::new(side))] {
            ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(width);
                add(ui);
            });
        }
    });
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
    page_head(ui, Title::Slab("Workspace"), Some(Subtitle::Typed(name)), |ui| {
        if button(ui, "Change folder").clicked() {
            action = Some(WorkspaceAction::ChangeFolder);
        }
    });
    page_body(ui, |ui| {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            split(ui, |ui| profile(ui, view), |ui| tools(ui, environment, view.has_salary_data));
            ui.add_space(SPLIT_GAP);
            split(ui, |ui| portals(ui, view), |ui| {
                templates(ui, view);
                ui.add_space(12.0);
                sync(ui, view);
                ui.add_space(12.0);
                reset(ui);
            });
        });
    });
    action
}

fn profile(ui: &mut Ui, view: &WorkspaceView) {
    panel(ui, |ui| {
        panel_header(ui, "Profile", Some(".claude/skills/job-application-assistant"));
        for (index, file) in view.profile.iter().enumerate() {
            row(ui, index == 0, |ui| {
                ui.monospace(file.name.as_str());
                right(ui, |ui| {
                    match file.state {
                        ProfileState::Filled => small_command_action(ui, "Find signals", "/expand"),
                        ProfileState::NeedsSetup { .. } | ProfileState::Missing => small_command_action(ui, "Update", "/setup"),
                    };
                    match file.state {
                        ProfileState::Filled => tag(ui, "Filled", Tone::Hired),
                        ProfileState::NeedsSetup { placeholders } => tag(ui, format!("{placeholders} to fill"), Tone::Interview),
                        ProfileState::Missing => tag(ui, "Missing", Tone::Closed),
                    };
                });
            });
        }
    });
}

/// One line of the portal table, its cells placed by `PORTAL_COLUMNS`.
fn portal_line(ui: &mut Ui, padding: (f32, f32), mut cell: impl FnMut(&mut Ui, usize)) {
    let width = ui.available_width();
    Frame::new().inner_margin(Margin::symmetric(0, padding.1 as i8)).show(ui, |ui| {
        line(ui, TAG_HEIGHT, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (column, share) in PORTAL_COLUMNS.iter().enumerate() {
                let cell_width = width * share;
                ui.allocate_ui_with_layout(vec2(cell_width, TAG_HEIGHT), Layout::left_to_right(Align::Center), |ui| {
                    ui.set_width(cell_width);
                    ui.add_space(padding.0);
                    cell(ui, column);
                });
            }
        });
    });
}

fn portal_cell(ui: &mut Ui, portal: &PortalRow, column: usize) {
    match column {
        0 => {
            ui.monospace(portal.name.as_str());
        }
        1 => {
            ui.monospace(portal.jobs_seen.to_string());
        }
        _ if portal.enabled => {
            tag(ui, "Enabled", Tone::Hired);
        }
        _ => {
            tag(ui, "Disabled", Tone::Plain);
        }
    }
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
            portal_line(ui, TABLE_HEAD_PADDING, |ui, column| {
                table_head(ui, ["Portal", "Jobs seen", "State"][column]);
            });
            table_rule(ui);
            for (index, portal) in view.portals.iter().enumerate() {
                if index > 0 {
                    divider(ui);
                }
                portal_line(ui, TABLE_CELL_PADDING, |ui, column| portal_cell(ui, portal, column));
            }
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

/// A row of the Environment slip: the name in mono, a note, then a tag.
fn tool_row(ui: &mut Ui, first: bool, name: &str, note: &str, mark: impl FnOnce(&mut Ui)) {
    if !first {
        divider(ui);
    }
    Frame::new().inner_margin(Margin::symmetric(INSET, TOOL_ROW_PADDING)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        line(ui, TAG_HEIGHT, |ui| {
            ui.monospace(name);
            right(ui, |ui| {
                mark(ui);
                muted(ui, note);
            });
        });
    });
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

fn tools(ui: &mut Ui, environment: &mut EnvironmentChecks, has_salary_data: bool) {
    let results = environment.results().map(<[ToolCheck]>::to_vec);
    let ready = results.as_ref().map(|results| results.iter().filter(|check| check.version.is_some()).count() + usize::from(has_salary_data));
    let note = ready.zip(results.as_ref()).map(|(ready, results)| format!("{ready} of {} ready", results.len() + 1));
    slip(ui, |ui| {
        panel_header(ui, "Environment", note.as_deref());
        match &results {
            Some(results) => {
                for (index, check) in results.iter().enumerate() {
                    let note = if check.spec.name == "claude" { environment.version_of("claude").unwrap_or(check.spec.purpose) } else { check.spec.purpose };
                    tool_row(ui, index == 0, check.spec.name, note, |ui| tool_mark(ui, check));
                }
            }
            None => {
                tool_row(ui, true, "Checking tools", "", |_| {});
            }
        }
        tool_row(ui, false, "salary_data.json", "Optional", |ui| {
            if has_salary_data {
                tag(ui, "Ready", Tone::Hired);
            } else {
                tag(ui, "Missing", Tone::Closed);
            }
        });
        panel_body(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled_ui(!environment.is_running(), |ui| small_button(ui, "Check again")).inner.clicked() {
                    environment.recheck();
                }
            });
        });
    });
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
