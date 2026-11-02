use egui::{Align, Frame, Layout, Margin, ScrollArea, Ui, vec2};
use egui_extras::{Column, TableBuilder};

use crate::ui_kit::tokens::{INSET, ITEM_GAP, ROW_ITEM_GAP, ROW_PADDING, SMALL_BUTTON_HEIGHT, SPLIT_GAP, SPLIT_SIDE_BASIS, TABLE_CELL_PADDING, TABLE_HEAD_HEIGHT, TAG_HEIGHT};
use crate::ui_kit::{
    CellRule, Ink, ScreenState, Subtitle, Title, Tone, Type, button, command_action, danger_command_button, divider, enabled_when, faint_mono, hit_mark, list_row, muted, page_body, page_head, panel, panel_body, panel_header,
    panel_header_tone, hidden_note, rich, setting_row, setting_section, settings_page, skeleton, skeleton_lines, slip, small_button, small_command_action, split, table_cell, table_head, tag,
};
use crate::shared_kernel::Source;
use crate::workspace::application::{AuthMode, EnvironmentChecks, PortalRow, ProfileState, ToolCheck, WorkspaceView};

/// Environment rows sit closer than other list rows.
const TOOL_ROW_PADDING: i8 = 6;
/// Shares of the portal table's width: portal, jobs seen, state.
const PORTAL_COLUMNS: [f32; 3] = [0.5, 0.3, 0.2];

/// A profile row holds the plain title over the file name.
const PROFILE_ROW_HEIGHT: f32 = 38.0;

/// What Jobdesk promises about the folder, on Setup and on Runtime status.
const PROMISE: &str = "Jobdesk reads your folder and never changes it. Only a task you start can change it, through Claude Code on this computer.";

/// Runtime status's title and lead, as Settings indexes them for search.
/// Why Change folder waits: a new folder would leave the running task behind.
const FOLDER_BUSY: &str = "A task is running. Change the folder once it ends.";

const RUNTIME_TITLE: &str = "Runtime status";
const RUNTIME_LEAD: &str = "Whether Claude Code, your profile, tools, portals, templates and sync are ready.";

/// A row of fixed height, so its text centres on the tags and buttons beside it.
fn line(ui: &mut Ui, height: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
        add(ui);
    });
}

/// Where rows sit: in Setup's panels and slips, or flush on a Settings page, which pads them itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rows {
    Listed,
    Flush,
}

impl Rows {
    fn row<R>(self, ui: &mut Ui, first: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
        match self {
            Rows::Listed => list_row(ui, first, add),
            Rows::Flush => flush_row(ui, first, add),
        }
    }

    /// How far a tool row's words sit in from the edge.
    fn inset(self) -> i8 {
        match self {
            Rows::Listed => INSET,
            Rows::Flush => 0,
        }
    }
}

/// A row ruled off from the one above unless `first`, with no inset of its own.
fn flush_row<R>(ui: &mut Ui, first: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !first {
        divider(ui);
    }
    Frame::new()
        .inner_margin(Margin::symmetric(0, ROW_PADDING))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP;
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn row(ui: &mut Ui, rows: Rows, first: bool, add: impl FnOnce(&mut Ui)) {
    rows.row(ui, first, |ui| line(ui, SMALL_BUTTON_HEIGHT, add));
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
/// Setup draws the alert for every source that failed. Of those, it reads only the saved jobs, for each portal's jobs seen,
/// so a failed one hides those counts. Until the first read, the slips show their names over grey lines.
/// While `task_live`, Change folder waits for the task to end.
pub fn show(ui: &mut Ui, state: &ScreenState, name: &str, view: &WorkspaceView, environment: &mut EnvironmentChecks, task_live: bool) -> Option<WorkspaceAction> {
    environment.ensure_started();
    let mut action = None;
    let has_documents = view.profile.iter().any(|file| matches!(file.state, ProfileState::Filled));
    let subtitle = if state.reading() { Subtitle::Text("Reading your profile, tools and portals…") } else { Subtitle::Typed(name) };
    page_head(ui, Title::Slab("Setup"), Some(subtitle), |ui| {
        if enabled_when(ui, !task_live, FOLDER_BUSY, |ui| button(ui, "Change folder")).clicked() {
            action = Some(WorkspaceAction::ChangeFolder);
        }
        if has_documents {
            command_action(ui, "Look for more in your documents", "/expand");
        }
    });
    page_body(ui, |ui| {
        (state.alerts)(ui);
        muted(ui, PROMISE);
        ui.add_space(SPLIT_GAP);
        if state.reading() {
            loading(ui);
            return;
        }
        let jobs_failed = state.cannot_read(Source::Jobs);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            split(ui, SPLIT_SIDE_BASIS, |main, side| {
                profile(main, view);
                tools(side, environment, view.has_salary_data);
            });
            ui.add_space(SPLIT_GAP);
            split(ui, SPLIT_SIDE_BASIS, |main, side| {
                portals(main, view, jobs_failed);
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

/// Settings › Runtime status: Setup's content in one column, under Claude Code's version and sign-in.
/// `sign_in` is what the last task reported. Without `folder_open`, only Claude Code and the tools show.
/// Headings and labels mark the hits of the search `query`.
pub fn runtime_status(ui: &mut Ui, state: &ScreenState, view: &WorkspaceView, environment: &mut EnvironmentChecks, sign_in: AuthMode, folder_open: bool, query: &str) {
    environment.ensure_started();
    let running = environment.is_running();
    let read = folder_open && !state.reading();
    let has_documents = read && view.profile.iter().any(|file| matches!(file.state, ProfileState::Filled));
    let checks: &EnvironmentChecks = environment;
    let (recheck, ()) = settings_page(
        ui,
        RUNTIME_TITLE,
        RUNTIME_LEAD,
        |ui| {
            // Right to left, so the first one goes at the far right.
            if has_documents {
                small_command_action(ui, "Look for more in your documents", "/expand");
            }
            // A running check is its own reason: the label says so, and no hover is needed.
            enabled_when(ui, !running, "Checking the tools now.", |ui| small_button(ui, if running { "Checking…" } else { "Check again" })).clicked()
        },
        |ui| {
            hit_mark(ui, PROMISE, Type::BODY, Ink::Muted, query);
            (state.alerts)(ui);
            claude_code(ui, checks, sign_in, query);
            // Until the folder is open and read, only Claude Code and the tools show, and a line says why.
            let waiting = match (folder_open, read) {
                (false, _) => Some("Choose your job search folder to see your profile, portals, templates and sync."),
                (true, false) => Some("Reading your profile, portals, templates and sync…"),
                (true, true) => None,
            };
            if waiting.is_none() {
                setting_section(ui, "Profile", Some(".claude/skills/job-application-assistant"), query);
                profile_rows(ui, view, Rows::Flush);
            }
            // Claude Code has its own section above, so the count leaves it out as the rows do.
            setting_section(ui, "Environment", checks.results().map(|results| ready_note(results, false)).as_deref(), query);
            tool_rows(ui, checks, Rows::Flush, false, read.then_some(view.has_salary_data));
            if let Some(waiting) = waiting {
                ui.add_space(SPLIT_GAP);
                muted(ui, waiting);
                return;
            }
            setting_section(ui, "Portals", Some(".agents/skills"), query);
            portal_rows(ui, view, state.cannot_read(Source::Jobs));
            setting_section(ui, "Templates", None, query);
            template_rows(ui, view, Rows::Flush);
            actions_line(ui, |ui| {
                small_command_action(ui, "Add template", "/add-template");
            });
            setting_section(ui, "Sync", None, query);
            sync_rows(ui, view, Rows::Flush);
            setting_section(ui, "Reset", None, query);
            flush_row(ui, true, reset_line);
        },
    );
    if recheck {
        environment.recheck();
    }
}

/// What runs every task: the program's version, and how the last task signed in.
fn claude_code(ui: &mut Ui, checks: &EnvironmentChecks, sign_in: AuthMode, query: &str) {
    setting_section(ui, "Claude Code", None, query);
    let found = checks.results().map(|results| results.iter().any(|check| check.spec.name == "claude" && check.version.is_some()));
    setting_row(ui, "On this computer", "Jobdesk runs this program for every task.", query, |ui| match found {
        Some(true) => {
            tag(ui, "Installed", Tone::Hired);
            if let Some(version) = checks.version_of("claude") {
                ui.monospace(version);
            }
        }
        Some(false) => {
            tag(ui, "Missing", Tone::Closed);
        }
        None => {
            muted(ui, "Checking…");
        }
    });
    let word = match sign_in {
        AuthMode::ClaudeLogin => "Claude sign-in",
        AuthMode::ApiKey => "API key",
        AuthMode::NoRunYet => "After the first task",
    };
    setting_row(ui, "Signed in with", "As the last task reported.", query, |ui| {
        ui.label(rich(ui, word, Type::STRONG, Ink::Text));
    });
}

/// A wrapped line of buttons under a section's rows.
fn actions_line(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new().inner_margin(Margin::symmetric(0, 12)).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = ITEM_GAP;
            add(ui);
        });
    });
}

/// Runtime status's portals: the table flush on the page, then its actions.
fn portal_rows(ui: &mut Ui, view: &WorkspaceView, jobs_failed: bool) {
    if view.portals.is_empty() {
        flush_row(ui, true, |ui| muted(ui, "No portals installed."));
    } else {
        portal_table(ui, &view.portals, !jobs_failed);
        if jobs_failed {
            flush_row(ui, false, |ui| hidden_note(ui, "Jobs seen are hidden while your saved jobs cannot be read."));
        }
    }
    actions_line(ui, portal_actions);
}

/// A section's name over grey lines where its rows go.
fn loading_lines(ui: &mut Ui, title: &str, note: Option<&str>, widths: &[f32]) {
    panel_header(ui, title, note);
    panel_body(ui, |ui| {
        ui.set_width(ui.available_width());
        skeleton_lines(ui, widths, skeleton);
    });
}

/// The sections before the first read, in their places.
fn loading(ui: &mut Ui) {
    split(ui, SPLIT_SIDE_BASIS, |main, side| {
        panel(main, |ui| loading_lines(ui, "Profile", Some(".claude/skills/job-application-assistant"), &[220.0, 180.0, 240.0, 200.0, 170.0, 230.0, 190.0]));
        slip(side, |ui| loading_lines(ui, "Environment", None, &[200.0, 170.0, 190.0, 160.0, 180.0]));
    });
    ui.add_space(SPLIT_GAP);
    split(ui, SPLIT_SIDE_BASIS, |main, side| {
        panel(main, |ui| loading_lines(ui, "Portals", Some(".agents/skills"), &[180.0, 150.0, 200.0]));
        slip(side, |ui| loading_lines(ui, "Templates", None, &[190.0, 160.0]));
        side.add_space(12.0);
        slip(side, |ui| loading_lines(ui, "Sync", None, &[200.0, 170.0]));
    });
}

fn profile(ui: &mut Ui, view: &WorkspaceView) {
    panel(ui, |ui| {
        panel_header(ui, "Profile", Some(".claude/skills/job-application-assistant"));
        profile_rows(ui, view, Rows::Listed);
    });
}

fn profile_rows(ui: &mut Ui, view: &WorkspaceView, rows: Rows) {
    for (index, file) in view.profile.iter().enumerate() {
        rows.row(ui, index == 0, |ui| line(ui, PROFILE_ROW_HEIGHT, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(profile_title(file.name.as_str()));
                faint_mono(ui, file.name.as_str());
            });
            right(ui, |ui| {
                if !matches!(file.state, ProfileState::Filled) {
                    small_command_action(ui, "Update", "/setup");
                }
                match file.state {
                    ProfileState::Filled => tag(ui, "Filled", Tone::Hired),
                    ProfileState::NeedsSetup { placeholders } => tag(ui, format!("{placeholders} to fill"), Tone::Interview),
                    ProfileState::Missing => tag(ui, "Missing", Tone::Closed),
                };
            });
        }));
    }
}

/// Without `counted`, the Jobs seen column is left out: its counts would read zero.
fn portal_table(ui: &mut Ui, portals: &[PortalRow], counted: bool) {
    let total = ui.available_width();
    let last = portals.len().saturating_sub(1);
    let titles: &[&str] = if counted { &["Portal", "Jobs seen", "State"] } else { &["Portal", "State"] };
    // Cells pad themselves, so the rules run unbroken.
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        let mut table = TableBuilder::new(ui)
            .id_salt("workspace_portals")
            .vscroll(false)
            .striped(false)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact((total * PORTAL_COLUMNS[0]).floor()).clip(true));
        if counted {
            table = table.column(Column::exact((total * PORTAL_COLUMNS[1]).floor()).clip(true));
        }
        table
            .column(Column::remainder().clip(true))
            .header(TABLE_HEAD_HEIGHT, |mut header| {
                for &title in titles {
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
                    if counted {
                        line.col(|ui| {
                            table_cell(ui, rule, |ui| ui.monospace(portal.jobs_seen.to_string()));
                        });
                    }
                    line.col(|ui| {
                        table_cell(ui, rule, |ui| if portal.enabled { tag(ui, "Enabled", Tone::Hired) } else { tag(ui, "Disabled", Tone::Plain) });
                    });
                });
            });
    });
}

/// The jobs seen on each portal come from the saved jobs, so they hide while those cannot be read.
fn portals(ui: &mut Ui, view: &WorkspaceView, jobs_failed: bool) {
    panel(ui, |ui| {
        panel_header(ui, "Portals", Some(".agents/skills"));
        if view.portals.is_empty() {
            panel_body(ui, |ui| {
                ui.set_width(ui.available_width());
                muted(ui, "No portals installed.");
            });
        } else {
            portal_table(ui, &view.portals, !jobs_failed);
            if jobs_failed {
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    hidden_note(ui, "Jobs seen are hidden while your saved jobs cannot be read.");
                });
            }
        }
        actions_line(ui, portal_actions);
    });
}

fn portal_actions(ui: &mut Ui) {
    small_command_action(ui, "Check portal health", "/scrape health");
    small_command_action(ui, "Add portal", "/add-portal");
}

/// A row of the Environment slip: the name in mono (or plain words), a note, then a tag.
fn tool_row(ui: &mut Ui, rows: Rows, first: bool, name: ToolName, note: &str, mark: impl FnOnce(&mut Ui)) {
    if !first {
        divider(ui);
    }
    Frame::new().inner_margin(Margin::symmetric(rows.inset(), TOOL_ROW_PADDING)).show(ui, |ui| {
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
/// Without `with_claude`, Claude Code is left out, as `tool_rows` leaves out its row.
fn ready_note(results: &[ToolCheck], with_claude: bool) -> String {
    let required = results.iter().filter(|check| check.spec.required && (with_claude || check.spec.name != "claude"));
    let ready = required.clone().filter(|check| check.version.is_some()).count();
    format!("{ready} of {} ready", required.count())
}

fn tools(ui: &mut Ui, environment: &mut EnvironmentChecks, has_salary_data: bool) {
    let running = environment.is_running();
    let mut recheck = false;
    let checks: &EnvironmentChecks = environment;
    let note = checks.results().map(|results| ready_note(results, true));
    slip(ui, |ui| {
        panel_header(ui, "Environment", note.as_deref());
        tool_rows(ui, checks, Rows::Listed, true, Some(has_salary_data));
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

/// The tools, each with its mark, then the salary file when `salary` says whether it is there.
/// Without `with_claude`, Claude Code is left out, because Runtime status shows it on its own above.
fn tool_rows(ui: &mut Ui, checks: &EnvironmentChecks, rows: Rows, with_claude: bool, salary: Option<bool>) {
    let mut first = true;
    match checks.results() {
        Some(results) => {
            for check in results.iter().filter(|check| with_claude || check.spec.name != "claude") {
                if check.version.is_none() && check.spec.required {
                    // A shortfall reads as what it costs the user, then what is missing.
                    tool_row(ui, rows, first, ToolName::Plain(format!("{}: missing {}", check.spec.purpose, check.spec.name)), "", |ui| tool_mark(ui, check));
                } else {
                    let note = if check.spec.name == "claude" { checks.version_of("claude").unwrap_or(check.spec.purpose) } else { check.spec.purpose };
                    tool_row(ui, rows, first, ToolName::Mono(check.spec.name), note, |ui| tool_mark(ui, check));
                }
                first = false;
            }
        }
        None => {
            tool_row(ui, rows, true, ToolName::Mono("Checking tools"), "", |_| {});
            first = false;
        }
    }
    if let Some(has_salary_data) = salary {
        tool_row(ui, rows, first, ToolName::Mono("salary_data.json"), "Salary figures", |ui| {
            if has_salary_data {
                tag(ui, "Ready", Tone::Hired);
            } else {
                tag(ui, "Optional", Tone::Plain);
            }
        });
    }
}

fn templates(ui: &mut Ui, view: &WorkspaceView) {
    slip(ui, |ui| {
        panel_header(ui, "Templates", None);
        template_rows(ui, view, Rows::Listed);
        panel_body(ui, |ui| small_command_action(ui, "Add template", "/add-template"));
    });
}

fn template_rows(ui: &mut Ui, view: &WorkspaceView, rows: Rows) {
    let label = |custom: bool, stock: &str| if custom { "Custom".to_owned() } else { stock.to_owned() };
    row(ui, rows, true, |ui| {
        ui.label(format!("CV: {}", label(view.templates.custom_cv, "moderncv, banking")));
        right(ui, |ui| tag(ui, "Active", Tone::Hired));
    });
    row(ui, rows, false, |ui| {
        ui.label(format!("Letter: {}", label(view.templates.custom_letter, "cover.cls")));
        right(ui, |ui| tag(ui, "Active", Tone::Hired));
    });
    for name in &view.templates.registered {
        row(ui, rows, false, |ui| {
            faint_mono(ui, name);
        });
    }
}

fn sync(ui: &mut Ui, view: &WorkspaceView) {
    slip(ui, |ui| {
        panel_header(ui, "Sync", None);
        sync_rows(ui, view, Rows::Listed);
    });
}

fn sync_rows(ui: &mut Ui, view: &WorkspaceView, rows: Rows) {
    row(ui, rows, true, |ui| {
        ui.label(view.sync.gmail_last_sync.as_deref().map_or("Gmail, not synced yet".to_owned(), |last| format!("Gmail, last sync {last}")));
        right(ui, |ui| small_command_action(ui, "Check now", "/gmail-sync"));
    });
    row(ui, rows, false, |ui| {
        ui.label(if view.sync.notion_set_up { "Notion, connected" } else { "Notion, not connected" });
        right(ui, |ui| small_command_action(ui, "Sync now", "/notion-sync"));
    });
}

fn reset(ui: &mut Ui) {
    slip(ui, |ui| {
        panel_header_tone(ui, "Reset", None, Tone::Closed);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            reset_line(ui);
        });
    });
}

/// What `/reset` clears, with its button at the right.
fn reset_line(ui: &mut Ui) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::application::TOOLS;

    #[test]
    fn only_required_tools_count_towards_ready() {
        let checks: Vec<ToolCheck> = TOOLS.iter().map(|spec| ToolCheck { spec: *spec, version: (spec.name != "bun" && spec.required).then(|| "1.0".to_owned()) }).collect();
        assert_eq!(ready_note(&checks, true), "4 of 5 ready", "pdftotext is optional and missing");
        assert_eq!(ready_note(&checks, false), "3 of 4 ready", "Claude Code's row is left out, so its count is too");
    }

    #[test]
    fn without_claude_the_note_counts_only_the_rows_shown() {
        let checks: Vec<ToolCheck> = TOOLS.iter().map(|spec| ToolCheck { spec: *spec, version: (spec.name != "claude").then(|| "1.0".to_owned()) }).collect();
        assert_eq!(ready_note(&checks, false), "4 of 4 ready", "every row Runtime status shows under Environment is ready");
        assert_eq!(ready_note(&checks, true), "4 of 5 ready");
    }

    /// Every tool answers except `missing`.
    struct Without(&'static str);

    impl crate::workspace::application::ToolProber for Without {
        fn probe(&self, tool: &crate::workspace::application::ToolSpec) -> Option<String> {
            (tool.name != self.0).then(|| "2.1.4 (Claude Code)".to_owned())
        }
    }

    fn setup(ctx: &egui::Context) {
        crate::ui_kit::install_fonts(ctx);
        crate::ui_kit::install_theme(ctx);
    }

    /// Steps until `label` shows, as the check runs on a worker.
    fn wait_for(harness: &mut egui_kittest::Harness, label: &str) {
        use egui_kittest::kittest::Queryable;
        for _ in 0..50 {
            harness.run_steps(1);
            if harness.query_by_label(label).is_some() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn runtime_status_with_claude_code_missing_counts_only_the_other_tools() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let mut checks = EnvironmentChecks::new(std::sync::Arc::new(Without("claude")), std::sync::Arc::new(|| {}));
        let view = WorkspaceView::default();
        let mut harness = Harness::builder().with_size([1000.0, 1400.0]).with_context_setup(setup).build_ui(move |ui| {
            let state = ScreenState { load: crate::ui_kit::Load::Ready, unread: &[], alerts: &|_| {} };
            checks.poll();
            runtime_status(ui, &state, &view, &mut checks, AuthMode::NoRunYet, false, "");
        });
        wait_for(&mut harness, "4 of 4 ready");
        harness.get_by_label("4 of 4 ready");
        harness.get_by_label("Missing");
        // With no folder, only Claude Code and the tools show.
        assert!(harness.query_by_label("Profile").is_none());
        assert!(harness.query_by_label("Sync").is_none());
    }

    #[test]
    fn runtime_status_shows_claude_code_and_the_missing_tool() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let mut checks = EnvironmentChecks::new(std::sync::Arc::new(Without("bun")), std::sync::Arc::new(|| {}));
        let profile = vec![crate::workspace::application::ProfileFile { name: "01-candidate-profile.md".into(), state: ProfileState::Filled }];
        let view = WorkspaceView { profile, ..WorkspaceView::default() };
        let mut harness = Harness::builder().with_size([1000.0, 1400.0]).with_context_setup(setup).build_ui(move |ui| {
            let state = ScreenState { load: crate::ui_kit::Load::Ready, unread: &[], alerts: &|_| {} };
            checks.poll();
            runtime_status(ui, &state, &view, &mut checks, AuthMode::ClaudeLogin, true, "");
        });
        wait_for(&mut harness, "3 of 4 ready");
        for label in ["On this computer", "Signed in with", "Claude sign-in", "Job site search: missing bun", "3 of 4 ready", "Check again", "Look for more in your documents…"] {
            assert!(harness.query_by_label(label).is_some(), "{label} is missing");
        }
        // Profile comes before Environment, as on the canvas.
        let top = |label: &str| harness.get_by_label(label).rect().min.y;
        assert!(top("Profile") < top("Environment"));
    }
}
