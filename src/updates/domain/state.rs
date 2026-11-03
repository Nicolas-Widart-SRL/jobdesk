use std::path::PathBuf;
use std::time::Duration;

use chrono::NaiveDateTime;

use crate::updates::domain::{DownloadEnd, Failure, InstallBlock, Release, RestartPlan, Version};

/// How long after one check an automatic check is due again.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Whether an automatic check is due: switched on, and never made, or last made `CHECK_EVERY` ago.
/// A last attempt in the future, after the clock moved back, counts as due.
pub fn due(last_attempt: Option<NaiveDateTime>, now: NaiveDateTime, auto: bool) -> bool {
    auto && last_attempt.is_none_or(|last| (now - last).to_std().map_or(true, |since| since >= CHECK_EVERY))
}

/// The share of `total` that `done` is, in whole percent and never past 100.
pub fn percent(done: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    (u128::from(done.min(total)) * 100 / u128::from(total)) as u8
}

/// The last thing asked of the feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    NotYet,
    /// `manual` when the user pressed Check now, so a failure is theirs to see.
    Checking {
        manual: bool,
    },
    UpToDate {
        at: NaiveDateTime,
    },
    Failed {
        at: NaiveDateTime,
        failure: Failure,
        manual: bool,
    },
    /// A newer release was found.
    Found {
        at: NaiveDateTime,
    },
}

/// The download of the release in `UpdateState::release`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Download {
    Idle,
    Running {
        done: u64,
        total: u64,
    },
    Failed {
        percent: u8,
        failure: Failure,
    },
    BadSignature,
    /// Downloaded, and its signature checked.
    Ready {
        file: PathBuf,
    },
}

/// Where an update stands. Every change goes through a method here, so the rules sit in one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateState {
    pub current: Version,
    pub check: Check,
    pub download: Download,
    /// The newer release, once one was found.
    pub release: Option<Release>,
    pub block: Option<InstallBlock>,
}

impl UpdateState {
    pub fn new(current: Version, block: Option<InstallBlock>) -> Self {
        Self { current, check: Check::NotYet, download: Download::Idle, release: None, block }
    }

    pub fn is_checking(&self) -> bool {
        matches!(self.check, Check::Checking { .. })
    }

    /// A download that is running or done is kept whatever a later check finds.
    fn keeps_download(&self) -> bool {
        matches!(self.download, Download::Running { .. } | Download::Ready { .. })
    }

    /// Marks a check as started. One already running is not started again, but a press of Check now makes it the user's.
    pub fn start_check(&mut self, manual: bool) -> bool {
        match &mut self.check {
            Check::Checking { manual: held } => {
                *held |= manual;
                false
            }
            check => {
                *check = Check::Checking { manual };
                true
            }
        }
    }

    /// Takes the answer of a check made at `at`. A release that is not newer than this copy reads as up to date.
    /// A newer one never replaces a download that is running or ready.
    pub fn checked(&mut self, result: Result<Release, Failure>, at: NaiveDateTime) {
        let manual = matches!(self.check, Check::Checking { manual: true });
        let release = match result {
            Ok(release) => release,
            Err(failure) => {
                self.check = Check::Failed { at, failure, manual };
                return;
            }
        };
        let newer = release.version > self.current;
        self.check = if newer { Check::Found { at } } else { Check::UpToDate { at } };
        if self.keeps_download() {
            return;
        }
        if !newer {
            self.release = None;
            self.download = Download::Idle;
            return;
        }
        // The same version again keeps a failed download as it was, so Retry stays what the user saw.
        if self.release.as_ref().is_none_or(|known| known.version != release.version) {
            self.download = Download::Idle;
        }
        self.release = Some(release);
    }

    /// Whether Download or Retry may start: a newer release, nothing blocking it, and no download running or done.
    pub fn may_download(&self) -> bool {
        self.release.is_some() && self.block.is_none() && matches!(self.download, Download::Idle | Download::Failed { .. } | Download::BadSignature)
    }

    /// Starts the download and gives the release it is for, or `None` when it may not start.
    pub fn start_download(&mut self) -> Option<Release> {
        if !self.may_download() {
            return None;
        }
        let release = self.release.clone()?;
        self.download = Download::Running { done: 0, total: release.size };
        Some(release)
    }

    pub fn progress(&mut self, done: u64, total: u64) {
        if let Download::Running { .. } = self.download {
            self.download = Download::Running { done, total };
        }
    }

    /// Takes how a running download ended. Anything that comes when none runs is stale, and changes nothing.
    pub fn ended(&mut self, end: DownloadEnd) {
        if !matches!(self.download, Download::Running { .. }) {
            return;
        }
        self.download = match end {
            DownloadEnd::Done(file) => Download::Ready { file },
            DownloadEnd::Failed { percent, failure } => Download::Failed { percent, failure },
            DownloadEnd::BadSignature => Download::BadSignature,
            DownloadEnd::Cancelled => Download::Idle,
        };
    }

    /// Back to Idle at once, so Cancel needs no answer from the service.
    pub fn cancel(&mut self) {
        self.ended(DownloadEnd::Cancelled);
    }

    /// What Restart to update applies. Only a download that is ready has one.
    pub fn restart_plan(&self) -> Option<RestartPlan> {
        match (&self.download, &self.release) {
            (Download::Ready { file }, Some(release)) => Some(RestartPlan { version: release.version, file: file.clone() }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 3).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn version(minor: u32) -> Version {
        Version { major: 0, minor, patch: 0 }
    }

    fn release(minor: u32) -> Release {
        Release { version: version(minor), released: None, notes: "Notes".into(), size: 1000, signature: "sig".into() }
    }

    fn state() -> UpdateState {
        UpdateState::new(version(1), None)
    }

    fn found() -> UpdateState {
        let mut state = state();
        state.checked(Ok(release(2)), at(9, 0));
        state
    }

    fn ready() -> UpdateState {
        let mut state = found();
        state.start_download();
        state.ended(DownloadEnd::Done(PathBuf::from("/data/updates/Jobdesk")));
        state
    }

    #[test]
    fn a_newer_release_is_found_and_may_be_downloaded() {
        let state = found();
        assert_eq!(state.check, Check::Found { at: at(9, 0) });
        assert_eq!(state.release, Some(release(2)));
        assert!(state.may_download());
    }

    #[test]
    fn an_equal_or_older_release_is_up_to_date() {
        for minor in [1, 0] {
            let mut state = state();
            state.checked(Ok(release(minor)), at(9, 0));
            assert_eq!(state.check, Check::UpToDate { at: at(9, 0) }, "0.{minor}.0");
            assert_eq!(state.release, None);
            assert!(!state.may_download());
        }
    }

    #[test]
    fn a_release_that_is_no_longer_the_latest_is_forgotten() {
        let mut state = found();
        state.checked(Ok(release(1)), at(15, 0));
        assert_eq!((state.release, state.check), (None, Check::UpToDate { at: at(15, 0) }));
    }

    #[test]
    fn a_download_is_refused_while_blocked_or_running() {
        for block in [InstallBlock::ReadOnly, InstallBlock::NotInstalled, InstallBlock::NoKey] {
            let mut blocked = UpdateState::new(version(1), Some(block));
            blocked.checked(Ok(release(2)), at(9, 0));
            assert!(!blocked.may_download(), "{block:?}");
            assert_eq!(blocked.start_download(), None);
            assert_eq!(blocked.download, Download::Idle);
        }
        let mut state = found();
        assert_eq!(state.start_download(), Some(release(2)));
        assert_eq!(state.download, Download::Running { done: 0, total: 1000 });
        assert!(!state.may_download());
        assert_eq!(state.start_download(), None);
    }

    #[test]
    fn with_nothing_found_there_is_nothing_to_download() {
        let mut state = state();
        assert!(!state.may_download());
        assert_eq!(state.start_download(), None);
    }

    #[test]
    fn restart_has_a_plan_only_from_ready() {
        let mut state = found();
        assert_eq!(state.restart_plan(), None);
        state.start_download();
        assert_eq!(state.restart_plan(), None, "running");
        state.progress(400, 1000);
        assert_eq!(state.restart_plan(), None, "running with progress");
        state.ended(DownloadEnd::Done(PathBuf::from("/data/updates/Jobdesk")));
        assert_eq!(state.restart_plan(), Some(RestartPlan { version: version(2), file: PathBuf::from("/data/updates/Jobdesk") }));
        for end in [DownloadEnd::BadSignature, DownloadEnd::Failed { percent: 3, failure: Failure::Disk }, DownloadEnd::Cancelled] {
            let mut other = found();
            other.start_download();
            other.ended(end);
            assert_eq!(other.restart_plan(), None);
        }
    }

    #[test]
    fn a_newer_release_keeps_a_download_that_is_ready_or_running() {
        let mut state = ready();
        state.checked(Ok(release(3)), at(15, 0));
        assert_eq!(state.release, Some(release(2)));
        assert!(matches!(state.download, Download::Ready { .. }));
        assert_eq!(state.check, Check::Found { at: at(15, 0) });

        let mut state = found();
        state.start_download();
        state.progress(10, 1000);
        state.checked(Ok(release(3)), at(15, 0));
        assert_eq!(state.release, Some(release(2)));
        assert_eq!(state.download, Download::Running { done: 10, total: 1000 });
    }

    #[test]
    fn a_newer_release_replaces_a_failed_download() {
        let mut state = found();
        state.start_download();
        state.ended(DownloadEnd::Failed { percent: 41, failure: Failure::Interrupted });
        state.checked(Ok(release(2)), at(15, 0));
        assert_eq!(state.download, Download::Failed { percent: 41, failure: Failure::Interrupted }, "the same version keeps what the user saw");
        state.checked(Ok(release(3)), at(21, 0));
        assert_eq!((state.release, state.download), (Some(release(3)), Download::Idle));
    }

    #[test]
    fn cancel_returns_to_idle_and_a_late_end_changes_nothing() {
        let mut state = found();
        state.start_download();
        state.progress(300, 1000);
        state.cancel();
        assert_eq!(state.download, Download::Idle);
        state.ended(DownloadEnd::Done(PathBuf::from("/late")));
        state.progress(900, 1000);
        assert_eq!(state.download, Download::Idle);
        assert!(state.may_download());
    }

    #[test]
    fn a_bad_signature_and_a_failed_download_may_be_retried() {
        let mut state = found();
        state.start_download();
        state.ended(DownloadEnd::BadSignature);
        assert_eq!(state.download, Download::BadSignature);
        assert!(state.may_download());
        assert!(state.start_download().is_some());
        state.ended(DownloadEnd::Failed { percent: 41, failure: Failure::Interrupted });
        assert!(state.may_download());
        assert!(state.start_download().is_some());
        assert_eq!(state.download, Download::Running { done: 0, total: 1000 });
    }

    #[test]
    fn a_failed_check_keeps_the_manual_flag_and_changes_no_release() {
        let mut state = found();
        assert!(state.start_check(true));
        state.checked(Err(Failure::TimedOut), at(15, 0));
        assert_eq!(state.check, Check::Failed { at: at(15, 0), failure: Failure::TimedOut, manual: true });
        assert_eq!(state.release, Some(release(2)));
        let mut state = self::state();
        state.start_check(false);
        state.checked(Err(Failure::BadFeed), at(15, 0));
        assert_eq!(state.check, Check::Failed { at: at(15, 0), failure: Failure::BadFeed, manual: false });
    }

    #[test]
    fn a_check_runs_once_at_a_time_and_a_press_makes_it_the_users() {
        let mut state = state();
        assert!(state.start_check(false));
        assert!(!state.start_check(true));
        assert_eq!(state.check, Check::Checking { manual: true });
        assert!(!state.start_check(false));
        assert_eq!(state.check, Check::Checking { manual: true }, "an automatic start does not take it back");
    }

    #[test]
    fn progress_is_kept_only_while_running() {
        let mut state = found();
        state.progress(5, 10);
        assert_eq!(state.download, Download::Idle);
        state.start_download();
        state.progress(5, 10);
        assert_eq!(state.download, Download::Running { done: 5, total: 10 });
    }

    #[test]
    fn percent_stays_between_zero_and_a_hundred() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(5_400_000, 14_200_000), 38);
        assert_eq!(percent(14_200_000, 14_200_000), 100);
        assert_eq!(percent(20, 10), 100);
        assert_eq!(percent(u64::MAX, u64::MAX), 100);
    }

    #[test]
    fn a_check_is_due_when_never_made() {
        assert!(due(None, at(9, 0), true));
    }

    #[test]
    fn a_check_is_due_six_hours_after_the_last() {
        assert!(!due(Some(at(9, 0)), at(14, 59), true));
        assert!(due(Some(at(9, 0)), at(15, 0), true));
        assert!(due(Some(at(9, 0)), at(23, 0), true));
        assert!(!due(Some(at(9, 0)), at(9, 0), true));
    }

    #[test]
    fn a_check_is_never_due_with_the_switch_off() {
        assert!(!due(None, at(9, 0), false));
        assert!(!due(Some(at(1, 0)), at(23, 0), false));
    }

    #[test]
    fn a_last_attempt_in_the_future_is_due_again() {
        assert!(due(Some(at(20, 0)), at(9, 0), true));
    }
}
