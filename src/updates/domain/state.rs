use std::path::PathBuf;
use std::time::Duration;

use chrono::NaiveDateTime;

use crate::updates::domain::{DownloadEnd, DownloadId, Failure, InstallBlock, Release, RestartPlan, Version};

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
    /// The number of the download that runs or ran last. Only events that carry it count.
    pub download_id: DownloadId,
}

impl UpdateState {
    pub fn new(current: Version, block: Option<InstallBlock>) -> Self {
        Self { current, check: Check::NotYet, download: Download::Idle, release: None, block, download_id: DownloadId::default() }
    }

    pub fn is_checking(&self) -> bool {
        matches!(self.check, Check::Checking { .. })
    }

    fn has_download(&self) -> bool {
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

    /// Takes the answer of a check made at `at`. A release that is not newer than this copy reads as up to date,
    /// and a download of an older find is dropped with it, because that release is gone. A newer release never
    /// replaces a download that is running or ready. Gives `true` when a running or ready download was dropped,
    /// so the caller asks the service to delete what it holds.
    pub fn checked(&mut self, result: Result<Release, Failure>, at: NaiveDateTime) -> bool {
        let manual = matches!(self.check, Check::Checking { manual: true });
        let release = match result {
            Ok(release) => release,
            Err(failure) => {
                self.check = Check::Failed { at, failure, manual };
                return false;
            }
        };
        let newer = release.version > self.current;
        self.check = if newer { Check::Found { at } } else { Check::UpToDate { at } };
        if !newer {
            let dropped = self.has_download();
            self.release = None;
            self.download = Download::Idle;
            return dropped;
        }
        if self.has_download() {
            return false;
        }
        // The same version again keeps a failed download as it was, so Retry stays what the user saw.
        if self.release.as_ref().is_none_or(|known| known.version != release.version) {
            self.download = Download::Idle;
        }
        self.release = Some(release);
        false
    }

    /// Whether Download or Retry may start: a newer release, nothing blocking it, and no download running or done.
    pub fn may_download(&self) -> bool {
        self.release.is_some() && self.block.is_none() && matches!(self.download, Download::Idle | Download::Failed { .. } | Download::BadSignature)
    }

    /// Starts the download and gives the release it is for with the new number of the download, or `None` when it may not start.
    pub fn start_download(&mut self) -> Option<(Release, DownloadId)> {
        if !self.may_download() {
            return None;
        }
        let release = self.release.clone()?;
        self.download_id = DownloadId(self.download_id.0 + 1);
        self.download = Download::Running { done: 0, total: release.size };
        Some((release, self.download_id))
    }

    /// Takes the progress of download `id`. It counts only for the download that runs now.
    pub fn progress(&mut self, id: DownloadId, done: u64, total: u64) {
        if id == self.download_id && matches!(self.download, Download::Running { .. }) {
            self.download = Download::Running { done, total };
        }
    }

    /// Takes how download `id` ended. Anything that comes when none runs, or from an older download, is stale and changes nothing.
    pub fn ended(&mut self, id: DownloadId, end: DownloadEnd) {
        if id != self.download_id || !matches!(self.download, Download::Running { .. }) {
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
        self.ended(self.download_id, DownloadEnd::Cancelled);
    }

    /// What Restart to update applies. Only a download that is ready has one.
    pub fn restart_plan(&self) -> Option<RestartPlan> {
        match (&self.download, &self.release) {
            (Download::Ready { file }, Some(release)) => {
                Some(RestartPlan { version: release.version, file: file.clone(), signature: release.signature.clone() })
            }
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
        Release { version: version(minor), released: None, notes: "Notes".into(), size: 1000, signature: "sig".into(), pin: None }
    }

    /// Reports the end of the download that runs now, as its service would.
    fn finish(state: &mut UpdateState, end: DownloadEnd) {
        let id = state.download_id;
        state.ended(id, end);
    }

    fn advance(state: &mut UpdateState, done: u64, total: u64) {
        let id = state.download_id;
        state.progress(id, done, total);
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
        finish(&mut state, DownloadEnd::Done(PathBuf::from("/data/updates/Jobdesk")));
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
        for block in [InstallBlock::ReadOnly, InstallBlock::NotInstalled, InstallBlock::NoKey, InstallBlock::NoBuild, InstallBlock::UnreadableVersion] {
            let mut blocked = UpdateState::new(version(1), Some(block));
            blocked.checked(Ok(release(2)), at(9, 0));
            assert!(!blocked.may_download(), "{block:?}");
            assert_eq!(blocked.start_download(), None);
            assert_eq!(blocked.download, Download::Idle);
        }
        let mut state = found();
        assert_eq!(state.start_download(), Some((release(2), DownloadId(1))));
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
        advance(&mut state, 400, 1000);
        assert_eq!(state.restart_plan(), None, "running with progress");
        finish(&mut state, DownloadEnd::Done(PathBuf::from("/data/updates/Jobdesk")));
        assert_eq!(state.restart_plan(), Some(RestartPlan { version: version(2), file: PathBuf::from("/data/updates/Jobdesk"), signature: "sig".into() }));
        for end in [DownloadEnd::BadSignature, DownloadEnd::Failed { percent: 3, failure: Failure::Disk }, DownloadEnd::Cancelled] {
            let mut other = found();
            other.start_download();
            finish(&mut other, end);
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
        advance(&mut state, 10, 1000);
        state.checked(Ok(release(3)), at(15, 0));
        assert_eq!(state.release, Some(release(2)));
        assert_eq!(state.download, Download::Running { done: 10, total: 1000 });
    }

    #[test]
    fn a_check_that_finds_nothing_newer_drops_a_ready_download() {
        let mut state = ready();
        assert!(state.checked(Ok(release(1)), at(15, 0)), "the caller is told to delete the file");
        assert_eq!((&state.release, &state.download, &state.check), (&None, &Download::Idle, &Check::UpToDate { at: at(15, 0) }));
        assert_eq!(state.restart_plan(), None);
        assert!(!state.may_download());
    }

    #[test]
    fn a_check_that_finds_nothing_newer_drops_a_running_download() {
        let mut state = found();
        state.start_download();
        advance(&mut state, 10, 1000);
        assert!(state.checked(Ok(release(1)), at(15, 0)));
        assert_eq!((&state.release, &state.download), (&None, &Download::Idle));
        let late = state.download_id;
        state.ended(late, DownloadEnd::Done(PathBuf::from("/late")));
        assert_eq!(state.restart_plan(), None, "what the dropped download reports late changes nothing");
    }

    #[test]
    fn a_check_tells_nothing_to_delete_when_no_download_was_dropped() {
        let mut state = found();
        assert!(!state.checked(Ok(release(1)), at(15, 0)));
        assert!(!state.checked(Ok(release(2)), at(21, 0)));
        let mut kept = ready();
        assert!(!kept.checked(Ok(release(3)), at(15, 0)), "a newer find keeps the download");
        assert!(!kept.checked(Err(Failure::TimedOut), at(16, 0)), "a failed check keeps it too");
        assert!(matches!(kept.download, Download::Ready { .. }));
    }

    #[test]
    fn a_newer_release_replaces_a_failed_download() {
        let mut state = found();
        state.start_download();
        finish(&mut state, DownloadEnd::Failed { percent: 41, failure: Failure::Interrupted });
        state.checked(Ok(release(2)), at(15, 0));
        assert_eq!(state.download, Download::Failed { percent: 41, failure: Failure::Interrupted }, "the same version keeps what the user saw");
        state.checked(Ok(release(3)), at(21, 0));
        assert_eq!((state.release, state.download), (Some(release(3)), Download::Idle));
    }

    #[test]
    fn cancel_returns_to_idle_and_a_late_end_changes_nothing() {
        let mut state = found();
        state.start_download();
        advance(&mut state, 300, 1000);
        state.cancel();
        assert_eq!(state.download, Download::Idle);
        finish(&mut state, DownloadEnd::Done(PathBuf::from("/late")));
        advance(&mut state, 900, 1000);
        assert_eq!(state.download, Download::Idle);
        assert!(state.may_download());
    }

    #[test]
    fn each_download_has_its_own_number() {
        let mut state = found();
        assert_eq!(state.start_download().map(|(_, id)| id), Some(DownloadId(1)));
        state.cancel();
        assert_eq!(state.start_download().map(|(_, id)| id), Some(DownloadId(2)));
    }

    #[test]
    fn a_late_event_of_a_cancelled_download_changes_nothing_in_the_next() {
        let old_file = PathBuf::from("/data/updates/Jobdesk-old");
        for late in
            [DownloadEnd::Cancelled, DownloadEnd::Done(old_file.clone()), DownloadEnd::BadSignature, DownloadEnd::Failed { percent: 9, failure: Failure::Disk }]
        {
            let mut state = found();
            let (_, old) = state.start_download().unwrap();
            state.cancel();
            let (_, new) = state.start_download().unwrap();
            assert_ne!(old, new);
            state.ended(old, late.clone());
            state.progress(old, 999, 1000);
            assert_eq!(state.download, Download::Running { done: 0, total: 1000 }, "{late:?}");
            assert_eq!(state.restart_plan(), None, "{late:?}");
            state.progress(new, 5, 1000);
            assert_eq!(state.download, Download::Running { done: 5, total: 1000 });
            state.ended(new, DownloadEnd::Done(PathBuf::from("/data/updates/Jobdesk-new")));
            assert_eq!(state.restart_plan().map(|plan| plan.file), Some(PathBuf::from("/data/updates/Jobdesk-new")));
        }
    }

    #[test]
    fn a_late_done_of_an_older_version_never_makes_a_plan_for_the_newer() {
        let mut state = found();
        let (_, old) = state.start_download().unwrap();
        state.cancel();
        state.checked(Ok(release(3)), at(15, 0));
        state.start_download().unwrap();
        state.ended(old, DownloadEnd::Done(PathBuf::from("/data/updates/Jobdesk-0.2.0")));
        assert_eq!(state.restart_plan(), None);
        assert_eq!(state.release, Some(release(3)));
    }

    #[test]
    fn the_restart_plan_carries_the_signature_of_its_release() {
        let mut state = state();
        state.checked(Ok(Release { signature: "sig-of-0.2.0".into(), ..release(2) }), at(9, 0));
        state.start_download();
        finish(&mut state, DownloadEnd::Done(PathBuf::from("/data/updates/Jobdesk")));
        assert_eq!(state.restart_plan().unwrap().signature, "sig-of-0.2.0");
    }

    #[test]
    fn a_bad_signature_and_a_failed_download_may_be_retried() {
        let mut state = found();
        state.start_download();
        finish(&mut state, DownloadEnd::BadSignature);
        assert_eq!(state.download, Download::BadSignature);
        assert!(state.may_download());
        assert!(state.start_download().is_some());
        finish(&mut state, DownloadEnd::Failed { percent: 41, failure: Failure::Interrupted });
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
        advance(&mut state, 5, 10);
        assert_eq!(state.download, Download::Idle);
        state.start_download();
        advance(&mut state, 5, 10);
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
