use std::time::Duration;

use chrono::NaiveDateTime;

use crate::updates::application::words::{
    self, AUTO_OFF, AUTO_ON, CHECK_FAILED, CHECKING, NOT_CHECKED, NOTES_FOOT, RELEASE_PAGE, RESTART, TRUST_LINE, UP_TO_DATE, WHATS_NEW,
};
use crate::updates::domain::{
    CHECK_EVERY, Check, Download, InstallBlock, Platform, Release, RestartPlan, ServiceEvent, UpdateService, UpdateState, Version, due, percent, release_page,
};

/// The colour of the square that leads the update group: news, good, or failed. Ink is the screen's to pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    News,
    Good,
    Failed,
}

/// What the one action of the update group does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarAction {
    Download,
    Cancel,
    Retry,
    Restart,
}

impl BarAction {
    /// The label in the status bar, where the version is already beside it.
    pub fn label(self) -> &'static str {
        match self {
            BarAction::Download => "Download",
            BarAction::Cancel => "Cancel",
            BarAction::Retry => "Retry",
            BarAction::Restart => RESTART,
        }
    }
}

/// The bar of a download that runs: the share as a fraction for the bar, then the words after it.
#[derive(Debug, Clone, PartialEq)]
pub struct BarProgress {
    pub fraction: f32,
    /// `38%`.
    pub share: String,
    /// `5.4 of 14.2 MB`.
    pub size: String,
}

/// The update group at the right of the status bar. It is there only once a newer release is known.
#[derive(Debug, Clone, PartialEq)]
pub struct BarGroup {
    /// The square before the words. A running download draws its bar instead, and the mark is not used.
    pub mark: Mark,
    pub words: String,
    /// `None` when the copy cannot update itself, and the notes say why.
    pub action: Option<BarAction>,
    pub progress: Option<BarProgress>,
}

/// The one step the notes offer, labelled with the version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesAction {
    pub action: BarAction,
    pub label: String,
}

/// The release notes popover.
#[derive(Debug, Clone, PartialEq)]
pub struct NotesView {
    /// `Jobdesk 0.2.0`.
    pub title: String,
    /// `Released Fri 30 Oct. You have 0.1.0.`
    pub meta: String,
    /// Markdown from the network. It is drawn only as text, with no link and no image.
    pub notes: String,
    pub trust: &'static str,
    pub foot: &'static str,
    /// `14.2 MB`.
    pub size: String,
    /// Built from the version tag, never read from the feed or the notes.
    pub release_page: String,
    pub release_page_label: &'static str,
    pub action: Option<NotesAction>,
    /// Why this copy cannot install it, in place of the action.
    pub blocked: Option<String>,
    /// What went wrong with the download, when it did.
    pub problem: Option<String>,
    /// What Restart to update does, once the download is ready.
    pub slip: Option<String>,
}

/// A line of help under a setting. `failed` draws it in the failed ink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpLine {
    pub text: String,
    pub failed: bool,
}

/// What a button in About does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AboutAction {
    /// Opens the notes popover.
    WhatsNew,
    /// Opens the page of the release, built from the version tag.
    ReleasePage,
    Bar(BarAction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutButton {
    pub action: AboutAction,
    pub label: String,
    pub primary: bool,
}

/// Settings › About › Updates.
#[derive(Debug, Clone, PartialEq)]
pub struct AboutUpdate {
    /// The switch, and the help under it.
    pub auto: bool,
    pub auto_help: &'static str,
    /// The tag beside Latest version: `Up to date` or `Check failed`.
    pub tag: Option<(&'static str, Mark)>,
    /// Draws `Checking…` as muted words.
    pub checking: bool,
    pub help: Vec<HelpLine>,
    /// Check now: `Some(enabled)`, or `None` when a newer release is known and the step to take is a button.
    pub check_now: Option<bool>,
    pub buttons: Vec<AboutButton>,
}

/// Everything the screens draw of an update. The screens read this and decide nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateView {
    pub bar: Option<BarGroup>,
    pub notes: Option<NotesView>,
    pub about: AboutUpdate,
    /// The word beside About on the settings rail, and whether it reads as failed.
    pub rail_word: (String, bool),
}

/// The update of Jobdesk itself: when to look, what was found, the download, and what Restart applies.
pub struct Updates {
    service: Box<dyn UpdateService>,
    state: UpdateState,
    platform: Platform,
    auto: bool,
    last_attempt: Option<NaiveDateTime>,
    /// The time of the last `poll` or `check_now`, which the words say a check relative to.
    now: Option<NaiveDateTime>,
}

impl Updates {
    /// `current` is the version this copy runs, as `CARGO_PKG_VERSION` gives it. `block` is why it cannot replace itself.
    /// Automatic checks start on, as the setting does.
    pub fn new(service: Box<dyn UpdateService>, current: &str, block: Option<InstallBlock>) -> Self {
        let current = Version::parse(current).unwrap_or_default();
        Self {
            service,
            state: UpdateState::new(current, block),
            platform: Platform::current().unwrap_or(Platform::MacArm),
            auto: true,
            last_attempt: None,
            now: None,
        }
    }

    /// Follows the setting. Only a change counts, so this may run every frame.
    pub fn set_auto(&mut self, auto: bool) {
        if self.auto != auto {
            self.auto = auto;
        }
    }

    /// Takes what the service reported, then starts an automatic check when one is due. Call it every frame.
    pub fn poll(&mut self, now: NaiveDateTime) {
        self.now = Some(now);
        while let Some(event) = self.service.poll() {
            match event {
                ServiceEvent::Checked(result) => self.state.checked(result, now),
                ServiceEvent::Progress { done, total } => self.state.progress(done, total),
                ServiceEvent::Ended(end) => self.state.ended(end),
            }
        }
        if due(self.last_attempt, now, self.auto) {
            self.start_check(false, now);
        }
    }

    /// Check now, which looks whatever the switch says.
    pub fn check_now(&mut self, now: NaiveDateTime) {
        self.now = Some(now);
        self.start_check(true, now);
    }

    fn start_check(&mut self, manual: bool, now: NaiveDateTime) {
        if self.state.start_check(manual) {
            self.last_attempt = Some(now);
            self.service.check();
        }
    }

    /// Starts the download of the release found, when it may start: Download and Retry.
    pub fn download(&mut self) {
        if let Some(release) = self.state.start_download() {
            self.service.download(&release);
        }
    }

    /// Stops a running download. It is Idle at once, and the service deletes what it wrote.
    pub fn cancel(&mut self) {
        if matches!(self.state.download, Download::Running { .. }) {
            self.service.cancel();
            self.state.cancel();
        }
    }

    /// What Restart to update applies. Only a download that is ready has one.
    pub fn restart_plan(&self) -> Option<RestartPlan> {
        self.state.restart_plan()
    }

    /// How long until the next automatic check is due, so the window can wake then. `None` when none will be.
    pub fn next_due(&self, now: NaiveDateTime) -> Option<Duration> {
        if !self.auto || self.state.is_checking() {
            return None;
        }
        let Some(last) = self.last_attempt else { return Some(Duration::ZERO) };
        let since = (now - last).to_std().unwrap_or(CHECK_EVERY);
        Some(CHECK_EVERY.saturating_sub(since))
    }

    pub fn view(&self) -> UpdateView {
        let (rail_word, failed) = self.rail_word();
        UpdateView { bar: self.bar(), notes: self.notes(), about: self.about(), rail_word: (rail_word, failed) }
    }

    fn bar(&self) -> Option<BarGroup> {
        let release = self.state.release.as_ref()?;
        let version = release.version;
        let group = |mark, words, action| BarGroup { mark, words, action, progress: None };
        Some(match &self.state.download {
            Download::Idle => group(Mark::News, words::bar_available(version), self.state.block.is_none().then_some(BarAction::Download)),
            Download::Running { done, total } => BarGroup {
                mark: Mark::News,
                words: words::bar_downloading(version),
                action: Some(BarAction::Cancel),
                progress: Some(BarProgress {
                    fraction: f32::from(percent(*done, *total)) / 100.0,
                    share: words::share(*done, *total),
                    size: words::progress_size(*done, *total),
                }),
            },
            Download::Ready { .. } => group(Mark::Good, words::bar_ready(version), Some(BarAction::Restart)),
            Download::Failed { percent, failure } => group(Mark::Failed, words::bar_failed(version, *percent, *failure), Some(BarAction::Retry)),
            Download::BadSignature => group(Mark::Failed, words::bar_bad_signature(version), Some(BarAction::Retry)),
        })
    }

    fn notes(&self) -> Option<NotesView> {
        let release = self.state.release.as_ref()?;
        let version = release.version;
        let step = |action: BarAction, label: String| Some(NotesAction { action, label });
        let (action, problem, slip) = match &self.state.download {
            Download::Idle => (self.state.block.is_none().then(|| step(BarAction::Download, words::notes_download(version))).flatten(), None, None),
            Download::Running { .. } => (step(BarAction::Cancel, BarAction::Cancel.label().to_owned()), None, None),
            Download::Ready { .. } => (step(BarAction::Restart, RESTART.to_owned()), None, Some(words::ready_slip(self.platform, version))),
            Download::Failed { percent, failure } => {
                (step(BarAction::Retry, BarAction::Retry.label().to_owned()), Some(words::bar_failed(version, *percent, *failure)), None)
            }
            Download::BadSignature => (step(BarAction::Retry, BarAction::Retry.label().to_owned()), Some(words::bar_bad_signature(version)), None),
        };
        Some(NotesView {
            title: words::notes_title(version),
            meta: words::notes_meta(self.state.current, release.released),
            notes: release.notes.clone(),
            trust: TRUST_LINE,
            foot: NOTES_FOOT,
            size: words::size_words(release.size),
            release_page: release_page(&version),
            release_page_label: RELEASE_PAGE,
            action,
            blocked: self.state.block.map(|block| words::block_words(block, version)),
            problem,
            slip,
        })
    }

    fn about(&self) -> AboutUpdate {
        let mut about = AboutUpdate {
            auto: self.auto,
            auto_help: if self.auto { AUTO_ON } else { AUTO_OFF },
            tag: None,
            checking: false,
            help: Vec::new(),
            check_now: Some(true),
            buttons: Vec::new(),
        };
        let line = |text: String| HelpLine { text, failed: false };
        // A release that is known leads, whatever a later check said.
        if let Some(release) = &self.state.release {
            about.check_now = None;
            self.about_release(&mut about, release);
            return about;
        }
        match &self.state.check {
            Check::Checking { .. } => {
                about.help.push(line(CHECKING.to_owned()));
                about.checking = true;
                about.check_now = Some(false);
            }
            Check::UpToDate { at } => {
                about.help.push(line(words::last_checked(*at, self.now.unwrap_or(*at))));
                about.tag = Some((UP_TO_DATE, Mark::Good));
            }
            Check::Failed { at, failure, .. } => {
                about.help.push(HelpLine { text: words::check_failed_line(*failure, *at, self.now.unwrap_or(*at)), failed: true });
                about.help.push(line(words::check_failed_next(self.auto).to_owned()));
                about.tag = Some((CHECK_FAILED, Mark::Failed));
            }
            Check::NotYet | Check::Found { .. } => about.help.push(line(NOT_CHECKED.to_owned())),
        }
        about
    }

    fn about_release(&self, about: &mut AboutUpdate, release: &Release) {
        let version = release.version;
        let line = |text: String| HelpLine { text, failed: false };
        let button = |action: AboutAction, label: &str, primary: bool| AboutButton { action, label: label.to_owned(), primary };
        let whats_new = button(AboutAction::WhatsNew, WHATS_NEW, false);
        match &self.state.download {
            Download::Idle => {
                let mut text = words::about_update(version, self.state.current, release.released);
                if let (Check::Found { at } | Check::UpToDate { at }, Some(now)) = (&self.state.check, self.now) {
                    text = format!("{text} {}", words::last_checked(*at, now));
                }
                about.help.push(line(text));
                about.buttons.push(whats_new);
                match self.state.block {
                    Some(block) => {
                        about.help.push(line(words::block_words(block, version)));
                        about.buttons.push(button(AboutAction::ReleasePage, RELEASE_PAGE, false));
                    }
                    None => {
                        about.buttons.push(AboutButton { action: AboutAction::Bar(BarAction::Download), label: words::notes_download(version), primary: true })
                    }
                }
            }
            Download::Running { done, total } => {
                about.help.push(line(words::about_downloading(version, *done, *total)));
                about.buttons.push(button(AboutAction::Bar(BarAction::Cancel), BarAction::Cancel.label(), false));
            }
            Download::Ready { .. } => {
                about.help.push(line(words::about_ready(version)));
                about.buttons.push(button(AboutAction::Bar(BarAction::Restart), RESTART, true));
            }
            Download::Failed { percent, failure } => {
                about.help.push(HelpLine { text: words::about_download_failed(version, *percent, *failure), failed: true });
                about.buttons.push(whats_new);
                about.buttons.push(button(AboutAction::Bar(BarAction::Retry), BarAction::Retry.label(), true));
            }
            Download::BadSignature => {
                about.help.push(HelpLine { text: words::about_bad_signature(version), failed: true });
                about.buttons.push(whats_new);
                about.buttons.push(button(AboutAction::Bar(BarAction::Retry), BarAction::Retry.label(), true));
            }
        }
    }

    /// The word beside About on the rail: the version in use, or the news, or that a check failed.
    fn rail_word(&self) -> (String, bool) {
        if let Some(release) = &self.state.release {
            let version = release.version;
            return match self.state.download {
                Download::Ready { .. } => (words::rail_ready(version), false),
                _ => (words::rail_available(version), false),
            };
        }
        match self.state.check {
            Check::Failed { .. } => (CHECK_FAILED.to_owned(), true),
            _ => (self.state.current.to_string(), false),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use chrono::NaiveDate;

    use super::*;
    use crate::updates::domain::{DownloadEnd, Failure};

    #[derive(Default)]
    struct Log {
        calls: Vec<&'static str>,
        events: VecDeque<ServiceEvent>,
        downloaded: Vec<Version>,
    }

    struct Recording(Arc<Mutex<Log>>);

    impl UpdateService for Recording {
        fn check(&mut self) {
            self.0.lock().unwrap().calls.push("check");
        }
        fn download(&mut self, release: &Release) {
            let mut log = self.0.lock().unwrap();
            log.calls.push("download");
            log.downloaded.push(release.version);
        }
        fn cancel(&mut self) {
            self.0.lock().unwrap().calls.push("cancel");
        }
        fn poll(&mut self) -> Option<ServiceEvent> {
            self.0.lock().unwrap().events.pop_front()
        }
    }

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 3).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn release() -> Release {
        Release {
            version: Version { major: 0, minor: 2, patch: 0 },
            released: NaiveDate::from_ymd_opt(2026, 10, 30),
            notes: "## New\n- A thing".into(),
            size: 14_200_000,
            signature: "sig".into(),
        }
    }

    fn desk(block: Option<InstallBlock>) -> (Updates, Arc<Mutex<Log>>) {
        let log = Arc::new(Mutex::new(Log::default()));
        (Updates::new(Box::new(Recording(log.clone())), "0.1.0", block), log)
    }

    fn answer(log: &Arc<Mutex<Log>>, event: ServiceEvent) {
        log.lock().unwrap().events.push_back(event);
    }

    fn calls(log: &Arc<Mutex<Log>>) -> Vec<&'static str> {
        log.lock().unwrap().calls.clone()
    }

    /// A desk that found 0.2.0 at 09:00.
    fn found() -> (Updates, Arc<Mutex<Log>>) {
        let (mut updates, log) = desk(None);
        updates.poll(at(9, 0));
        answer(&log, ServiceEvent::Checked(Ok(release())));
        updates.poll(at(9, 0));
        (updates, log)
    }

    fn ready() -> (Updates, Arc<Mutex<Log>>) {
        let (mut updates, log) = found();
        updates.download();
        answer(&log, ServiceEvent::Ended(DownloadEnd::Done(PathBuf::from("/data/updates/Jobdesk"))));
        updates.poll(at(9, 5));
        (updates, log)
    }

    #[test]
    fn the_first_poll_starts_an_automatic_check() {
        let (mut updates, log) = desk(None);
        updates.poll(at(9, 0));
        assert_eq!(calls(&log), ["check"]);
        updates.poll(at(9, 1));
        assert_eq!(calls(&log), ["check"], "one check at a time");
    }

    #[test]
    fn with_the_switch_off_no_check_starts_until_asked() {
        let (mut updates, log) = desk(None);
        updates.set_auto(false);
        updates.poll(at(9, 0));
        assert!(calls(&log).is_empty());
        assert_eq!(updates.next_due(at(9, 0)), None);
        updates.check_now(at(9, 1));
        assert_eq!(calls(&log), ["check"]);
    }

    #[test]
    fn a_check_comes_again_six_hours_after_the_last_attempt() {
        let (mut updates, log) = desk(None);
        updates.poll(at(9, 0));
        answer(&log, ServiceEvent::Checked(Ok(Release { version: Version { major: 0, minor: 1, patch: 0 }, ..release() })));
        updates.poll(at(9, 0));
        updates.poll(at(14, 59));
        assert_eq!(calls(&log), ["check"]);
        assert_eq!(updates.next_due(at(14, 59)), Some(Duration::from_secs(60)));
        updates.poll(at(15, 0));
        assert_eq!(calls(&log), ["check", "check"]);
    }

    #[test]
    fn a_manual_check_failure_and_an_automatic_one_both_reach_about() {
        let (mut updates, log) = desk(None);
        updates.poll(at(9, 12));
        answer(&log, ServiceEvent::Checked(Err(Failure::TimedOut)));
        updates.poll(at(9, 12));
        let view = updates.view();
        assert_eq!(view.bar, None, "a failed automatic check shows nothing in the bar");
        assert_eq!(view.rail_word, ("Check failed".to_owned(), true));
        assert_eq!(view.about.tag, Some(("Check failed", Mark::Failed)));
        assert_eq!(view.about.help[0], HelpLine { text: "Could not reach GitHub at 09:12: the connection timed out.".into(), failed: true });
        assert_eq!(view.about.help[1].text, "Jobdesk tries again in about 6 hours, and nothing changed on this computer.");
        assert_eq!(view.about.check_now, Some(true));
        assert_eq!(view.notes, None);
    }

    #[test]
    fn about_says_checking_and_disables_check_now() {
        let (mut updates, _log) = desk(None);
        updates.set_auto(false);
        updates.check_now(at(9, 0));
        let about = updates.view().about;
        assert!(about.checking);
        assert_eq!(about.check_now, Some(false));
        assert_eq!(about.help[0].text, "Asking GitHub for the latest release…");
        assert!(!about.auto);
        assert_eq!(about.auto_help, "Off. Jobdesk looks for a new version only when you choose Check now.");
        assert_eq!(updates.view().bar, None, "checking shows nothing in the bar");
    }

    #[test]
    fn about_says_up_to_date_after_a_check_that_found_nothing() {
        let (mut updates, log) = desk(None);
        updates.poll(at(9, 12));
        answer(&log, ServiceEvent::Checked(Ok(Release { version: Version { major: 0, minor: 1, patch: 0 }, ..release() })));
        updates.poll(at(9, 12));
        let view = updates.view();
        assert_eq!(view.about.tag, Some(("Up to date", Mark::Good)));
        assert_eq!(view.about.help[0].text, "Last checked today at 09:12.");
        assert_eq!(view.rail_word, ("0.1.0".to_owned(), false));
        assert_eq!((view.bar, view.notes), (None, None));
    }

    #[test]
    fn about_before_any_check_says_so() {
        let (updates, _log) = desk(None);
        let view = updates.view();
        assert_eq!(view.about.help[0].text, "Not checked yet.");
        assert_eq!(view.about.auto_help, AUTO_ON);
        assert_eq!(view.rail_word, ("0.1.0".to_owned(), false));
    }

    #[test]
    fn a_newer_release_shows_in_the_bar_with_download() {
        let (updates, _log) = found();
        let view = updates.view();
        let bar = view.bar.unwrap();
        assert_eq!((bar.mark, bar.words.as_str(), bar.action, bar.progress), (Mark::News, "Jobdesk 0.2.0 is out", Some(BarAction::Download), None));
        assert_eq!(view.rail_word, ("0.2.0 is out".to_owned(), false));
        let about = view.about;
        assert_eq!(about.help[0].text, "Jobdesk 0.2.0, released Fri 30 Oct. You have 0.1.0. Last checked today at 09:00.");
        assert_eq!(about.check_now, None);
        let labels: Vec<_> = about.buttons.iter().map(|button| (button.label.as_str(), button.primary)).collect();
        assert_eq!(labels, [("What's new", false), ("Download 0.2.0", true)]);
    }

    #[test]
    fn the_notes_carry_the_release_and_the_link_built_from_its_version() {
        let (updates, _log) = found();
        let notes = updates.view().notes.unwrap();
        assert_eq!(notes.title, "Jobdesk 0.2.0");
        assert_eq!(notes.meta, "Released Fri 30 Oct. You have 0.1.0.");
        assert_eq!(notes.notes, "## New\n- A thing");
        assert_eq!(notes.trust, "From the GitHub release. Its links and images do not open here.");
        assert_eq!(notes.foot, "Nothing installs until you choose Restart to update. Esc closes.");
        assert_eq!(notes.size, "14.2 MB");
        assert_eq!(notes.release_page, "https://github.com/Nicolas-Widart-SRL/jobdesk/releases/tag/v0.2.0");
        assert_eq!(notes.action, Some(NotesAction { action: BarAction::Download, label: "Download 0.2.0".into() }));
        assert_eq!((notes.blocked, notes.problem, notes.slip), (None, None, None));
    }

    #[test]
    fn download_starts_the_service_once_and_shows_progress() {
        let (mut updates, log) = found();
        updates.download();
        updates.download();
        assert_eq!(calls(&log), ["check", "download"]);
        assert_eq!(log.lock().unwrap().downloaded, [Version { major: 0, minor: 2, patch: 0 }]);
        answer(&log, ServiceEvent::Progress { done: 5_400_000, total: 14_200_000 });
        updates.poll(at(9, 1));
        let view = updates.view();
        let bar = view.bar.unwrap();
        assert_eq!(bar.words, "Downloading 0.2.0");
        assert_eq!(bar.action, Some(BarAction::Cancel));
        let progress = bar.progress.unwrap();
        assert_eq!((progress.share.as_str(), progress.size.as_str()), ("38%", "5.4 of 14.2 MB"));
        assert!((progress.fraction - 0.38).abs() < 1e-6);
        assert_eq!(view.about.help[0].text, "Downloading 0.2.0: 38%, 5.4 of 14.2 MB.");
        assert_eq!(view.about.buttons[0].label, "Cancel");
    }

    #[test]
    fn cancel_stops_the_service_and_returns_to_download() {
        let (mut updates, log) = found();
        updates.cancel();
        assert_eq!(calls(&log), ["check"], "nothing runs, so nothing is cancelled");
        updates.download();
        updates.cancel();
        assert_eq!(calls(&log), ["check", "download", "cancel"]);
        assert_eq!(updates.view().bar.unwrap().action, Some(BarAction::Download));
        updates.download();
        assert_eq!(calls(&log), ["check", "download", "cancel", "download"]);
    }

    #[test]
    fn a_blocked_copy_shows_the_news_with_a_reason_and_no_download() {
        let (mut updates, log) = desk(Some(InstallBlock::ReadOnly));
        updates.poll(at(9, 0));
        answer(&log, ServiceEvent::Checked(Ok(release())));
        updates.poll(at(9, 0));
        updates.download();
        assert_eq!(calls(&log), ["check"], "the service is never asked to download");
        let view = updates.view();
        assert_eq!(view.bar.unwrap().action, None);
        let notes = view.notes.unwrap();
        assert_eq!(notes.action, None);
        assert_eq!(notes.blocked.as_deref(), Some("Jobdesk cannot replace itself where it is installed. Install 0.2.0 from the release page."));
        let buttons: Vec<_> = view.about.buttons.iter().map(|button| button.action).collect();
        assert_eq!(buttons, [AboutAction::WhatsNew, AboutAction::ReleasePage]);
        assert_eq!(view.about.help.len(), 2);
    }

    #[test]
    fn a_ready_download_offers_restart_and_a_plan() {
        let (updates, _log) = ready();
        let view = updates.view();
        let bar = view.bar.unwrap();
        assert_eq!((bar.mark, bar.words.as_str(), bar.action), (Mark::Good, "0.2.0 is downloaded and checked", Some(BarAction::Restart)));
        assert_eq!(view.rail_word, ("0.2.0 ready".to_owned(), false));
        let notes = view.notes.unwrap();
        assert_eq!(notes.action, Some(NotesAction { action: BarAction::Restart, label: "Restart to update".into() }));
        assert!(notes.slip.unwrap().starts_with("Jobdesk closes, "));
        assert!(view.about.help[0].text.starts_with("Jobdesk 0.2.0 is downloaded and its signature checked."));
        let plan = updates.restart_plan().unwrap();
        assert_eq!((plan.version, plan.file), (Version { major: 0, minor: 2, patch: 0 }, PathBuf::from("/data/updates/Jobdesk")));
    }

    #[test]
    fn there_is_no_restart_plan_before_the_download_is_ready() {
        let (mut updates, _log) = found();
        assert_eq!(updates.restart_plan(), None);
        updates.download();
        assert_eq!(updates.restart_plan(), None);
    }

    #[test]
    fn a_failed_download_says_why_and_offers_retry() {
        let (mut updates, log) = found();
        updates.download();
        answer(&log, ServiceEvent::Ended(DownloadEnd::Failed { percent: 41, failure: Failure::Interrupted }));
        updates.poll(at(9, 3));
        let view = updates.view();
        let bar = view.bar.unwrap();
        assert_eq!(
            (bar.mark, bar.words.as_str(), bar.action),
            (Mark::Failed, "Download of 0.2.0 stopped at 41%: the connection dropped", Some(BarAction::Retry))
        );
        assert_eq!(view.notes.unwrap().problem.as_deref(), Some("Download of 0.2.0 stopped at 41%: the connection dropped"));
        assert!(view.about.help[0].failed);
        updates.download();
        assert_eq!(calls(&log), ["check", "download", "download"]);
    }

    #[test]
    fn a_bad_signature_is_said_plainly_and_may_be_retried() {
        let (mut updates, log) = found();
        updates.download();
        answer(&log, ServiceEvent::Ended(DownloadEnd::BadSignature));
        updates.poll(at(9, 3));
        let bar = updates.view().bar.unwrap();
        assert_eq!(bar.words, "0.2.0 failed its signature check. It was deleted, not installed.");
        assert_eq!((bar.mark, bar.action), (Mark::Failed, Some(BarAction::Retry)));
        assert_eq!(updates.restart_plan(), None);
        updates.download();
        assert_eq!(calls(&log), ["check", "download", "download"]);
    }

    #[test]
    fn a_failed_check_leaves_a_known_release_in_the_bar_and_about() {
        let (mut updates, log) = found();
        updates.check_now(at(15, 0));
        answer(&log, ServiceEvent::Checked(Err(Failure::Unreachable)));
        updates.poll(at(15, 0));
        let view = updates.view();
        assert!(view.bar.is_some());
        assert_eq!(view.rail_word.0, "0.2.0 is out");
        assert_eq!(view.about.tag, None);
    }

    #[test]
    fn a_newer_release_keeps_the_download_that_is_ready() {
        let (mut updates, log) = ready();
        updates.check_now(at(16, 0));
        answer(&log, ServiceEvent::Checked(Ok(Release { version: Version { major: 0, minor: 3, patch: 0 }, ..release() })));
        updates.poll(at(16, 0));
        assert_eq!(updates.restart_plan().unwrap().version, Version { major: 0, minor: 2, patch: 0 });
        assert_eq!(updates.view().bar.unwrap().words, "0.2.0 is downloaded and checked");
    }

    #[test]
    fn check_now_does_not_start_a_second_check() {
        let (mut updates, log) = desk(None);
        updates.poll(at(9, 0));
        updates.check_now(at(9, 1));
        assert_eq!(calls(&log), ["check"]);
    }

    #[test]
    fn the_window_wakes_for_the_next_check() {
        let (mut updates, log) = desk(None);
        assert_eq!(updates.next_due(at(9, 0)), Some(Duration::ZERO));
        updates.poll(at(9, 0));
        assert_eq!(updates.next_due(at(9, 0)), None, "while it checks");
        answer(&log, ServiceEvent::Checked(Err(Failure::TimedOut)));
        updates.poll(at(9, 0));
        assert_eq!(updates.next_due(at(9, 0)), Some(CHECK_EVERY));
        assert_eq!(updates.next_due(at(12, 0)), Some(Duration::from_secs(3 * 3600)));
        assert_eq!(updates.next_due(at(21, 0)), Some(Duration::ZERO));
    }

    #[test]
    fn the_switch_is_followed_only_when_it_changes() {
        let (mut updates, _log) = desk(None);
        assert!(updates.view().about.auto);
        updates.set_auto(false);
        updates.set_auto(false);
        assert!(!updates.view().about.auto);
        updates.set_auto(true);
        assert!(updates.view().about.auto);
    }
}
