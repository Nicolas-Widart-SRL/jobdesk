use chrono::NaiveDate;

use crate::runs::application::{CATALOG, CommandSpec, Period, UsageReport, WORDS_BUDGET_USD};
use crate::settings::application::usage::money;
use crate::settings::domain::{
    AutoCheck, CapReached, ComingUp, CommandDefault, Dollars, Drawn, KeyTest, LaterLasts, Page, PermissionMode, Preferences, PreferencesStore, ReasonRefusal,
    Retention, RunDefaults, SpendCap, StartScreen, StartValues, TextSize, ThemeChoice, drawn_index,
};
use crate::shared_kernel::{LoadError, ModelChoice, SignIn};

/// What went wrong with the settings file, which the rail says in its own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    /// It could not be read at launch, so the defaults are in use until the next change replaces it.
    Read,
    /// The last change could not be saved, so it lasts until Jobdesk closes.
    Save,
}

/// The user's preferences. Each change is saved at once, and a failure to read or save is kept to show.
pub struct Settings {
    store: Option<Box<dyn PreferencesStore>>,
    preferences: Preferences,
    issue: Option<(IssueKind, LoadError)>,
}

impl Settings {
    /// A file that cannot be read gives the defaults, and the next change replaces it.
    /// With no file yet, `first` is used and saved at once: what the window already restored, such as its theme and zoom.
    pub fn open(store: Box<dyn PreferencesStore>, first: Preferences) -> Self {
        let (preferences, issue) = match store.load() {
            Ok(Some(mut found)) => {
                // The file layer cannot see the catalog, so rows for names it does not hold are dropped here.
                found.run_defaults.commands.retain(|row| CATALOG.iter().any(|spec| spec.name == row.command));
                (found, None)
            }
            Ok(None) => {
                let issue = store.save(&first).err().map(|error| (IssueKind::Save, error));
                (first, issue)
            }
            Err(error) => (Preferences::default(), Some((IssueKind::Read, error))),
        };
        Self { store: Some(store), preferences, issue }
    }

    /// The defaults, kept nowhere. For a window that has no data folder.
    pub fn unsaved() -> Self {
        Self { store: None, preferences: Preferences::default(), issue: None }
    }

    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    pub fn issue(&self) -> Option<(IssueKind, &LoadError)> {
        self.issue.as_ref().map(|(kind, error)| (*kind, error))
    }

    pub fn set_theme(&mut self, theme: ThemeChoice) {
        self.change(|preferences| &mut preferences.theme, theme);
    }

    pub fn set_text_size(&mut self, text_size: TextSize) {
        self.change(|preferences| &mut preferences.text_size, text_size);
    }

    pub fn set_reduce_motion(&mut self, reduce_motion: bool) {
        self.change(|preferences| &mut preferences.reduce_motion, reduce_motion);
    }

    pub fn set_open_on(&mut self, open_on: StartScreen) {
        self.change(|preferences| &mut preferences.open_on, open_on);
    }

    pub fn set_model(&mut self, model: ModelChoice) {
        self.change(|preferences| &mut preferences.run_defaults.model, model);
    }

    pub fn set_budget(&mut self, budget: Option<Dollars>) {
        self.change(|preferences| &mut preferences.run_defaults.budget, budget);
    }

    /// Adds a row for the command, seeded with what a task of it starts with now. A command with a row gets no second one.
    pub fn add_command(&mut self, spec: &'static CommandSpec) {
        let defaults = &mut self.preferences.run_defaults;
        if defaults.commands.iter().any(|row| row.command == spec.name) {
            return;
        }
        let StartValues { model, budget } = defaults.resolve(spec.name, Dollars::whole(spec.budget_usd));
        defaults.commands.push(CommandDefault { command: spec.name.to_owned(), model, budget });
        self.save();
    }

    pub fn set_command_model(&mut self, command: &str, model: ModelChoice) {
        self.change_row(command, |row| &mut row.model, model);
    }

    pub fn set_command_budget(&mut self, command: &str, budget: Dollars) {
        self.change_row(command, |row| &mut row.budget, budget);
    }

    pub fn remove_command(&mut self, command: &str) {
        let commands = &mut self.preferences.run_defaults.commands;
        let Some(at) = commands.iter().position(|row| row.command == command) else { return };
        commands.remove(at);
        self.save();
    }

    pub fn set_cap_on(&mut self, on: bool) {
        self.change(|preferences| &mut preferences.cap.on, on);
    }

    pub fn set_cap_amount(&mut self, amount: Option<Dollars>) {
        self.change(|preferences| &mut preferences.cap.amount, amount);
    }

    pub fn set_permissions(&mut self, permissions: PermissionMode) {
        self.change(|preferences| &mut preferences.permissions, permissions);
    }

    pub fn set_retention(&mut self, retention: Retention) {
        self.change(|preferences| &mut preferences.retention, retention);
    }

    pub fn set_sign_in(&mut self, sign_in: SignIn) {
        self.change(|preferences| &mut preferences.sign_in, sign_in);
    }

    /// Switches the automatic check for a new Jobdesk on or off.
    pub fn set_check_updates(&mut self, on: bool) {
        self.change(|preferences| &mut preferences.check_updates, AutoCheck::from_on(on));
    }

    pub fn set_key_test(&mut self, key_test: Option<KeyTest>) {
        self.change(|preferences| &mut preferences.key_test, key_test);
    }

    pub fn set_coming_up(&mut self, coming_up: ComingUp) {
        self.change(|preferences| &mut preferences.queue.coming_up, coming_up);
    }

    pub fn set_later(&mut self, later: LaterLasts) {
        self.change(|preferences| &mut preferences.queue.later, later);
    }

    /// Adds a reason at the end of the list. A refused reason changes and saves nothing.
    pub fn add_skip_reason(&mut self, text: &str) -> Result<(), ReasonRefusal> {
        self.preferences.queue.skip_reasons.add(text)?;
        self.save();
        Ok(())
    }

    pub fn remove_skip_reason(&mut self, text: &str) {
        if self.preferences.queue.skip_reasons.remove(text) {
            self.save();
        }
    }

    pub fn set_hide_below_floor(&mut self, hide: bool) {
        self.change(|preferences| &mut preferences.jobs_view.below_floor, hide);
    }

    pub fn set_hide_too_far(&mut self, hide: bool) {
        self.change(|preferences| &mut preferences.jobs_view.too_far, hide);
    }

    pub fn set_hide_language(&mut self, hide: bool) {
        self.change(|preferences| &mut preferences.jobs_view.language, hide);
    }

    pub fn set_hide_flagged(&mut self, hide: bool) {
        self.change(|preferences| &mut preferences.jobs_view.flagged, hide);
    }

    pub fn set_hide_under_score(&mut self, hide: bool) {
        self.change(|preferences| &mut preferences.jobs_view.under_score, hide);
    }

    /// The score the "under" filter hides jobs below, kept between 0 and 100.
    pub fn set_score_floor(&mut self, score: u8) {
        self.change(|preferences| &mut preferences.jobs_view.score, score.min(100));
    }

    /// Saves only a changed value. The new value stays in use when the save fails.
    fn change<T: PartialEq>(&mut self, field: impl Fn(&mut Preferences) -> &mut T, value: T) {
        let slot = field(&mut self.preferences);
        if *slot == value {
            return;
        }
        *slot = value;
        self.save();
    }

    /// Like `change`, for one command's row. A command with no row changes nothing.
    fn change_row<T: PartialEq>(&mut self, command: &str, field: impl Fn(&mut CommandDefault) -> &mut T, value: T) {
        let Some(row) = self.preferences.run_defaults.commands.iter_mut().find(|row| row.command == command) else { return };
        let slot = field(row);
        if *slot == value {
            return;
        }
        *slot = value;
        self.save();
    }

    fn save(&mut self) {
        let Some(store) = &self.store else { return };
        self.issue = store.save(&self.preferences).err().map(|error| (IssueKind::Save, error));
    }
}

/// What a task of the command starts with: its row, else the defaults for every task, else the catalog's suggestion.
pub fn start_values(defaults: &RunDefaults, command: &CommandSpec) -> StartValues {
    defaults.resolve(command.name, Dollars::whole(command.budget_usd))
}

/// What a task in plain words starts with: the defaults for every task, else its own suggestion.
/// No command's row applies, because the task is no command.
pub fn task_start_values(defaults: &RunDefaults) -> StartValues {
    defaults.resolve("", Dollars::whole(WORDS_BUDGET_USD))
}

/// The commands that have no row yet, in catalog order.
pub fn addable_commands(defaults: &RunDefaults) -> Vec<&'static CommandSpec> {
    CATALOG.iter().filter(|spec| !defaults.commands.iter().any(|row| row.command == spec.name)).collect()
}

/// Whether a new task may start this month.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CapCheck {
    /// No cap in force, or this month is under it.
    Open,
    /// A cap is in force and the run logs are not read yet, so the month's spend is not known.
    Reading,
    Reached(CapReached),
}

/// Checks the cap against what this month's tasks cost, summed as Usage statistics sums them.
/// A resumed task counts on the day it started, so a task that began last month adds nothing to this one.
/// `usage` is None while the run logs are still being read. With no cap in force there is nothing to wait for.
pub fn cap_check(cap: &SpendCap, usage: Option<&UsageReport>, today: NaiveDate) -> CapCheck {
    if cap.active().is_none() {
        return CapCheck::Open;
    }
    let Some(report) = usage else { return CapCheck::Reading };
    match cap.reached(report.totals(Period::Month, today).spent_usd, today) {
        Some(reached) => CapCheck::Reached(reached),
        None => CapCheck::Open,
    }
}

/// The line the launcher shows above Run when the cap holds new tasks back.
pub fn cap_line(reached: &CapReached) -> String {
    format!("This month's tasks reached your {} cap.", money(reached.cap.usd()))
}

/// Why Run is off, which says how much was spent and when the month turns.
/// A no-break space holds the day to its month, so a wrapped tooltip never leaves "1" alone at a line's end.
pub fn cap_reason(reached: &CapReached) -> String {
    let cap = money(reached.cap.usd());
    let spent = money(reached.spent_usd);
    let opens = reached.reopens_on.format("%-d\u{a0}%B");
    if reached.over() {
        format!("This month's tasks cost {spent}, over your {cap} cap. Run opens again on {opens}.")
    } else {
        format!("This month's tasks cost {spent}, your whole {cap} cap. Run opens again on {opens}.")
    }
}

/// Why Run waits while a cap is in force and the month's spend is not read yet.
pub const CAP_READING: &str = "Run waits while this month's spend is read.";

/// The rail's word for a cap in force: `Cap $60`, or `Cap $60.50` when it has cents.
pub fn cap_word(cap: Dollars) -> String {
    let amount = money(cap.usd());
    format!("Cap {}", amount.strip_suffix(".00").unwrap_or(&amount))
}

/// A size in decimal units for a line of detail: `999 bytes`, `18 KB`, `211.8 MB`, `1.5 GB`.
pub fn size_detail(bytes: u64) -> String {
    sized(bytes, true)
}

/// A size in whole units for a short word: `212 MB`, `2 GB`.
pub fn size_short(bytes: u64) -> String {
    sized(bytes, false)
}

/// Kilobytes are always whole. Megabytes and gigabytes keep a decimal in the detail.
/// A size that rounds up to the next unit's first step moves there: 999,999 bytes is `1.0 MB`, not `1000 KB`.
fn sized(bytes: u64, detail: bool) -> String {
    if bytes < 1000 {
        return if bytes == 1 { "1 byte".to_owned() } else { format!("{bytes} bytes") };
    }
    let mut value = bytes as f64 / 1000.0;
    for unit in ["KB", "MB", "GB"] {
        let decimals = usize::from(detail && unit != "KB");
        let text = format!("{value:.decimals$}");
        if unit == "GB" || text.parse::<f64>().is_ok_and(|rounded| rounded < 1000.0) {
            return format!("{text} {unit}");
        }
        value /= 1000.0;
    }
    unreachable!("the last unit always returns")
}

/// What the tool check knows of Claude Code's version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeVersion<'a> {
    /// The first check has not answered yet.
    Checking,
    Found(&'a str),
    NotFound,
}

/// What About's Copy details puts on the clipboard for a bug report: Jobdesk's version, the system it runs on, and
/// Claude Code's version, that it was not found, or that the check has not answered yet.
pub fn about_details(jobdesk: &str, system: &str, arch: &str, claude: ClaudeVersion) -> String {
    let claude = match claude {
        ClaudeVersion::Found(version) => version,
        ClaudeVersion::NotFound => "not found",
        ClaudeVersion::Checking => "not checked yet",
    };
    format!("Jobdesk {jobdesk}\n{system} ({arch})\nClaude Code {claude}")
}

/// The question Clear tray decisions asks, with how many it forgets.
pub fn clear_question(decisions: usize) -> String {
    if decisions == 1 { "Clear 1 tray decision?".to_owned() } else { format!("Clear {decisions} tray decisions?") }
}

/// The pages a search matches, each with how many of its lines match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHits {
    pub pages: Vec<(Page, usize)>,
    pub total: usize,
}

/// `None` for a blank query. Case does not matter, and every line the page draws counts once.
/// `drawn` says what the pages draw as things stand, such as Runtime status before the folder is read.
pub fn search(query: &str, drawn: Drawn) -> Option<SearchHits> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let pages: Vec<(Page, usize)> = Page::ALL
        .iter()
        .map(|&page| (page, drawn_index(page, drawn).iter().filter(|line| line.to_lowercase().contains(&needle)).count()))
        .filter(|&(_, count)| count > 0)
        .collect();
    let total = pages.iter().map(|&(_, count)| count).sum();
    Some(SearchHits { pages, total })
}

/// The one word the rail shows beside Runtime status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeWord {
    None,
    Missing(usize),
    ToFill(usize),
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;
    use crate::settings::domain::{PartDrawn, TermsDrawn};

    #[derive(Default)]
    struct Shared {
        saved: RefCell<Vec<Preferences>>,
        load: RefCell<Option<Result<Option<Preferences>, LoadError>>>,
        fail_save: Cell<bool>,
    }

    struct Fake(Rc<Shared>);

    impl PreferencesStore for Fake {
        fn load(&self) -> Result<Option<Preferences>, LoadError> {
            self.0.load.borrow_mut().take().unwrap_or(Ok(None))
        }

        fn save(&self, preferences: &Preferences) -> Result<(), LoadError> {
            if self.0.fail_save.get() {
                return Err(LoadError::new("settings.json", "disk full"));
            }
            self.0.saved.borrow_mut().push(preferences.clone());
            Ok(())
        }
    }

    /// Opened over a file that holds the defaults, so the first save is a change's.
    fn settings(shared: &Rc<Shared>) -> Settings {
        if shared.load.borrow().is_none() {
            *shared.load.borrow_mut() = Some(Ok(Some(Preferences::default())));
        }
        Settings::open(Box::new(Fake(shared.clone())), Preferences::default())
    }

    #[test]
    fn nothing_saved_takes_and_saves_what_the_window_restored() {
        let shared = Rc::new(Shared::default());
        let restored = Preferences { theme: ThemeChoice::Dark, text_size: TextSize::from_percent(125), ..Preferences::default() };
        let settings = Settings::open(Box::new(Fake(shared.clone())), restored.clone());
        assert_eq!(*settings.preferences(), restored);
        assert!(settings.issue().is_none());
        assert_eq!(*shared.saved.borrow(), vec![restored]);
    }

    #[test]
    fn a_seed_that_cannot_be_saved_is_a_save_issue() {
        let shared = Rc::new(Shared::default());
        shared.fail_save.set(true);
        let settings = Settings::open(Box::new(Fake(shared.clone())), Preferences::default());
        assert_eq!(settings.issue().map(|(kind, _)| kind), Some(IssueKind::Save));
    }

    #[test]
    fn saved_preferences_come_back() {
        let shared = Rc::new(Shared::default());
        let stored = Preferences {
            theme: ThemeChoice::Dark,
            text_size: TextSize::from_percent(125),
            reduce_motion: true,
            open_on: StartScreen::Jobs,
            retention: Retention::Days30,
            ..Preferences::default()
        };
        *shared.load.borrow_mut() = Some(Ok(Some(stored.clone())));
        assert_eq!(*settings(&shared).preferences(), stored);
    }

    #[test]
    fn the_same_value_twice_saves_once() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        settings.set_theme(ThemeChoice::Dark);
        settings.set_theme(ThemeChoice::Dark);
        settings.set_text_size(TextSize::default());
        assert_eq!(shared.saved.borrow().len(), 1);
        assert_eq!(shared.saved.borrow()[0].theme, ThemeChoice::Dark);
    }

    #[test]
    fn the_sign_in_and_key_test_save_once_and_only_on_a_change() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        settings.set_sign_in(SignIn::ClaudeSignIn);
        settings.set_key_test(None);
        assert!(shared.saved.borrow().is_empty(), "the defaults again change nothing");
        settings.set_sign_in(SignIn::ApiKey);
        settings.set_sign_in(SignIn::ApiKey);
        assert_eq!(shared.saved.borrow().len(), 1);
        let test = KeyTest { passed: true, at: NaiveDate::from_ymd_opt(2026, 3, 9).unwrap().and_hms_opt(14, 3, 0).unwrap(), cost_usd: 0.04 };
        settings.set_key_test(Some(test));
        settings.set_key_test(Some(test));
        assert_eq!(shared.saved.borrow().len(), 2);
        settings.set_key_test(Some(KeyTest { passed: false, ..test }));
        settings.set_key_test(None);
        let saved = shared.saved.borrow();
        assert_eq!(saved.len(), 4);
        assert_eq!((saved[1].sign_in, saved[1].key_test), (SignIn::ApiKey, Some(test)));
        assert_eq!((saved[3].sign_in, saved[3].key_test), (SignIn::ApiKey, None));
    }

    #[test]
    fn the_queue_and_jobs_view_setters_save_once_and_only_on_a_change() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        settings.set_coming_up(ComingUp::TwoWeeks);
        settings.set_later(LaterLasts::Tomorrow);
        settings.set_hide_below_floor(false);
        settings.set_hide_too_far(false);
        settings.set_hide_language(false);
        settings.set_hide_flagged(false);
        settings.set_hide_under_score(false);
        settings.set_score_floor(60);
        assert!(shared.saved.borrow().is_empty(), "the defaults again change nothing");

        settings.set_coming_up(ComingUp::Month);
        settings.set_coming_up(ComingUp::Month);
        settings.set_later(LaterLasts::UntilMonday);
        settings.set_later(LaterLasts::UntilMonday);
        settings.set_hide_below_floor(true);
        settings.set_hide_below_floor(true);
        settings.set_hide_too_far(true);
        settings.set_hide_too_far(true);
        settings.set_hide_language(true);
        settings.set_hide_language(true);
        settings.set_hide_flagged(true);
        settings.set_hide_flagged(true);
        settings.set_hide_under_score(true);
        settings.set_hide_under_score(true);
        settings.set_score_floor(75);
        settings.set_score_floor(75);
        assert_eq!(shared.saved.borrow().len(), 8);
        let saved = shared.saved.borrow();
        let last = saved.last().unwrap();
        assert_eq!((last.queue.coming_up, last.queue.later), (ComingUp::Month, LaterLasts::UntilMonday));
        let view = last.jobs_view;
        assert_eq!((view.below_floor, view.too_far, view.language, view.flagged, view.under_score, view.score), (true, true, true, true, true, 75));
    }

    #[test]
    fn the_score_floor_stays_between_0_and_100() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        settings.set_score_floor(250);
        assert_eq!(settings.preferences().jobs_view.score, 100);
        settings.set_score_floor(100);
        settings.set_score_floor(0);
        assert_eq!(settings.preferences().jobs_view.score, 0);
        assert_eq!(shared.saved.borrow().len(), 2);
    }

    #[test]
    fn a_skip_reason_saves_once_and_a_refused_one_saves_nothing() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        assert_eq!(settings.add_skip_reason("  Culture "), Ok(()));
        assert_eq!(shared.saved.borrow().len(), 1);
        assert_eq!(settings.add_skip_reason("culture"), Err(ReasonRefusal::Taken));
        assert_eq!(settings.add_skip_reason("   "), Err(ReasonRefusal::Empty));
        assert_eq!(settings.add_skip_reason(&"x".repeat(41)), Err(ReasonRefusal::TooLong));
        assert_eq!(shared.saved.borrow().len(), 1);
        settings.remove_skip_reason("Nothing like it");
        assert_eq!(shared.saved.borrow().len(), 1);
        settings.remove_skip_reason("rate");
        assert_eq!(shared.saved.borrow().len(), 2);
        assert_eq!(settings.preferences().queue.skip_reasons.list(), ["Scope", "Domain", "Location", "Culture"]);
        assert_eq!(shared.saved.borrow()[1].queue.skip_reasons.list(), settings.preferences().queue.skip_reasons.list());
        for n in 0..4 {
            settings.add_skip_reason(&format!("Extra {n}")).unwrap();
        }
        let saves = shared.saved.borrow().len();
        assert_eq!(settings.add_skip_reason("One more"), Err(ReasonRefusal::Full));
        assert_eq!(shared.saved.borrow().len(), saves);
    }

    #[test]
    fn every_setter_saves_its_value() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        settings.set_text_size(TextSize::from_percent(150));
        settings.set_reduce_motion(true);
        settings.set_open_on(StartScreen::Tasks);
        let saved = shared.saved.borrow();
        assert_eq!(saved.len(), 3);
        let last = &saved[2];
        assert_eq!((last.text_size.percent(), last.reduce_motion, last.open_on), (150, true, StartScreen::Tasks));
    }

    #[test]
    fn the_update_switch_saves_only_a_change() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        let before = shared.saved.borrow().len();
        settings.set_check_updates(true);
        assert_eq!(shared.saved.borrow().len(), before, "on is what it already was");
        settings.set_check_updates(false);
        assert_eq!(settings.preferences().check_updates, AutoCheck::Off);
        assert_eq!(shared.saved.borrow().last().map(|saved| saved.check_updates), Some(AutoCheck::Off));
        settings.set_check_updates(true);
        assert_eq!(settings.preferences().check_updates, AutoCheck::On);
    }

    #[test]
    fn a_read_error_gives_the_defaults_and_a_read_issue_until_a_change_saves() {
        let shared = Rc::new(Shared::default());
        *shared.load.borrow_mut() = Some(Err(LoadError::new("settings.json", "bad")));
        let restored = Preferences { theme: ThemeChoice::Dark, ..Preferences::default() };
        let mut settings = Settings::open(Box::new(Fake(shared.clone())), restored);
        assert_eq!(*settings.preferences(), Preferences::default(), "an unreadable file is not a missing one");
        assert!(shared.saved.borrow().is_empty(), "the file is left as it is until a change");
        assert_eq!(settings.issue().map(|(kind, issue)| (kind, issue.source.as_str())), Some((IssueKind::Read, "settings.json")));
        settings.set_theme(ThemeChoice::Light);
        assert!(settings.issue().is_none(), "the change replaced the file");
    }

    #[test]
    fn a_save_error_keeps_the_value_and_records_a_save_issue() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        shared.fail_save.set(true);
        settings.set_reduce_motion(true);
        assert!(settings.preferences().reduce_motion);
        assert_eq!(settings.issue().map(|(kind, _)| kind), Some(IssueKind::Save));
        shared.fail_save.set(false);
        settings.set_open_on(StartScreen::Insights);
        assert!(settings.issue().is_none());
    }

    #[test]
    fn unsaved_settings_change_without_a_store() {
        let mut settings = Settings::unsaved();
        settings.set_theme(ThemeChoice::Light);
        assert_eq!(settings.preferences().theme, ThemeChoice::Light);
        assert!(settings.issue().is_none());
    }

    #[test]
    fn a_search_counts_lines_per_page() {
        let hits = search("FOLDER", Drawn::folder(true)).unwrap();
        assert_eq!(hits.pages, vec![(Page::General, 3), (Page::Defaults, 1), (Page::Permissions, 2), (Page::Runtime, 1), (Page::Data, 4)]);
        assert_eq!(hits.total, 11);
        let spend = search("spend", Drawn::folder(true)).unwrap();
        assert_eq!(spend.pages, vec![(Page::Defaults, 1)]);
    }

    /// Run defaults says what an empty budget falls back to, so the words must follow the catalog.
    #[test]
    fn the_suggested_budgets_run_from_one_to_twenty_five_dollars() {
        let budgets = CATALOG.iter().map(|spec| spec.budget_usd);
        assert_eq!((budgets.clone().min(), budgets.max()), (Some(1), Some(25)));
        assert!(crate::settings::domain::index(Page::Defaults).iter().any(|line| line.contains("its suggested budget, $1 to $25.")));
    }

    #[test]
    fn the_details_name_both_versions_and_the_system() {
        assert_eq!(about_details("0.1.0", "macOS", "aarch64", ClaudeVersion::Found("2.1.4")), "Jobdesk 0.1.0\nmacOS (aarch64)\nClaude Code 2.1.4");
        assert_eq!(about_details("0.1.0", "Windows", "x86_64", ClaudeVersion::NotFound), "Jobdesk 0.1.0\nWindows (x86_64)\nClaude Code not found");
        assert_eq!(
            about_details("0.1.0", "macOS", "aarch64", ClaudeVersion::Checking),
            "Jobdesk 0.1.0\nmacOS (aarch64)\nClaude Code not checked yet",
            "a check still running never claims Claude Code is missing"
        );
    }

    #[test]
    fn the_clear_question_counts_the_decisions() {
        assert_eq!(clear_question(214), "Clear 214 tray decisions?");
        assert_eq!(clear_question(1), "Clear 1 tray decision?");
    }

    #[test]
    fn without_a_folder_runtime_status_matches_only_what_it_draws() {
        assert_eq!(search("sync", Drawn::folder(true)).unwrap().pages, vec![(Page::Runtime, 1)]);
        assert!(search("sync", Drawn::folder(false)).unwrap().pages.is_empty());
        assert_eq!(search("environment", Drawn::folder(false)).unwrap().pages, vec![(Page::Runtime, 1)]);
    }

    #[test]
    fn a_search_finds_the_job_search_pages() {
        assert_eq!(search("skip reasons", Drawn::folder(true)).unwrap().pages, vec![(Page::Queue, 1)], "Clear tray decisions leaves the list of reasons alone");
        assert_eq!(search("past skips", Drawn::folder(true)).unwrap().pages, vec![(Page::Queue, 1), (Page::Data, 1)]);
        assert_eq!(search("Hide jobs scored under", Drawn::folder(true)).unwrap().pages, vec![(Page::JobsView, 1)]);
        assert_eq!(search("commute", Drawn::folder(true)).unwrap().pages, vec![(Page::Terms, 2), (Page::JobsView, 1)]);
        assert_eq!(search("commute", Drawn::folder(false)).unwrap().pages, vec![(Page::JobsView, 1)], "without a folder, Your terms draws no rows");
    }

    fn terms_drawn(rate: PartDrawn, commute: PartDrawn) -> Drawn {
        Drawn { folder_read: true, terms: TermsDrawn { rate, commute }, auto_check: true }
    }

    /// About's switch has a help for on and one for off, and a search matches only the one it shows.
    #[test]
    fn a_search_finds_the_help_the_update_switch_shows() {
        let off = Drawn { auto_check: false, ..Drawn::folder(true) };
        let pages = |query, drawn| search(query, drawn).unwrap().pages;
        assert_eq!(pages("6 hours", Drawn::folder(true)), vec![(Page::About, 1)]);
        assert!(pages("6 hours", off).is_empty(), "off, the help says nothing of it");
        assert_eq!(pages("GitHub", off), vec![(Page::Data, 1)], "off, only Data and privacy's line names GitHub");
        assert_eq!(pages("only when you choose", off), vec![(Page::About, 1)]);
        assert!(pages("only when you choose", Drawn::folder(true)).is_empty());
        assert_eq!(pages("updates", off), vec![(Page::About, 2)], "the section and the switch, either way");
    }

    #[test]
    fn a_search_skips_the_terms_a_failed_file_hides() {
        assert!(search("contract", terms_drawn(PartDrawn::Hidden, PartDrawn::Stated)).unwrap().pages.is_empty(), "the profile failed");
        assert_eq!(search("commute", terms_drawn(PartDrawn::Stated, PartDrawn::Hidden)).unwrap().pages, vec![(Page::JobsView, 1)], "the evaluation failed");
        assert_eq!(search("contract", terms_drawn(PartDrawn::Stated, PartDrawn::Hidden)).unwrap().pages, vec![(Page::Terms, 3)]);
    }

    #[test]
    fn a_search_finds_the_fixed_terms_lines_only_where_the_page_draws_them() {
        let pages = |query, drawn| search(query, drawn).unwrap().pages;
        assert_eq!(pages("lower day rate", Drawn::folder(true)), vec![(Page::Terms, 1)]);
        assert!(pages("lower day rate", terms_drawn(PartDrawn::NotStated, PartDrawn::Stated)).is_empty(), "no floor, so its help is not drawn");
        assert!(pages("reads Unclear", Drawn::folder(true)).is_empty());
        assert_eq!(pages("reads Unclear", terms_drawn(PartDrawn::NotStated, PartDrawn::Stated)), vec![(Page::Terms, 1)]);
        assert_eq!(pages("reads Unclear", terms_drawn(PartDrawn::NotStated, PartDrawn::NotStated)), vec![(Page::Terms, 2)]);
        assert_eq!(pages("commute tiers", terms_drawn(PartDrawn::Stated, PartDrawn::NotStated)), vec![(Page::Terms, 2)], "the note and the row");
        assert_eq!(pages("commute tiers", Drawn::folder(true)), vec![(Page::Terms, 1)], "the note alone");
    }

    #[test]
    fn a_blank_search_is_none_and_a_miss_has_no_pages() {
        assert_eq!(search("  ", Drawn::folder(true)), None);
        assert_eq!(search("", Drawn::folder(true)), None);
        let miss = search("zzz", Drawn::folder(true)).unwrap();
        assert!(miss.pages.is_empty());
        assert_eq!(miss.total, 0);
    }

    #[test]
    fn a_search_trims_the_query() {
        assert_eq!(search("  motion ", Drawn::folder(true)).unwrap().pages, vec![(Page::Appearance, 2)]);
    }

    fn spec(name: &str) -> &'static CommandSpec {
        CATALOG.iter().find(|spec| spec.name == name).unwrap()
    }

    fn dollars(text: &str) -> Dollars {
        Dollars::parse(text).unwrap()
    }

    fn saves(shared: &Rc<Shared>) -> usize {
        shared.saved.borrow().len()
    }

    #[test]
    fn every_new_setter_saves_once_and_only_on_a_change() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        let mut expect = 0;
        let mut once = |settings: &mut Settings, change: &dyn Fn(&mut Settings)| {
            change(settings);
            expect += 1;
            assert_eq!(saves(&shared), expect);
            change(settings);
            assert_eq!(saves(&shared), expect, "the same value again saves nothing");
        };
        once(&mut settings, &|s| s.set_model(ModelChoice::Opus));
        once(&mut settings, &|s| s.set_budget(Some(dollars("4"))));
        once(&mut settings, &|s| s.add_command(spec("/rank")));
        once(&mut settings, &|s| s.set_command_model("/rank", ModelChoice::Haiku));
        once(&mut settings, &|s| s.set_command_budget("/rank", dollars("1.5")));
        once(&mut settings, &|s| s.set_cap_on(true));
        once(&mut settings, &|s| s.set_cap_amount(Some(dollars("60"))));
        once(&mut settings, &|s| s.set_permissions(PermissionMode::Ask));
        once(&mut settings, &|s| s.set_retention(Retention::Days90));
        let last = shared.saved.borrow().last().cloned().unwrap();
        assert_eq!(last, *settings.preferences());
        assert_eq!(last.run_defaults.commands, vec![CommandDefault { command: "/rank".to_owned(), model: ModelChoice::Haiku, budget: dollars("1.50") }]);
        assert_eq!((last.cap.active(), last.permissions, last.retention), (Some(dollars("60")), PermissionMode::Ask, Retention::Days90));
        settings.remove_command("/rank");
        assert_eq!(saves(&shared), expect + 1);
        settings.remove_command("/rank");
        assert_eq!(saves(&shared), expect + 1, "a command with no row removes nothing");
        assert!(settings.preferences().run_defaults.commands.is_empty());
    }

    #[test]
    fn a_command_row_that_is_not_there_changes_nothing() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        settings.set_command_model("/rank", ModelChoice::Opus);
        settings.set_command_budget("/rank", dollars("9"));
        assert_eq!(saves(&shared), 0);
        assert!(settings.preferences().run_defaults.commands.is_empty());
    }

    #[test]
    fn add_command_seeds_from_the_current_start_values_and_refuses_a_duplicate() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        settings.add_command(spec("/scrape"));
        assert_eq!(
            settings.preferences().run_defaults.commands,
            vec![CommandDefault { command: "/scrape".to_owned(), model: ModelChoice::Default, budget: dollars("2") }]
        );
        settings.set_model(ModelChoice::Sonnet);
        settings.set_budget(Some(dollars("4")));
        settings.add_command(spec("/rank"));
        let rank = settings.preferences().run_defaults.commands[1].clone();
        assert_eq!((rank.command.as_str(), rank.model, rank.budget), ("/rank", ModelChoice::Sonnet, dollars("4")), "seeded with the every-task values");
        let before = saves(&shared);
        settings.add_command(spec("/scrape"));
        assert_eq!(settings.preferences().run_defaults.commands.len(), 2);
        assert_eq!(saves(&shared), before, "a duplicate saves nothing");
    }

    #[test]
    fn a_loaded_row_keeps_only_a_name_the_catalog_holds() {
        let shared = Rc::new(Shared::default());
        let row = |command: &str| CommandDefault { command: command.to_owned(), model: ModelChoice::Opus, budget: dollars("2") };
        let stored = Preferences {
            run_defaults: RunDefaults {
                commands: vec![
                    row("/rank"),
                    row("/rank "),
                    row("/ranking"),
                    row("/ra\u{1b}nk"),
                    row("/rank\n"),
                    row("/RANK"),
                    row("/nonsense"),
                    row("/scrape"),
                ],
                ..RunDefaults::default()
            },
            ..Preferences::default()
        };
        *shared.load.borrow_mut() = Some(Ok(Some(stored)));
        let settings = Settings::open(Box::new(Fake(shared.clone())), Preferences::default());
        let kept: Vec<_> = settings.preferences().run_defaults.commands.iter().map(|row| row.command.as_str()).collect();
        assert_eq!(kept, ["/rank", "/scrape"]);
    }

    #[test]
    fn an_unsaved_window_still_takes_the_new_settings() {
        let mut settings = Settings::unsaved();
        settings.set_retention(Retention::Days30);
        settings.add_command(spec("/rank"));
        settings.set_permissions(PermissionMode::Ask);
        assert_eq!(settings.preferences().retention, Retention::Days30);
        assert_eq!(settings.preferences().run_defaults.commands.len(), 1);
        assert!(settings.issue().is_none());
    }

    #[test]
    fn a_save_error_keeps_a_new_setting_in_use() {
        let shared = Rc::new(Shared::default());
        let mut settings = settings(&shared);
        shared.fail_save.set(true);
        settings.set_cap_on(true);
        assert!(settings.preferences().cap.on);
        assert_eq!(settings.issue().map(|(kind, _)| kind), Some(IssueKind::Save));
    }

    #[test]
    fn start_values_follow_the_defaults_then_the_catalog() {
        let rank = spec("/rank");
        let scrape = spec("/scrape");
        let none = RunDefaults::default();
        assert_eq!(start_values(&none, rank), StartValues { model: ModelChoice::Default, budget: Dollars::whole(3) });
        let defaults = RunDefaults {
            model: ModelChoice::Opus,
            budget: Some(dollars("4")),
            commands: vec![CommandDefault { command: "/rank".to_owned(), model: ModelChoice::Haiku, budget: dollars("1") }],
        };
        assert_eq!(start_values(&defaults, rank), StartValues { model: ModelChoice::Haiku, budget: dollars("1") });
        assert_eq!(start_values(&defaults, scrape), StartValues { model: ModelChoice::Opus, budget: dollars("4") });
    }

    #[test]
    fn a_task_in_plain_words_starts_from_the_defaults_for_every_task() {
        let none = RunDefaults::default();
        assert_eq!(task_start_values(&none), StartValues { model: ModelChoice::Default, budget: Dollars::whole(WORDS_BUDGET_USD) });
        let defaults = RunDefaults {
            model: ModelChoice::Opus,
            budget: Some(dollars("4")),
            commands: vec![CommandDefault { command: "/rank".to_owned(), model: ModelChoice::Haiku, budget: dollars("1") }],
        };
        assert_eq!(task_start_values(&defaults), StartValues { model: ModelChoice::Opus, budget: dollars("4") });
    }

    #[test]
    fn addable_commands_keep_the_catalog_order_minus_the_rows() {
        let all: Vec<_> = addable_commands(&RunDefaults::default()).iter().map(|spec| spec.name).collect();
        assert_eq!(all, CATALOG.iter().map(|spec| spec.name).collect::<Vec<_>>());
        let defaults = RunDefaults {
            commands: vec![CommandDefault { command: "/rank".to_owned(), model: ModelChoice::Default, budget: dollars("1") }],
            ..RunDefaults::default()
        };
        let left: Vec<_> = addable_commands(&defaults).iter().map(|spec| spec.name).collect();
        assert_eq!(left.len(), CATALOG.len() - 1);
        assert!(!left.contains(&"/rank"));
        assert_eq!(left[0], "/scrape");
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn task(on: NaiveDate, cost_usd: f64) -> crate::runs::application::TaskCost {
        crate::runs::application::TaskCost {
            started_on: on,
            command: Some("/rank".to_owned()),
            outcome: crate::runs::application::TaskOutcome::Finished,
            cost_usd,
            models: vec![],
        }
    }

    fn cap_of(amount: &str) -> SpendCap {
        SpendCap { on: true, amount: Some(dollars(amount)) }
    }

    #[test]
    fn a_cap_waits_for_the_logs_and_opens_on_a_cap_that_is_off() {
        let today = day(2026, 10, 31);
        assert_eq!(cap_check(&cap_of("60"), None, today), CapCheck::Reading);
        assert_eq!(cap_check(&SpendCap::default(), None, today), CapCheck::Open, "no cap, nothing to wait for");
        assert_eq!(cap_check(&SpendCap { on: true, amount: None }, None, today), CapCheck::Open);
        let report = UsageReport::new(vec![task(day(2026, 10, 3), 500.0)]);
        assert_eq!(cap_check(&SpendCap { on: false, amount: Some(dollars("60")) }, Some(&report), today), CapCheck::Open);
    }

    #[test]
    fn a_cap_sums_this_month_only() {
        let report = UsageReport::new(vec![task(day(2026, 10, 3), 40.0), task(day(2026, 10, 30), 20.4), task(day(2026, 9, 30), 90.0)]);
        let CapCheck::Reached(reached) = cap_check(&cap_of("60"), Some(&report), day(2026, 10, 31)) else { panic!("over the cap") };
        assert!((reached.spent_usd - 60.4).abs() < 1e-9);
        assert_eq!(reached.reopens_on, day(2026, 11, 1));
        assert_eq!(cap_check(&cap_of("61"), Some(&report), day(2026, 10, 31)), CapCheck::Open);
    }

    #[test]
    fn the_new_month_opens_the_cap_again() {
        let report = UsageReport::new(vec![task(day(2026, 10, 31), 70.0)]);
        assert_eq!(cap_check(&cap_of("60"), Some(&report), day(2026, 11, 1)), CapCheck::Open);
        assert!(matches!(cap_check(&cap_of("60"), Some(&report), day(2026, 10, 31)), CapCheck::Reached(_)));
    }

    #[test]
    fn the_cap_words_say_over_or_exactly_at() {
        let over = SpendCap { on: true, amount: Some(dollars("60")) }.reached(60.4, day(2026, 10, 31)).unwrap();
        assert_eq!(cap_line(&over), "This month's tasks reached your $60.00 cap.");
        assert_eq!(cap_reason(&over), "This month's tasks cost $60.40, over your $60.00 cap. Run opens again on 1\u{a0}November.");
        let at = SpendCap { on: true, amount: Some(dollars("60")) }.reached(60.0, day(2026, 12, 9)).unwrap();
        assert_eq!(cap_reason(&at), "This month's tasks cost $60.00, your whole $60.00 cap. Run opens again on 1\u{a0}January.");
        assert_eq!(CAP_READING, "Run waits while this month's spend is read.");
    }

    #[test]
    fn the_cap_word_drops_whole_cents() {
        assert_eq!(cap_word(dollars("60")), "Cap $60");
        assert_eq!(cap_word(dollars("60.50")), "Cap $60.50");
        assert_eq!(cap_word(dollars("0.05")), "Cap $0.05");
        assert_eq!(cap_word(dollars("1500")), "Cap $1,500");
    }

    #[test]
    fn sizes_read_in_decimal_units() {
        assert_eq!(size_detail(0), "0 bytes");
        assert_eq!(size_detail(1), "1 byte");
        assert_eq!(size_detail(999), "999 bytes");
        assert_eq!(size_detail(1_000), "1 KB");
        assert_eq!(size_detail(18_000), "18 KB");
        assert_eq!(size_detail(211_800_000), "211.8 MB");
        assert_eq!(size_detail(1_500_000_000), "1.5 GB");
        assert_eq!(size_detail(999_999), "1.0 MB", "no 1000 KB");
        assert_eq!(size_detail(999_950_000), "1.0 GB", "no 1000.0 MB");
        assert_eq!(size_detail(u64::MAX), "18446744073.7 GB");
    }

    #[test]
    fn short_sizes_are_whole() {
        assert_eq!(size_short(0), "0 bytes");
        assert_eq!(size_short(999), "999 bytes");
        assert_eq!(size_short(18_400), "18 KB");
        assert_eq!(size_short(211_800_000), "212 MB");
        assert_eq!(size_short(1_900_000_000), "2 GB");
        assert_eq!(size_short(999_600), "1 MB");
    }
}
