use std::time::Duration;

use chrono::NaiveDateTime;

use crate::updates::application::words::{
    self, AUTO_OFF, AUTO_ON, CHECK_FAILED, CHECKING, NOT_CHECKED, NOTES_FOOT, RELEASE_PAGE, RESTART, TRUST_LINE, UP_TO_DATE, WHATS_NEW,
};
use crate::updates::domain::{
    ApplyFailed, CHECK_EVERY, Check, Download, InstallBlock, Platform, Release, RestartPlan, ServiceEvent, UpdateService, UpdateState, Version, due, percent,
    release_page,
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
    /// What a screen reader calls the notes: `What changed in 0.2.0`.
    pub notes_label: String,
    pub trust: &'static str,
    pub foot: &'static str,
    /// `14.2 MB`.
    pub size: String,
    /// Built from the version tag, never read from the feed or the notes.
    /// `None` when the release page has no build this copy could use.
    pub release_page: Option<String>,
    pub release_page_label: &'static str,
    pub action: Option<NotesAction>,
    /// Why this copy cannot install it, in place of the action.
    pub blocked: Option<String>,
    /// What went wrong with the download, or with the last Restart to update, when it did.
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
    /// `None` on a computer Jobdesk publishes no build for.
    platform: Option<Platform>,
    auto: bool,
    last_attempt: Option<NaiveDateTime>,
    /// The time of the last `poll` or `check_now`, which the words say a check relative to.
    now: Option<NaiveDateTime>,
    /// The release the last Restart to update did not install, until a download starts or the release is gone.
    apply_failed: Option<ApplyFailed>,
}

impl Updates {
    /// `current` is the version this copy runs, as `CARGO_PKG_VERSION` gives it. `block` is why it cannot replace itself.
    /// `auto` is the setting as it is saved, so no check starts against the user's choice before the first `set_auto`.
    /// A copy that runs on a computer with no published build has no feed entry to read: it checks nothing and says why.
    /// A copy whose version cannot be read has nothing to compare: it checks nothing and offers nothing.
    pub fn new(service: Box<dyn UpdateService>, current: &str, block: Option<InstallBlock>, auto: bool) -> Self {
        Self::with_platform(service, current, block, auto, Platform::current())
    }

    /// As `new`, for the platform given instead of the one this runs on. For tests and previews, so each state looks
    /// the same on every computer.
    pub fn with_platform(service: Box<dyn UpdateService>, current: &str, block: Option<InstallBlock>, auto: bool, platform: Option<Platform>) -> Self {
        let version = Version::parse(current);
        // Neither of these two can ever find a release, so each outweighs whatever else the caller found.
        let block = match (version, platform) {
            (None, _) => Some(InstallBlock::UnreadableVersion),
            (Some(_), None) => Some(InstallBlock::NoBuild),
            (Some(_), Some(_)) => block,
        };
        Self { service, state: UpdateState::new(version.unwrap_or_default(), block), platform, auto, last_attempt: None, now: None, apply_failed: None }
    }

    /// Whether this copy knows its own version, so a release can be told newer or older.
    fn comparable(&self) -> bool {
        self.state.block != Some(InstallBlock::UnreadableVersion)
    }

    /// Why no check could ever find a release, if none could: this copy does not know its version, or Jobdesk publishes
    /// no build for it. Then no check starts and About says why.
    fn never_checks(&self) -> Option<InstallBlock> {
        self.state.block.filter(|block| matches!(block, InstallBlock::UnreadableVersion | InstallBlock::NoBuild))
    }

    /// Takes why the last Restart to update did not install a release, as the launch read it. The bar says it in the
    /// failed look until a download starts again. A release no newer than this copy was installed after all, or is gone,
    /// so it is dropped.
    pub fn apply_failed(&mut self, failed: ApplyFailed) {
        if self.comparable() && failed.version > self.state.current {
            self.apply_failed = Some(failed);
        }
    }

    /// The failed Restart to update the screens say, while nothing took its place: no other release found, and no
    /// download started since.
    fn apply_failure(&self) -> Option<ApplyFailed> {
        let failed = self.apply_failed?;
        let same = self.state.release.as_ref().is_none_or(|release| release.version == failed.version);
        (same && self.state.download == Download::Idle).then_some(failed)
    }

    /// Follows the setting. It is cheap, so it may run every frame.
    pub fn set_auto(&mut self, auto: bool) {
        self.auto = auto;
    }

    /// Takes what the service reported, then starts an automatic check when one is due. Call it every frame.
    pub fn poll(&mut self, now: NaiveDateTime) {
        self.now = Some(now);
        while let Some(event) = self.service.poll() {
            match event {
                // With no version of its own to compare, or no build to take, whatever the feed says is not taken.
                ServiceEvent::Checked(_) if self.never_checks().is_some() => {}
                ServiceEvent::Checked(result) => {
                    // A release that is gone takes its download with it, and the service deletes the file.
                    if self.state.checked(result, now) {
                        self.service.cancel();
                    }
                    // The release that did not install is no longer the latest, so there is nothing left to retry.
                    if matches!(self.state.check, Check::UpToDate { .. }) {
                        self.apply_failed = None;
                    }
                }
                ServiceEvent::Progress { id, done, total } => self.state.progress(id, done, total),
                ServiceEvent::Ended { id, end } => self.state.ended(id, end),
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
        if self.never_checks().is_none() && self.state.start_check(manual) {
            self.last_attempt = Some(now);
            self.service.check();
        }
    }

    /// Starts the download of the release found, when it may start: Download and Retry.
    pub fn download(&mut self) {
        if let Some((release, id)) = self.state.start_download() {
            self.apply_failed = None;
            self.service.download(&release, id);
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
        if !self.auto || self.state.is_checking() || self.never_checks().is_some() {
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
        if let Some(failed) = self.apply_failure() {
            // Retry once the release is known again, which the next check tells.
            let action = (self.state.release.is_some() && self.state.block.is_none()).then_some(BarAction::Retry);
            return Some(BarGroup { mark: Mark::Failed, words: words::bar_apply_failed(failed.version, failed.failure), action, progress: None });
        }
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
        let may = self.state.block.is_none();
        let (action, problem, slip) = match &self.state.download {
            Download::Idle => match self.apply_failure() {
                Some(failed) => (
                    may.then(|| step(BarAction::Retry, BarAction::Retry.label().to_owned())).flatten(),
                    Some(words::bar_apply_failed(failed.version, failed.failure)),
                    None,
                ),
                None => (may.then(|| step(BarAction::Download, words::notes_download(version))).flatten(), None, None),
            },
            Download::Running { .. } => (step(BarAction::Cancel, BarAction::Cancel.label().to_owned()), None, None),
            Download::Ready { .. } => (step(BarAction::Restart, RESTART.to_owned()), None, self.platform.map(|platform| words::ready_slip(platform, version))),
            Download::Failed { percent, failure } => {
                (step(BarAction::Retry, BarAction::Retry.label().to_owned()), Some(words::bar_failed(version, *percent, *failure)), None)
            }
            Download::BadSignature => (step(BarAction::Retry, BarAction::Retry.label().to_owned()), Some(words::bar_bad_signature(version)), None),
        };
        Some(NotesView {
            title: words::notes_title(version),
            meta: words::notes_meta(self.state.current, release.released),
            notes: release.notes.clone(),
            notes_label: words::notes_label(version),
            trust: TRUST_LINE,
            foot: NOTES_FOOT,
            size: words::size_words(release.size),
            release_page: self.state.block.is_none_or(words::release_page_helps).then(|| release_page(&version)),
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
        if let Some(block) = self.never_checks() {
            about.check_now = None;
            about.help.push(line(words::block_words(block, self.state.current)));
            return about;
        }
        if let (Some(failed), None) = (self.apply_failure(), &self.state.release) {
            about.help.push(HelpLine { text: words::about_apply_failed(failed.version, self.state.current, failed.failure), failed: true });
        }
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
                if let Some(failed) = self.apply_failure() {
                    about.help.push(HelpLine { text: words::about_apply_failed(failed.version, self.state.current, failed.failure), failed: true });
                    if self.state.block.is_none() {
                        about.buttons.push(button(AboutAction::Bar(BarAction::Retry), BarAction::Retry.label(), true));
                    }
                    return;
                }
                match self.state.block {
                    Some(block) => {
                        about.help.push(line(words::block_words(block, version)));
                        if words::release_page_helps(block) {
                            about.buttons.push(button(AboutAction::ReleasePage, RELEASE_PAGE, false));
                        }
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
        if !self.comparable() {
            return (words::RAIL_UNREADABLE.to_owned(), false);
        }
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
    use crate::updates::domain::{DownloadEnd, DownloadId, Failure};

    #[derive(Default)]
    struct Log {
        calls: Vec<&'static str>,
        events: VecDeque<ServiceEvent>,
        downloaded: Vec<Version>,
        ids: Vec<DownloadId>,
    }

    struct Recording(Arc<Mutex<Log>>);

    impl UpdateService for Recording {
        fn check(&mut self) {
            self.0.lock().unwrap().calls.push("check");
        }
        fn download(&mut self, release: &Release, id: DownloadId) {
            let mut log = self.0.lock().unwrap();
            log.calls.push("download");
            log.downloaded.push(release.version);
            log.ids.push(id);
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
        desk_of("0.1.0", block, Some(Platform::MacArm))
    }

    fn desk_of(current: &str, block: Option<InstallBlock>, platform: Option<Platform>) -> (Updates, Arc<Mutex<Log>>) {
        let log = Arc::new(Mutex::new(Log::default()));
        (Updates::with_platform(Box::new(Recording(log.clone())), current, block, true, platform), log)
    }

    fn answer(log: &Arc<Mutex<Log>>, event: ServiceEvent) {
        log.lock().unwrap().events.push_back(event);
    }

    /// The number of the download the service was asked for last.
    fn last_id(log: &Arc<Mutex<Log>>) -> DownloadId {
        *log.lock().unwrap().ids.last().expect("a download was started")
    }

    fn finish(log: &Arc<Mutex<Log>>, end: DownloadEnd) {
        let id = last_id(log);
        answer(log, ServiceEvent::Ended { id, end });
    }

    fn advance(log: &Arc<Mutex<Log>>, done: u64, total: u64) {
        let id = last_id(log);
        answer(log, ServiceEvent::Progress { id, done, total });
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
        finish(&log, DownloadEnd::Done(PathBuf::from("/data/updates/Jobdesk")));
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
        assert_eq!(notes.release_page.as_deref(), Some("https://github.com/Nicolas-Widart-SRL/jobdesk/releases/tag/v0.2.0"));
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
        advance(&log, 5_400_000, 14_200_000);
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
        finish(&log, DownloadEnd::Failed { percent: 41, failure: Failure::Interrupted });
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
        finish(&log, DownloadEnd::BadSignature);
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
    fn the_switch_is_followed() {
        let (mut updates, _log) = desk(None);
        assert!(updates.view().about.auto);
        updates.set_auto(false);
        updates.set_auto(false);
        assert!(!updates.view().about.auto);
        updates.set_auto(true);
        assert!(updates.view().about.auto);
    }

    #[test]
    fn a_switch_that_starts_off_never_checks_before_it_is_asked() {
        let log = Arc::new(Mutex::new(Log::default()));
        let mut updates = Updates::with_platform(Box::new(Recording(log.clone())), "0.1.0", None, false, Some(Platform::MacArm));
        updates.poll(at(9, 0));
        assert!(calls(&log).is_empty());
        assert_eq!(updates.next_due(at(9, 0)), None);
        assert!(!updates.view().about.auto);
    }

    #[test]
    fn a_version_that_cannot_be_read_checks_nothing_and_offers_nothing() {
        for current in ["0.3.0-rc.1", "", "v1", "banana"] {
            for release in [release(), Release { version: Version { major: 0, minor: 1, patch: 0 }, ..release() }] {
                let (mut updates, log) = desk_of(current, Some(InstallBlock::ReadOnly), Some(Platform::MacArm));
                updates.poll(at(9, 0));
                updates.check_now(at(9, 1));
                assert!(calls(&log).is_empty(), "{current:?}: the feed has nothing to be compared with");
                answer(&log, ServiceEvent::Checked(Ok(release)));
                updates.poll(at(9, 2));
                updates.download();
                assert!(calls(&log).is_empty(), "{current:?}");
                let view = updates.view();
                assert_eq!((view.bar, view.notes), (None, None), "{current:?}: no release is ever set");
                assert_eq!(view.rail_word, ("Version unknown".to_owned(), false));
                let about = view.about;
                assert_eq!(about.help.len(), 1);
                assert_eq!(about.help[0].text, "This copy has no version number Jobdesk can read, so it cannot tell whether a newer one is out.");
                assert_eq!((about.tag, about.check_now, about.checking, about.buttons.is_empty()), (None, None, false, true));
                assert_eq!(updates.next_due(at(9, 2)), None);
                assert_eq!(updates.restart_plan(), None);
            }
        }
    }

    #[test]
    fn a_computer_with_no_published_build_checks_nothing_and_says_so() {
        for block in [None, Some(InstallBlock::ReadOnly), Some(InstallBlock::NoKey)] {
            let (mut updates, log) = desk_of("0.1.0", block, None);
            updates.poll(at(9, 0));
            updates.check_now(at(9, 1));
            assert!(calls(&log).is_empty(), "{block:?}: the feed has no build for this computer");
            assert_eq!(updates.next_due(at(9, 1)), None);
            // The feed's answer, had a check started all the same.
            answer(&log, ServiceEvent::Checked(Err(Failure::NoBuild)));
            updates.poll(at(9, 2));
            let view = updates.view();
            assert_eq!((view.bar, view.notes), (None, None));
            assert_eq!(view.rail_word, ("0.1.0".to_owned(), false), "{block:?}: nothing failed");
            let about = view.about;
            assert_eq!(about.help, [HelpLine { text: "Jobdesk publishes no build for this computer, so it cannot update itself.".into(), failed: false }]);
            assert_eq!((about.tag, about.check_now, about.checking, about.buttons.is_empty()), (None, None, false, true));
        }
    }

    #[test]
    fn a_block_the_caller_gave_is_kept() {
        let (mut updates, log) = desk_of("0.1.0", Some(InstallBlock::ReadOnly), Some(Platform::WindowsX64));
        updates.poll(at(9, 0));
        answer(&log, ServiceEvent::Checked(Ok(release())));
        updates.poll(at(9, 0));
        let blocked = updates.view().notes.unwrap().blocked.unwrap();
        assert!(blocked.starts_with("Jobdesk cannot replace itself where it is installed."), "{blocked}");
    }

    #[test]
    fn a_late_event_of_a_cancelled_download_changes_nothing_in_the_next() {
        let (mut updates, log) = found();
        updates.download();
        let old = last_id(&log);
        updates.cancel();
        updates.download();
        let new = last_id(&log);
        assert_ne!(old, new);
        answer(&log, ServiceEvent::Ended { id: old, end: DownloadEnd::Done(PathBuf::from("/data/updates/Jobdesk")) });
        answer(&log, ServiceEvent::Progress { id: old, done: 900, total: 1000 });
        answer(&log, ServiceEvent::Ended { id: old, end: DownloadEnd::Cancelled });
        updates.poll(at(9, 5));
        assert_eq!(updates.restart_plan(), None);
        assert_eq!(updates.view().bar.unwrap().words, "Downloading 0.2.0");
        advance(&log, 5_400_000, 14_200_000);
        updates.poll(at(9, 6));
        assert_eq!(updates.view().bar.unwrap().progress.unwrap().share, "38%");
    }

    fn not_installed() -> ApplyFailed {
        ApplyFailed { version: Version { major: 0, minor: 2, patch: 0 }, failure: crate::updates::domain::ApplyFailure::Replace }
    }

    #[test]
    fn a_restart_that_did_not_install_shows_failed_until_a_download_starts() {
        let (mut updates, log) = desk(None);
        updates.apply_failed(not_installed());
        let bar = updates.view().bar.unwrap();
        assert_eq!((bar.mark, bar.words.as_str(), bar.action), (Mark::Failed, "0.2.0 was not installed: the new copy could not be put in place", None));
        let about = updates.view().about;
        assert_eq!(
            about.help[0],
            HelpLine { text: "Restart to update did not install 0.2.0: the new copy could not be put in place. You still have 0.1.0.".into(), failed: true }
        );

        updates.poll(at(9, 0));
        answer(&log, ServiceEvent::Checked(Ok(release())));
        updates.poll(at(9, 0));
        let view = updates.view();
        assert_eq!(view.bar.as_ref().map(|bar| (bar.mark, bar.action)), Some((Mark::Failed, Some(BarAction::Retry))), "the release is known again");
        let notes = view.notes.unwrap();
        assert_eq!(notes.action, Some(NotesAction { action: BarAction::Retry, label: "Retry".into() }));
        assert_eq!(notes.problem.as_deref(), Some("0.2.0 was not installed: the new copy could not be put in place"));
        let buttons: Vec<_> = view.about.buttons.iter().map(|button| button.action).collect();
        assert_eq!(buttons, [AboutAction::WhatsNew, AboutAction::Bar(BarAction::Retry)]);

        updates.download();
        assert_eq!(calls(&log), ["check", "download"]);
        assert_eq!(updates.view().bar.unwrap().words, "Downloading 0.2.0");
        updates.cancel();
        assert_eq!(updates.view().bar.unwrap().words, "Jobdesk 0.2.0 is out", "the failure is not said again");
    }

    #[test]
    fn a_restart_failure_of_a_release_no_newer_than_this_copy_is_dropped() {
        let (mut updates, _log) = desk_of("0.2.0", None, Some(Platform::MacArm));
        updates.apply_failed(not_installed());
        assert_eq!(updates.view().bar, None, "the release went in after all");

        let (mut updates, log) = desk(None);
        updates.apply_failed(not_installed());
        updates.poll(at(9, 0));
        answer(&log, ServiceEvent::Checked(Ok(Release { version: Version { major: 0, minor: 1, patch: 0 }, ..release() })));
        updates.poll(at(9, 0));
        assert_eq!(updates.view().bar, None, "the release is gone");
        assert!(!updates.view().about.help.iter().any(|line| line.failed));
    }

    #[test]
    fn the_notes_are_named_for_a_screen_reader() {
        let (updates, _log) = found();
        assert_eq!(updates.view().notes.unwrap().notes_label, "What changed in 0.2.0");
    }

    #[test]
    fn the_restart_plan_carries_the_signature_of_the_release() {
        let (updates, _log) = ready();
        assert_eq!(updates.restart_plan().unwrap().signature, "sig");
    }

    #[test]
    fn a_release_that_is_withdrawn_takes_its_ready_download_with_it() {
        let (mut updates, log) = ready();
        updates.check_now(at(15, 0));
        answer(&log, ServiceEvent::Checked(Ok(Release { version: Version { major: 0, minor: 1, patch: 0 }, ..release() })));
        updates.poll(at(15, 0));
        assert_eq!(calls(&log).last(), Some(&"cancel"), "the service is asked to delete the file");
        assert_eq!(updates.restart_plan(), None);
        let view = updates.view();
        assert_eq!((view.bar, view.notes), (None, None));
        assert_eq!(view.about.tag, Some(("Up to date", Mark::Good)));
    }

    #[test]
    fn a_release_that_is_withdrawn_stops_its_running_download() {
        let (mut updates, log) = found();
        updates.download();
        updates.check_now(at(9, 30));
        answer(&log, ServiceEvent::Checked(Ok(Release { version: Version { major: 0, minor: 1, patch: 0 }, ..release() })));
        updates.poll(at(9, 30));
        assert_eq!(calls(&log), ["check", "download", "check", "cancel"]);
        assert_eq!(updates.view().bar, None);
        finish(&log, DownloadEnd::Done(PathBuf::from("/late")));
        updates.poll(at(9, 31));
        assert_eq!(updates.restart_plan(), None);
    }
}
