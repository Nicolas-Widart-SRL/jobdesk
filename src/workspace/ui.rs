use egui::{ScrollArea, Ui};

use crate::ui_kit::{Subtitle, Title, Tone, button, command_button, divider, faint_mono, list_row, muted, page_body, page_head, panel, panel_body, panel_header, tag};
use crate::workspace::application::{AuthMode, EnvironmentChecks, ProfileState, ToolCheck, WorkspaceView};

fn row(ui: &mut Ui, first: bool, add: impl FnOnce(&mut Ui)) {
    list_row(ui, first, |ui| ui.horizontal(add));
}

fn right(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
}

/// What the user asked the shell to do from this screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceAction {
    ChangeFolder,
}

/// `name` is the checkout's folder name, never its full path.
pub fn show(ui: &mut Ui, name: &str, view: &WorkspaceView, environment: &mut EnvironmentChecks, auth: AuthMode) -> Option<WorkspaceAction> {
    environment.ensure_started();
    let mut action = None;
    page_head(ui, Title::Slab("Workspace"), Some(Subtitle::Typed(name)), |ui| {
        if button(ui, "Change folder").clicked() {
            action = Some(WorkspaceAction::ChangeFolder);
        }
    });
    page_body(ui, |ui| {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns_const(|[left, right_side]| {
                profile(left, view);
                left.add_space(12.0);
                portals(left, view);
                tools(right_side, environment);
                right_side.add_space(12.0);
                authentication(right_side, auth);
                right_side.add_space(12.0);
                templates(right_side, view);
                right_side.add_space(12.0);
                sync(right_side, view);
                right_side.add_space(12.0);
                reset(right_side);
            });
        });
    });
    action
}

fn profile(ui: &mut Ui, view: &WorkspaceView) {
    panel(ui, |ui| {
        panel_header(ui, "Profile", Some(".claude/skills"));
        for (index, file) in view.profile.iter().enumerate() {
            row(ui, index == 0, |ui| {
                ui.monospace(file.name.as_str());
                right(ui, |ui| match file.state {
                    ProfileState::Filled => {
                        tag(ui, "Filled", Tone::Hired);
                    }
                    ProfileState::NeedsSetup { placeholders } => {
                        tag(ui, format!("{placeholders} to fill"), Tone::Interview);
                    }
                    ProfileState::Missing => {
                        tag(ui, "Missing", Tone::Closed);
                    }
                });
            });
        }
        divider(ui);
        panel_body(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                command_button(ui, "/setup");
                command_button(ui, "/expand");
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
        }
        for (index, portal) in view.portals.iter().enumerate() {
            row(ui, index == 0, |ui| {
                ui.monospace(portal.name.as_str());
                right(ui, |ui| {
                    if portal.enabled {
                        tag(ui, "Enabled", Tone::Hired);
                    } else {
                        tag(ui, "Disabled", Tone::Plain);
                    }
                    muted(ui, format!("{} jobs seen", portal.jobs_seen));
                });
            });
        }
        divider(ui);
        panel_body(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                command_button(ui, "/scrape health");
                command_button(ui, "/add-portal");
            });
        });
    });
}

fn tool_row(ui: &mut Ui, first: bool, check: &ToolCheck) {
    row(ui, first, |ui| {
        ui.monospace(check.spec.name);
        muted(ui, check.spec.purpose);
        right(ui, |ui| match (&check.version, check.spec.required) {
            (Some(version), _) => {
                tag(ui, "Ready", Tone::Hired).on_hover_text(version);
            }
            (None, true) => {
                tag(ui, "Missing", Tone::Closed);
            }
            (None, false) => {
                tag(ui, "Optional", Tone::Plain);
            }
        });
    });
}

fn tools(ui: &mut Ui, environment: &mut EnvironmentChecks) {
    panel(ui, |ui| {
        panel_header(ui, "Environment", None);
        match environment.results() {
            Some(results) => {
                for (index, check) in results.iter().enumerate() {
                    tool_row(ui, index == 0, check);
                }
            }
            None => {
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    muted(ui, "Checking tools");
                });
            }
        }
        divider(ui);
        panel_body(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled_ui(!environment.is_running(), |ui| button(ui, "Check again")).inner.clicked() {
                    environment.recheck();
                }
            });
        });
    });
}

fn authentication(ui: &mut Ui, auth: AuthMode) {
    panel(ui, |ui| {
        panel_header(ui, "Authentication", None);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            match auth {
                AuthMode::NoRunYet => {
                    tag(ui, "Known after first run", Tone::Plain);
                }
                AuthMode::ClaudeLogin => {
                    tag(ui, "Your Claude CLI login", Tone::Active);
                }
                AuthMode::ApiKey => {
                    tag(ui, "API key or provider", Tone::Hired);
                }
            }
            muted(ui, "Jobdesk stores no credentials. The Claude CLI handles authentication itself.");
        });
    });
}

fn templates(ui: &mut Ui, view: &WorkspaceView) {
    let label = |custom: bool, stock: &str| if custom { "Custom".to_owned() } else { stock.to_owned() };
    panel(ui, |ui| {
        panel_header(ui, "Templates", Some("templates"));
        row(ui, true, |ui| {
            muted(ui, "CV");
            right(ui, |ui| {
                ui.label(label(view.templates.custom_cv, "moderncv, banking"));
            });
        });
        row(ui, false, |ui| {
            muted(ui, "Cover letter");
            right(ui, |ui| {
                ui.label(label(view.templates.custom_letter, "cover.cls"));
            });
        });
        for name in &view.templates.registered {
            row(ui, false, |ui| {
                faint_mono(ui, name);
            });
        }
        divider(ui);
        panel_body(ui, |ui| command_button(ui, "/add-template"));
    });
}

fn sync(ui: &mut Ui, view: &WorkspaceView) {
    panel(ui, |ui| {
        panel_header(ui, "Sync", None);
        row(ui, true, |ui| {
            muted(ui, view.sync.gmail_last_sync.as_deref().map_or("Gmail, not synced yet".to_owned(), |last| format!("Gmail, last sync {last}")));
            right(ui, |ui| command_button(ui, "/gmail-sync"));
        });
        row(ui, false, |ui| {
            muted(ui, if view.sync.notion_set_up { "Notion, connected" } else { "Notion, not connected" });
            right(ui, |ui| command_button(ui, "/notion-sync"));
        });
    });
}

fn reset(ui: &mut Ui) {
    panel(ui, |ui| {
        row(ui, true, |ui| {
            muted(ui, "Clear profile data, documents, or both.");
            right(ui, |ui| command_button(ui, "/reset"));
        });
    });
}
