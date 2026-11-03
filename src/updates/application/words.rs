//! Every word the update group, the release notes and Settings › About say. Nothing from the network is in them but numbers.

use chrono::{NaiveDate, NaiveDateTime};

use crate::updates::domain::{ApplyFailure, Failure, InstallBlock, Platform, Version, percent};

/// Settings › About, with the switch on and off.
pub const AUTO_ON: &str = "When Jobdesk opens and about every 6 hours, it reads the latest release on GitHub. It downloads nothing until you choose Download.";
pub const AUTO_OFF: &str = "Off. Jobdesk looks for a new version only when you choose Check now.";
pub const NOT_CHECKED: &str = "Not checked yet.";
pub const CHECKING: &str = "Asking GitHub for the latest release…";
pub const CHECKING_WORD: &str = "Checking…";
pub const UP_TO_DATE: &str = "Up to date";
pub const CHECK_FAILED: &str = "Check failed";
pub const WHATS_NEW: &str = "What's new";
pub const CHECK_NOW: &str = "Check now";
pub const RELEASE_PAGE: &str = "Open the release page";
pub const RESTART: &str = "Restart to update";

/// Under the release notes.
pub const TRUST_LINE: &str = "From the GitHub release. Its links and images do not open here.";
pub const NOTES_FOOT: &str = "Nothing installs until you choose Restart to update. Esc closes.";
/// The foot beside the actions, when a slip above them already says what happens.
pub const ESC_CLOSES: &str = "Esc closes";

/// The day of a day or a time, as the notes and About say it: `Fri 30 Oct`.
fn day(date: NaiveDate) -> String {
    date.format("%a %-d %b").to_string()
}

/// `today at 09:12`, `yesterday at 09:12`, or `Fri 30 Oct at 09:12`.
fn when(at: NaiveDateTime, now: NaiveDateTime) -> String {
    let clock = at.format("%H:%M");
    match (now.date() - at.date()).num_days() {
        0 => format!("today at {clock}"),
        1 => format!("yesterday at {clock}"),
        _ => format!("{} at {clock}", day(at.date())),
    }
}

pub fn bar_available(version: Version) -> String {
    format!("Jobdesk {version} is out")
}

pub fn bar_downloading(version: Version) -> String {
    format!("Downloading {version}")
}

pub fn bar_ready(version: Version) -> String {
    format!("{version} is downloaded and checked")
}

pub fn bar_failed(version: Version, percent: u8, failure: Failure) -> String {
    format!("Download of {version} stopped at {percent}%: {}", failure_reason(failure))
}

pub fn bar_bad_signature(version: Version) -> String {
    format!("{version} failed its signature check. It was deleted, not installed.")
}

/// A size in decimal megabytes with one decimal: `14.2`.
pub fn megabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_000_000.0)
}

pub fn share(done: u64, total: u64) -> String {
    format!("{}%", percent(done, total))
}

/// The size of a build: `14.2 MB`.
pub fn size_words(bytes: u64) -> String {
    format!("{} MB", megabytes(bytes))
}

/// The size of a download so far: `5.4 of 14.2 MB`.
pub fn progress_size(done: u64, total: u64) -> String {
    format!("{} of {} MB", megabytes(done.min(total)), megabytes(total))
}

pub fn notes_title(version: Version) -> String {
    format!("Jobdesk {version}")
}

pub fn notes_meta(current: Version, released: Option<NaiveDate>) -> String {
    match released {
        Some(date) => format!("Released {}. You have {current}.", day(date)),
        None => format!("You have {current}."),
    }
}

pub fn notes_download(version: Version) -> String {
    format!("Download {version}")
}

/// What a screen reader calls the notes of `version`.
pub fn notes_label(version: Version) -> String {
    format!("What changed in {version}")
}

/// Why Restart to update did not install a release, to follow a colon. Jobdesk's own words.
pub fn apply_failure_reason(failure: ApplyFailure) -> &'static str {
    match failure {
        ApplyFailure::Missing => "the downloaded file was gone",
        ApplyFailure::BadSignature => "the downloaded file no longer matched its signature",
        ApplyFailure::NotInstalled => "this copy was not installed by a Jobdesk installer",
        ApplyFailure::Replace => "the new copy could not be put in place",
        ApplyFailure::Start => "the new copy did not start",
    }
}

/// The bar after a Restart to update that did not install `version`: `0.2.0 was not installed: the new copy could not
/// be put in place`.
pub fn bar_apply_failed(version: Version, failure: ApplyFailure) -> String {
    format!("{version} was not installed: {}", apply_failure_reason(failure))
}

/// About's line for the same, with the version that runs instead.
pub fn about_apply_failed(version: Version, current: Version, failure: ApplyFailure) -> String {
    format!("Restart to update did not install {version}: {}. You still have {current}.", apply_failure_reason(failure))
}

/// What Restart to update does, on the notes' slip once a download is ready.
pub fn ready_slip(platform: Platform, version: Version) -> String {
    match platform {
        Platform::MacArm => format!("Jobdesk closes, installs {version} and opens again with the same folder and settings. Restart waits while a task is open."),
        Platform::WindowsX64 => {
            "Jobdesk closes, the installer shows its progress for a moment, then Jobdesk opens again with the same folder and settings. Restart waits while a task is open."
                .to_owned()
        }
    }
}

/// Why this copy cannot install `version` by itself, and where to get it when the release page has a build for it.
pub fn block_words(block: InstallBlock, version: Version) -> String {
    let get = format!("Install {version} from the release page.");
    match block {
        InstallBlock::ReadOnly => format!("Jobdesk cannot replace itself where it is installed. {get}"),
        InstallBlock::NotInstalled => format!("This copy was not installed by a Jobdesk installer, so it cannot update itself. {get}"),
        InstallBlock::NoKey => format!("This build has no update key, so it cannot check a download. {get}"),
        InstallBlock::NoBuild => "Jobdesk publishes no build for this computer, so it cannot update itself.".to_owned(),
        InstallBlock::UnreadableVersion => "This copy has no version number Jobdesk can read, so it cannot tell whether a newer one is out.".to_owned(),
    }
}

/// Whether the release page has a build this copy could be replaced with.
pub fn release_page_helps(block: InstallBlock) -> bool {
    !matches!(block, InstallBlock::NoBuild | InstallBlock::UnreadableVersion)
}

/// The word beside About on the settings rail when this copy has no readable version.
pub const RAIL_UNREADABLE: &str = "Version unknown";

/// What a Claude session open in Tasks means for Restart to update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    None,
    /// A run is working.
    Working,
    /// A session only waits for the user's reply.
    Open,
}

/// Why Restart to update is disabled, or `None` when it may go.
pub fn restart_wait(session: SessionState) -> Option<&'static str> {
    match session {
        SessionState::None => None,
        SessionState::Working => Some("A task is running. Restart once it finishes, so it is not cut off halfway."),
        SessionState::Open => Some("A task waits for your reply in Tasks. End its session there, then restart."),
    }
}

/// Why a check or a download failed, to follow a colon. Jobdesk's own words: nothing the server said.
pub fn failure_reason(failure: Failure) -> String {
    match failure {
        Failure::TimedOut => "the connection timed out".to_owned(),
        Failure::Unreachable => "there is no connection to GitHub".to_owned(),
        Failure::Refused(code) => format!("GitHub answered with the error {code}"),
        Failure::BadFeed => "the release information could not be read".to_owned(),
        Failure::NoBuild => "this release has no build for this computer".to_owned(),
        Failure::TooLarge => "the file was larger than the release said".to_owned(),
        Failure::Interrupted => "the connection dropped".to_owned(),
        Failure::Disk => "the file could not be saved on this computer".to_owned(),
    }
}

/// Whether the failure is about reaching GitHub at all.
fn unreached(failure: Failure) -> bool {
    matches!(failure, Failure::TimedOut | Failure::Unreachable | Failure::Interrupted)
}

pub fn last_checked(at: NaiveDateTime, now: NaiveDateTime) -> String {
    format!("Last checked {}.", when(at, now))
}

/// The first line of About's Latest version after a failed check: what went wrong and when.
pub fn check_failed_line(failure: Failure, at: NaiveDateTime, now: NaiveDateTime) -> String {
    let moment = match (now.date() - at.date()).num_days() {
        0 => format!("at {}", at.format("%H:%M")),
        _ => when(at, now),
    };
    let lead = if unreached(failure) { "Could not reach GitHub" } else { "The check failed" };
    format!("{lead} {moment}: {}.", failure_reason(failure))
}

/// The second line, which says what happens next.
pub fn check_failed_next(auto: bool) -> &'static str {
    if auto {
        "Jobdesk tries again in about 6 hours, and nothing changed on this computer."
    } else {
        "Jobdesk looks again only when you choose Check now, and nothing changed on this computer."
    }
}

/// About's Latest version once a newer release is known: `Jobdesk 0.2.0, released Fri 30 Oct. You have 0.1.0.`
pub fn about_update(version: Version, current: Version, released: Option<NaiveDate>) -> String {
    match released {
        Some(date) => format!("Jobdesk {version}, released {}. You have {current}.", day(date)),
        None => format!("Jobdesk {version}. You have {current}."),
    }
}

pub fn about_downloading(version: Version, done: u64, total: u64) -> String {
    format!("Downloading {version}: {}, {}.", share(done, total), progress_size(done, total))
}

pub fn about_ready(version: Version) -> String {
    format!(
        "Jobdesk {version} is downloaded and its signature checked. Restart closes Jobdesk, installs it and opens it again with the same folder and settings."
    )
}

pub fn about_download_failed(version: Version, percent: u8, failure: Failure) -> String {
    format!("Download of {version} stopped at {percent}%: {}.", failure_reason(failure))
}

pub fn about_bad_signature(version: Version) -> String {
    format!("{version} failed its signature check. It was deleted, not installed.")
}

/// The word beside About on the settings rail.
pub fn rail_available(version: Version) -> String {
    format!("{version} is out")
}

pub fn rail_ready(version: Version) -> String {
    format!("{version} ready")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(minor: u32) -> Version {
        Version { major: 0, minor, patch: 0 }
    }

    fn moment(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    #[test]
    fn the_bar_says_each_state_in_the_designed_words() {
        assert_eq!(bar_available(v(2)), "Jobdesk 0.2.0 is out");
        assert_eq!(bar_downloading(v(2)), "Downloading 0.2.0");
        assert_eq!(bar_ready(v(2)), "0.2.0 is downloaded and checked");
        assert_eq!(bar_failed(v(2), 41, Failure::Interrupted), "Download of 0.2.0 stopped at 41%: the connection dropped");
        assert_eq!(bar_bad_signature(v(2)), "0.2.0 failed its signature check. It was deleted, not installed.");
    }

    #[test]
    fn a_download_reads_as_a_share_and_a_size() {
        assert_eq!(share(5_400_000, 14_200_000), "38%");
        assert_eq!(progress_size(5_400_000, 14_200_000), "5.4 of 14.2 MB");
        assert_eq!(size_words(14_200_000), "14.2 MB");
        assert_eq!(progress_size(20, 10), "0.0 of 0.0 MB");
    }

    #[test]
    fn the_notes_head_says_the_day_and_the_version_in_use() {
        let released = NaiveDate::from_ymd_opt(2026, 10, 30);
        assert_eq!(notes_title(v(2)), "Jobdesk 0.2.0");
        assert_eq!(notes_meta(v(1), released), "Released Fri 30 Oct. You have 0.1.0.");
        assert_eq!(notes_meta(v(1), None), "You have 0.1.0.");
        assert_eq!(notes_download(v(2)), "Download 0.2.0");
    }

    #[test]
    fn the_ready_slip_says_what_restart_does_on_each_platform() {
        assert_eq!(
            ready_slip(Platform::MacArm, v(2)),
            "Jobdesk closes, installs 0.2.0 and opens again with the same folder and settings. Restart waits while a task is open."
        );
        let windows = ready_slip(Platform::WindowsX64, v(2));
        assert!(windows.starts_with("Jobdesk closes, the installer shows its progress for a moment, then Jobdesk opens again"));
        assert!(windows.ends_with("Restart waits while a task is open."));
    }

    #[test]
    fn restart_waits_with_a_reason_for_each_kind_of_session() {
        assert_eq!(restart_wait(SessionState::None), None);
        assert_eq!(restart_wait(SessionState::Working), Some("A task is running. Restart once it finishes, so it is not cut off halfway."));
        assert_eq!(restart_wait(SessionState::Open), Some("A task waits for your reply in Tasks. End its session there, then restart."));
    }

    #[test]
    fn a_blocked_copy_says_why_and_where_to_go() {
        assert_eq!(block_words(InstallBlock::ReadOnly, v(2)), "Jobdesk cannot replace itself where it is installed. Install 0.2.0 from the release page.");
        assert!(block_words(InstallBlock::NotInstalled, v(2)).starts_with("This copy was not installed by a Jobdesk installer, so it cannot update itself."));
        assert!(block_words(InstallBlock::NoKey, v(2)).starts_with("This build has no update key, so it cannot check a download."));
        assert!(block_words(InstallBlock::NoKey, v(2)).ends_with("Install 0.2.0 from the release page."));
    }

    #[test]
    fn a_copy_with_no_build_or_no_version_gives_no_advice_it_cannot_follow() {
        let no_build = block_words(InstallBlock::NoBuild, v(2));
        assert_eq!(no_build, "Jobdesk publishes no build for this computer, so it cannot update itself.");
        let unreadable = block_words(InstallBlock::UnreadableVersion, v(2));
        assert_eq!(unreadable, "This copy has no version number Jobdesk can read, so it cannot tell whether a newer one is out.");
        for words in [&no_build, &unreadable] {
            assert!(!words.contains("release page") && !words.contains("0.2.0"), "{words}");
        }
        for block in [InstallBlock::ReadOnly, InstallBlock::NotInstalled, InstallBlock::NoKey] {
            assert!(release_page_helps(block), "{block:?}");
        }
        assert!(!release_page_helps(InstallBlock::NoBuild) && !release_page_helps(InstallBlock::UnreadableVersion));
    }

    #[test]
    fn a_failure_is_said_in_jobdesks_own_words() {
        for failure in [
            Failure::TimedOut,
            Failure::Unreachable,
            Failure::Refused(503),
            Failure::BadFeed,
            Failure::NoBuild,
            Failure::TooLarge,
            Failure::Interrupted,
            Failure::Disk,
        ] {
            let reason = failure_reason(failure);
            assert!(!reason.is_empty() && !reason.ends_with('.'), "{failure:?}: {reason}");
        }
        assert_eq!(failure_reason(Failure::Refused(503)), "GitHub answered with the error 503");
    }

    #[test]
    fn an_update_that_did_not_install_says_why_in_jobdesks_own_words() {
        for failure in [ApplyFailure::Missing, ApplyFailure::BadSignature, ApplyFailure::NotInstalled, ApplyFailure::Replace, ApplyFailure::Start] {
            let reason = apply_failure_reason(failure);
            assert!(!reason.is_empty() && !reason.ends_with('.'), "{failure:?}: {reason}");
        }
        assert_eq!(bar_apply_failed(v(2), ApplyFailure::Replace), "0.2.0 was not installed: the new copy could not be put in place");
        assert_eq!(
            about_apply_failed(v(2), v(1), ApplyFailure::Start),
            "Restart to update did not install 0.2.0: the new copy did not start. You still have 0.1.0."
        );
        assert_eq!(notes_label(v(2)), "What changed in 0.2.0");
    }

    #[test]
    fn a_check_is_dated_by_the_day_it_ran() {
        let now = moment(3, 10, 0);
        assert_eq!(last_checked(moment(3, 9, 12), now), "Last checked today at 09:12.");
        assert_eq!(last_checked(moment(2, 23, 59), now), "Last checked yesterday at 23:59.");
        assert_eq!(last_checked(moment(1, 9, 12), now), "Last checked Sun 1 Nov at 09:12.");
    }

    #[test]
    fn a_failed_check_says_what_and_when() {
        let now = moment(3, 10, 0);
        assert_eq!(check_failed_line(Failure::TimedOut, moment(3, 9, 12), now), "Could not reach GitHub at 09:12: the connection timed out.");
        assert_eq!(check_failed_line(Failure::BadFeed, moment(3, 9, 12), now), "The check failed at 09:12: the release information could not be read.");
        assert_eq!(
            check_failed_line(Failure::Unreachable, moment(2, 9, 12), now),
            "Could not reach GitHub yesterday at 09:12: there is no connection to GitHub."
        );
        assert_eq!(check_failed_next(true), "Jobdesk tries again in about 6 hours, and nothing changed on this computer.");
        assert!(check_failed_next(false).contains("only when you choose Check now"));
    }

    #[test]
    fn about_says_the_state_of_the_download() {
        assert_eq!(about_update(v(2), v(1), NaiveDate::from_ymd_opt(2026, 10, 30)), "Jobdesk 0.2.0, released Fri 30 Oct. You have 0.1.0.");
        assert_eq!(about_update(v(2), v(1), None), "Jobdesk 0.2.0. You have 0.1.0.");
        assert_eq!(about_downloading(v(2), 5_400_000, 14_200_000), "Downloading 0.2.0: 38%, 5.4 of 14.2 MB.");
        assert!(about_ready(v(2)).starts_with("Jobdesk 0.2.0 is downloaded and its signature checked. Restart closes Jobdesk"));
        assert_eq!(about_download_failed(v(2), 41, Failure::Interrupted), "Download of 0.2.0 stopped at 41%: the connection dropped.");
        assert_eq!(rail_available(v(2)), "0.2.0 is out");
        assert_eq!(rail_ready(v(2)), "0.2.0 ready");
    }
}
