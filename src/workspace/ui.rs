use egui::{ScrollArea, Ui};

use crate::ui_kit::{Tone, command_button, divider, faint_mono, muted, panel, panel_body, panel_header, tag};
use crate::workspace::application::{AuthMode, EnvironmentChecks, ProfileState, ToolCheck, WorkspaceView};

fn row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 6)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(add);
    });
}

fn right(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
}

pub fn show(ui: &mut Ui, view: &WorkspaceView, environment: &mut EnvironmentChecks, auth: AuthMode) {
    environment.ensure_started();
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
}

fn profile(ui: &mut Ui, view: &WorkspaceView) {
    panel(ui, |ui| {
        panel_header(ui, "Profile", Some(".claude/skills"));
        for (index, file) in view.profile.iter().enumerate() {
            if index > 0 {
                divider(ui);
            }
            row(ui, |ui| {
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
            if index > 0 {
                divider(ui);
            }
            row(ui, |ui| {
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

fn tool_row(ui: &mut Ui, check: &ToolCheck) {
    row(ui, |ui| {
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
                    if index > 0 {
                        divider(ui);
                    }
                    tool_row(ui, check);
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
                if ui.add_enabled(!environment.is_running(), egui::Button::new("Check again")).clicked() {
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
        row(ui, |ui| {
            muted(ui, "CV");
            right(ui, |ui| {
                ui.label(label(view.templates.custom_cv, "moderncv, banking"));
            });
        });
        divider(ui);
        row(ui, |ui| {
            muted(ui, "Cover letter");
            right(ui, |ui| {
                ui.label(label(view.templates.custom_letter, "cover.cls"));
            });
        });
        for name in &view.templates.registered {
            divider(ui);
            row(ui, |ui| {
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
        row(ui, |ui| {
            muted(ui, view.sync.gmail_last_sync.as_deref().map_or("Gmail, not synced yet".to_owned(), |last| format!("Gmail, last sync {last}")));
            right(ui, |ui| command_button(ui, "/gmail-sync"));
        });
        divider(ui);
        row(ui, |ui| {
            muted(ui, if view.sync.notion_set_up { "Notion, connected" } else { "Notion, not connected" });
            right(ui, |ui| command_button(ui, "/notion-sync"));
        });
    });
}

fn reset(ui: &mut Ui) {
    panel(ui, |ui| {
        row(ui, |ui| {
            muted(ui, "Clear profile data, documents, or both.");
            right(ui, |ui| command_button(ui, "/reset"));
        });
    });
}
