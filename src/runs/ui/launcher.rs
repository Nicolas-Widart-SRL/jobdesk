use std::collections::HashMap;

use egui::{ComboBox, Frame, Key, Margin, Modifiers, ScrollArea, TextEdit, Ui, vec2};

use crate::runs::application::{
    CAP_READING, CATALOG, CapCheck, CommandGroup, CommandSpec, FieldSpec, RunDefaults, RunDesk, StartOptions, StartValues, cap_line, cap_reason, compose_line,
    spec_for_line, start_values, task_start_values,
};
use crate::shared_kernel::folder_wait::UNLOCKABLE;
use crate::shared_kernel::visible_text::all_visible;
use crate::shared_kernel::{FolderWait, ModelChoice};
use crate::ui_kit::tokens::{FIELD_AREA_HEIGHT, FIELD_GAP, FIELD_HEIGHT, FIELD_PADDING, HEADER_PADDING, INSET, ROW_GAP};
use crate::ui_kit::{
    Ink, Subtitle, Title, Tone, Type, alert, code_box, command_chord, command_row, divider, enabled_when, from_note, group_title, hand, link, muted, page_body,
    page_head, panel, panel_body, primary_shortcut_button, rich, slip, split, strong,
};

/// The keys that start the run.
const RUN_KEYS: [&str; 2] = command_chord("Enter");

/// The form's width, as on the canvas.
const FORM_WIDTH: f32 = 400.0;

const NO_BUDGET: &str = "Enter a budget above zero.";
const BUSY: &str = "Another run is active. One run at a time keeps your files safe.";
const TICK_FIRST: &str = "Tick \"I understand\" first.";
/// Test connection runs Claude Code too, and one Claude process runs at a time.
const TESTING: &str = "Test connection is running. Run opens once it ends.";
/// A task would read or write the job search folder while Jobdesk changes it.
const FOLDER_UPDATING: &str = "Jobdesk is updating your job search folder. Run opens once it ends.";

/// The form's header for a task in plain words, which has no command name to show.
const WORDS_TITLE: &str = "Task in plain words";
const WORDS_NOTE: &str = "Jobdesk wrote this task. Claude reads it as your request.";
/// The page's subtitle over a task in plain words, which has no command to pick.
const WORDS_SUBTITLE: &str = "Read the task. Nothing runs until you press Run.";
const COMMAND_SUBTITLE: &str = "Pick a command. Nothing runs until you press Run.";

/// What a new task starts with, and whether the monthly cap lets one start.
pub struct LaunchRules<'a> {
    pub defaults: &'a RunDefaults,
    pub cap: &'a CapCheck,
    /// Settings' Test connection is running, so Run waits for it to end.
    pub testing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LauncherRequest {
    /// The id of the run the user just started.
    Started(String),
    /// Open Settings on Run defaults, where the cap is.
    OpenRunDefaults,
}

/// What opened the launcher, named in a note at the top of its form. It changes nothing that Run sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchNote {
    /// Find new jobs… on Today, before the board holds any job.
    FirstSearch,
}

impl LaunchNote {
    fn words(self) -> (&'static str, &'static str) {
        match self {
            Self::FirstSearch => ("Your first search, from Today.", "Leave the box below empty to search for all your job titles."),
        }
    }
}

/// The model and the budget hold only what the user changed. Untouched, they show what the run defaults give the command.
pub struct Launcher {
    command: &'static CommandSpec,
    /// A task in plain words, which a screen of Jobdesk wrote. While it is set, the form shows it in place of the command,
    /// read only, and Run sends it as it stands. Picking a command clears it.
    words: Option<String>,
    /// What opened the form, until another line fills it, a command is picked or Run starts it.
    note: Option<LaunchNote>,
    values: HashMap<&'static str, String>,
    budget: Option<String>,
    model: Option<ModelChoice>,
    reset_confirmed: bool,
}

impl Default for Launcher {
    fn default() -> Self {
        Self::for_command(&CATALOG[0])
    }
}

impl Launcher {
    fn for_command(command: &'static CommandSpec) -> Self {
        Self { command, words: None, note: None, values: HashMap::new(), budget: None, model: None, reset_confirmed: false }
    }

    /// `prefill` with a note naming what opened the form. A line the form refuses takes no note either.
    pub fn prefill_noted(&mut self, line: &str, note: LaunchNote) -> bool {
        let taken = self.prefill(line);
        if taken {
            self.note = Some(note);
        }
        taken
    }

    /// The note the form shows, if any.
    pub fn note(&self) -> Option<LaunchNote> {
        self.note
    }

    /// Drops the note, as the launcher opens with no line.
    pub fn drop_note(&mut self) {
        self.note = None;
    }

    /// Fills the form from a line such as `/outcome followup acme`. A field with a flag, such as `/setup --section goals`,
    /// takes only the value after its flag, because the line puts the flag back.
    /// A line that does not start with `/` is a task in plain words. Returns whether the form took the line:
    /// a `/` line the catalog does not hold changes nothing, and neither does a task with a character `visible_chars`
    /// drops, so the task shown is always the text Claude reads.
    pub fn prefill(&mut self, line: &str) -> bool {
        let line = line.trim();
        if !line.is_empty() && !line.starts_with('/') {
            if !all_visible(line) {
                return false;
            }
            *self = Self { words: Some(line.to_owned()), ..Self::default() };
            return true;
        }
        let Some(command) = spec_for_line(line) else { return false };
        *self = Self::for_command(command);
        let arguments = line[command.name.len()..].trim();
        if let (Some(field), false) = (command.fields.first(), arguments.is_empty()) {
            let after_flag = field.flag.and_then(|flag| arguments.strip_prefix(flag)).filter(|rest| rest.starts_with(char::is_whitespace));
            self.values.insert(field.key, after_flag.map_or(arguments, str::trim).to_owned());
        }
        true
    }

    fn value_of(&self, field: &FieldSpec) -> String {
        self.values.get(field.key).cloned().unwrap_or_else(|| field.default.to_owned())
    }

    /// The task in plain words the form holds, if any.
    pub fn words(&self) -> Option<&str> {
        self.words.as_deref()
    }

    /// Drops a task in plain words for a fresh form. A task lasts only until it runs or the launcher opens with no line,
    /// so Run never repeats it unseen. A command's form stays as the user left it.
    pub fn clear_words(&mut self) {
        if self.words.is_some() {
            *self = Self::default();
        }
    }

    /// What Run sends: the task in plain words, or the command line the form composes.
    pub(crate) fn line(&self) -> String {
        match self.words() {
            Some(words) => words.to_owned(),
            None => compose_line(self.command, |field| self.value_of(field)),
        }
    }

    /// A task in plain words starts from the defaults for every task, since no command's row names it.
    fn start_values(&self, defaults: &RunDefaults) -> StartValues {
        if self.words.is_some() { task_start_values(defaults) } else { start_values(defaults, self.command) }
    }

    fn fields(&self) -> &'static [FieldSpec] {
        if self.words.is_some() { &[] } else { self.command.fields }
    }

    fn destructive(&self) -> bool {
        self.words.is_none() && self.command.destructive
    }

    /// Returns the run the user just started, or the way to the cap.
    pub fn show(&mut self, ui: &mut Ui, desk: &mut RunDesk, rules: &LaunchRules) -> Option<LauncherRequest> {
        // Taken before any field runs, so a multiline field never reads the keys as a new line, even while Run waits.
        let run_keys = ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter));
        let subtitle = Subtitle::Text(if self.words.is_some() { WORDS_SUBTITLE } else { COMMAND_SUBTITLE });
        page_head(ui, Title::Slab("New task"), Some(subtitle), |_| {});
        page_body(ui, |ui| self.body(ui, desk, rules, run_keys))
    }

    fn body(&mut self, ui: &mut Ui, desk: &mut RunDesk, rules: &LaunchRules, run_keys: bool) -> Option<LauncherRequest> {
        split(ui, FORM_WIDTH, |main, side| {
            ScrollArea::vertical().auto_shrink([false, false]).show(main, |ui| panel(ui, |ui| self.list(ui)));
            slip(side, |ui| ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| self.form(ui, desk, rules, run_keys)).inner)
        })
    }

    /// Each group's heading stands on the hairline of its first row. No row is picked while the form holds a task in plain words.
    fn list(&mut self, ui: &mut Ui) {
        for group in CommandGroup::ALL {
            group_title(ui, group.label());
            for command in CATALOG.iter().filter(|command| command.group == group) {
                let on = self.words.is_none() && command.name == self.command.name;
                if command_row(ui, command.name, command.summary, command.writes, on).clicked() {
                    *self = Self::for_command(command);
                }
            }
        }
    }

    /// `run_keys` is Cmd or Ctrl Enter, which starts the run only when the button could.
    /// A reached cap holds back only a new task: a running one, and replies to a live session, go on elsewhere.
    fn form(&mut self, ui: &mut Ui, desk: &mut RunDesk, rules: &LaunchRules, run_keys: bool) -> Option<LauncherRequest> {
        let mut asked = None;
        let start = self.start_values(rules.defaults);
        Frame::new().inner_margin(Margin::symmetric(INSET, HEADER_PADDING)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match self.words {
                Some(_) => ui.label(rich(ui, WORDS_TITLE, Type::SLIP_TITLE, Ink::Text)),
                None => ui.label(rich(ui, self.command.name, Type::COMMAND, Ink::Text)),
            };
        });
        divider(ui);
        panel_body(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            if let Some((title, body)) = self.note.map(LaunchNote::words) {
                from_note(ui, title, body);
            }
            muted(ui, if self.words.is_some() { WORDS_NOTE } else { self.command.summary });
            for field in self.fields() {
                let mut value = self.value_of(field);
                let changed = labelled(ui, field.label, |ui, label| {
                    let edit = if field.multiline {
                        TextEdit::multiline(&mut value).desired_rows(3).min_size(vec2(0.0, FIELD_AREA_HEIGHT))
                    } else {
                        TextEdit::singleline(&mut value).min_size(vec2(0.0, FIELD_HEIGHT)).align(egui::Align2::LEFT_CENTER)
                    };
                    let edit = edit.margin(Margin::symmetric(FIELD_PADDING as i8, 4)).desired_width(f32::INFINITY);
                    ui.add(edit).labelled_by(label).changed()
                });
                if changed {
                    self.values.insert(field.key, value);
                }
            }
            let mut model = self.model.unwrap_or(start.model);
            let mut budget_text = self.budget.clone().unwrap_or_else(|| start.budget.field_text());
            ui.columns_const(|[model_column, budget_column]| {
                labelled(model_column, "Model", |ui, label| {
                    ui.spacing_mut().interact_size.y = FIELD_HEIGHT;
                    let width = ui.available_width();
                    let mut at = ModelChoice::ALL.iter().position(|&choice| choice == model).unwrap_or(0);
                    let combo = ComboBox::from_id_salt("run_model").width(width).selected_text(model.label()).show_index(
                        ui,
                        &mut at,
                        ModelChoice::ALL.len(),
                        |index| ModelChoice::ALL[index].label(),
                    );
                    if hand(combo).labelled_by(label).changed() {
                        model = ModelChoice::ALL[at];
                        self.model = Some(model);
                    }
                });
                labelled(budget_column, "Spend at most, USD", |ui, label| {
                    let edit = TextEdit::singleline(&mut budget_text).min_size(vec2(0.0, FIELD_HEIGHT)).align(egui::Align2::LEFT_CENTER);
                    if ui.add(edit.margin(Margin::symmetric(FIELD_PADDING as i8, 4)).desired_width(f32::INFINITY)).labelled_by(label).changed() {
                        self.budget = Some(budget_text.clone());
                    }
                });
            });
            let line = self.line();
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = FIELD_GAP;
                ui.label(rich(ui, "Claude will run", Type::SMALL, Ink::Muted));
                code_box(ui, |ui| ui.add(egui::Label::new(rich(ui, &line, Type::MONO, Ink::Text)).wrap()));
            });
            muted(ui, "Runs in your job search folder.");
            let destructive = self.destructive();
            if destructive {
                ui.label(rich(ui, "This deletes your data", Type::STRONG, Tone::Closed));
                muted(ui, "Files deleted from documents/ cannot be recovered by this app. The command also asks before it deletes anything.");
                hand(ui.checkbox(&mut self.reset_confirmed, "I understand"));
            }
            let budget = budget_text.trim().parse::<f64>().ok().filter(|budget| *budget > 0.0);
            let busy = desk.is_busy();
            let reached = match rules.cap {
                CapCheck::Reached(reached) => Some(reached),
                CapCheck::Open | CapCheck::Reading => None,
            };
            // Jobdesk updates the folder, or cannot lock it, and says which.
            let folder_wait = desk.folder_wait().map(|wait| match wait {
                FolderWait::Updating => FOLDER_UPDATING,
                FolderWait::Unlockable => UNLOCKABLE,
            });
            let updating = folder_wait.is_some();
            let ready = budget.is_some() && !busy && !rules.testing && !updating && *rules.cap == CapCheck::Open && (!destructive || self.reset_confirmed);
            if budget.is_none() {
                alert(ui, NO_BUDGET);
            }
            if busy {
                strong(ui, BUSY);
            } else if let Some(wait) = folder_wait {
                strong(ui, wait);
            } else if rules.testing {
                strong(ui, TESTING);
            }
            if let Some(reached) = reached {
                strong(ui, cap_line(reached));
            }
            let why = match rules.cap {
                _ if budget.is_none() => NO_BUDGET.to_owned(),
                _ if busy => BUSY.to_owned(),
                _ if updating => folder_wait.unwrap_or_default().to_owned(),
                _ if rules.testing => TESTING.to_owned(),
                CapCheck::Reached(reached) => cap_reason(reached),
                CapCheck::Reading => CAP_READING.to_owned(),
                CapCheck::Open => TICK_FIRST.to_owned(),
            };
            let mut run = |ui: &mut Ui| {
                let mut started = None;
                enabled_when(ui, ready, &why, |ui| {
                    let run = primary_shortcut_button(ui, "Run", &RUN_KEYS);
                    if run.clicked() || (ready && run_keys) {
                        let options = StartOptions { model: model.alias().map(str::to_owned), budget_usd: budget.unwrap_or_default() };
                        started = desk.start(&line, &options, chrono::Local::now().naive_local()).ok();
                    }
                    run
                });
                started.map(LauncherRequest::Started)
            };
            // The way to the cap stands beside Run only while the cap holds it back, so the usual form keeps its look.
            asked = if reached.is_some() {
                ui.horizontal(|ui| {
                    let started = run(ui);
                    if link(ui, "Change the cap in Settings").clicked() { Some(LauncherRequest::OpenRunDefaults) } else { started }
                })
                .inner
            } else {
                run(ui)
            };
        });
        if matches!(asked, Some(LauncherRequest::Started(_))) {
            self.clear_words();
            self.note = None;
        }
        asked
    }
}

/// A form field: its label above the input.
fn labelled<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui, egui::Id) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = FIELD_GAP;
        let label = ui.label(rich(ui, label, Type::NOTE, Ink::Muted));
        add(ui, label.id)
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flag in the line is the field's own, so it is not typed into the field a second time.
    #[test]
    fn a_flagged_line_fills_only_the_value() {
        let mut launcher = Launcher::default();
        launcher.prefill("/setup --section experience");
        assert_eq!(launcher.line(), "/setup --section experience");
        launcher.prefill("/outcome followup acme");
        assert_eq!(launcher.line(), "/outcome followup acme");
        launcher.prefill("/setup");
        assert_eq!(launcher.line(), "/setup");
    }

    /// A command with a plain field gets everything after its name, a flag-like word included.
    #[test]
    fn a_plain_name_fills_the_first_field() {
        let mut launcher = Launcher::default();
        launcher.prefill("/build-cv main_tessel_platform_lead");
        assert_eq!((launcher.command.name, launcher.line().as_str()), ("/build-cv", "/build-cv main_tessel_platform_lead"));
        launcher.prefill("/build-cv --all");
        assert_eq!(launcher.line(), "/build-cv --all");
        launcher.prefill("/build-cv");
        assert_eq!(launcher.line(), "/build-cv");
    }

    /// A line that does not start with `/` opens a task in plain words, trimmed, which Run sends as it stands.
    /// A command line after it opens the command again.
    #[test]
    fn a_line_in_plain_words_opens_a_task() {
        let mut launcher = Launcher::default();
        launcher.prefill("  Read the sample portal's code and tell me what it does.\n");
        assert_eq!(launcher.words.as_deref(), Some("Read the sample portal's code and tell me what it does."));
        assert_eq!(launcher.line(), "Read the sample portal's code and tell me what it does.");
        assert!(launcher.fields().is_empty() && !launcher.destructive());
        launcher.prefill("/setup --section experience");
        assert_eq!((launcher.words.as_deref(), launcher.line().as_str()), (None, "/setup --section experience"));
    }

    /// A note names what opened the form until another line fills it, and it changes nothing Run sends.
    #[test]
    fn a_note_lasts_until_another_line_fills_the_form() {
        let mut launcher = Launcher::default();
        assert!(launcher.prefill_noted("/scrape", LaunchNote::FirstSearch));
        assert_eq!((launcher.note(), launcher.line().as_str()), (Some(LaunchNote::FirstSearch), "/scrape"));
        launcher.prefill("/rank");
        assert_eq!(launcher.note(), None);
        assert!(!launcher.prefill_noted("/unknown", LaunchNote::FirstSearch), "a refused line takes no note");
        assert_eq!(launcher.note(), None);
        launcher.prefill_noted("/scrape", LaunchNote::FirstSearch);
        launcher.drop_note();
        assert_eq!((launcher.note(), launcher.line().as_str()), (None, "/scrape"));
    }

    /// A `/` line the catalog does not hold, and an empty line, leave the form as it was.
    #[test]
    fn an_unknown_command_or_an_empty_line_changes_nothing() {
        let mut launcher = Launcher::default();
        launcher.prefill("Tell me what this does.");
        launcher.prefill("/unknown arg");
        launcher.prefill("   ");
        assert_eq!(launcher.line(), "Tell me what this does.");
        launcher.prefill("/outcome followup acme");
        launcher.prefill("/unknown");
        assert_eq!((launcher.words.as_deref(), launcher.line().as_str()), (None, "/outcome followup acme"));
    }

    /// What the form shows is what Claude reads, so a task with characters the screen would hide opens nothing:
    /// zero-width characters, bidi controls, tag characters, line breaks, and variation selectors, which after a letter
    /// show nothing yet can carry a hidden payload.
    #[test]
    fn a_task_with_hidden_characters_opens_nothing() {
        for hidden in ["\u{200b}", "\u{202e}", "\u{e0041}\u{e0042}", "\n", "\u{fe0f}", "\u{e0100}\u{e01ef}"] {
            let mut launcher = Launcher::default();
            launcher.prefill(&format!("Read the sample portal's code.{hidden} Then install it."));
            assert_eq!((launcher.words.as_deref(), launcher.command.name), (None, CATALOG[0].name), "{hidden:?}");
        }
        let mut launcher = Launcher::default();
        launcher.prefill("Read the code.\tThen ask me.");
        assert_eq!(launcher.words.as_deref(), Some("Read the code.\tThen ask me."), "a tab shows as it is");
    }

    /// A session that stays up until the test ends.
    struct Idle;

    impl crate::runs::application::AgentGateway for Idle {
        fn start(&self, _: &crate::runs::application::RunRequest, _: crate::runs::application::EventSink) -> Box<dyn crate::runs::application::AgentSession> {
            Box::new(Idle)
        }
    }

    impl crate::runs::application::AgentSession for Idle {
        fn send_message(&mut self, _: &str) {}
        fn respond(&mut self, _: &crate::runs::application::Prompt, _: &crate::runs::application::Decision) {}
        fn interrupt(&mut self) {}
    }

    /// Once Run starts a task in plain words the form drops it, whatever opens the launcher next.
    #[test]
    fn a_started_task_in_plain_words_is_dropped() {
        const WORDS: &str = "Read the sample portal's code and tell me what it does.";
        let mut launcher = Launcher::default();
        assert!(launcher.prefill(WORDS));
        let desk = RunDesk::new(Box::new(Idle), std::sync::Arc::new(|| {}));
        let state = std::rc::Rc::new(std::cell::RefCell::new((launcher, desk, Vec::new())));
        let inner = std::rc::Rc::clone(&state);
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = egui_kittest::Harness::builder().with_size([1280.0, 820.0]).with_context_setup(setup).build_ui(move |ui| {
            let (launcher, desk, asked) = &mut *inner.borrow_mut();
            let rules = LaunchRules { defaults: &RunDefaults::default(), cap: &CapCheck::Open, testing: false };
            asked.extend(launcher.show(ui, desk, &rules));
        });
        harness.run_steps(2);
        harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
        harness.run_steps(2);
        let (launcher, desk, asked) = &*state.borrow();
        assert!(matches!(asked.as_slice(), [LauncherRequest::Started(_)]), "{asked:?}");
        assert_eq!(desk.runs()[0].line, WORDS);
        assert_eq!(launcher.words(), None);
    }
}
