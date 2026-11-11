//! What the status bar and Settings › About say of a folder update, in plain words with no cost. The paths a notice lists
//! come from a release zip and from the user's folder, so each is cleaned and cut here before any screen gets it.

use crate::folder_update::application::{Failure, Outcome, Phase, Stage, UndoOutcome};
use crate::folder_update::domain::{Report, Reverted, SideCopy, SkipReason};
use crate::shared_kernel::visible_text::{variation_selector, visible_chars};
use crate::updates::application::Version;

/// Undo, beside the notice of an update.
pub const UNDO: &str = "Undo";
/// The same step at the foot of the details.
pub const UNDO_UPDATE: &str = "Undo the update";
pub const DISMISS: &str = "Dismiss";
/// The title over the details.
pub const DETAILS_TITLE: &str = "Your job search folder";
/// What a screen reader calls the lists in the details.
pub const DETAILS_LABEL: &str = "What the update did to your files";
/// The line under the lists on where the names come from and what they cannot do.
pub const DETAILS_TRUST: &str = "File names from your folder and from the update, shown as plain text. Nothing here opens.";
pub const ESC_CLOSES: &str = "Esc closes.";
/// Why Restart to update waits while the folder updates.
pub const RESTART_WAITS: &str = "Jobdesk is updating your job search folder. Restart once it finishes.";
/// What Settings › About says while the automatic check is off and the open folder is one a check can update. It holds
/// whether or not a newer release is out.
pub const CHECK_HINT: &str = "With automatic checks off, your job search folder is brought up to date only after you check for updates.";
/// Why Change folder waits while the folder updates.
pub const FOLDER_WAITS: &str = "Jobdesk is updating your job search folder. Change folder once it finishes.";

/// The most characters of a path a notice shows. A longer one is cut with an ellipsis.
pub const PATH_CHARS: usize = 160;
/// The most paths one list of a notice shows. The rest are counted.
pub const LIST_PATHS: usize = 40;

/// The colour of the square that leads the notice: news, good, or failed. Ink is the screen's to pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeMark {
    News,
    Good,
    Failed,
}

/// One list in the details: Jobdesk's own words, then the paths, each cleaned and cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeList {
    pub lead: String,
    /// Plain text with no line break, never Markdown or a link.
    pub items: Vec<String>,
    /// How many paths past `LIST_PATHS` the list leaves out, in words, such as `and 3 more`.
    pub more: Option<String>,
}

/// What the status bar says of a folder update. The screens draw it and decide nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub mark: NoticeMark,
    pub words: String,
    /// Undo shows while the update can be undone: after an update, until the next task starts; after an Undo that did
    /// not finish, for the rest of the launch, as finishing it keeps a task's edits; and after a failed Undo of either,
    /// to retry. Dismiss hides it, and an unfinished Undo offers it again at the next open.
    pub undo: bool,
    /// The notice can be put away. Not while the folder update works.
    pub dismiss: bool,
    /// What the details list. Empty when the words say it all, and then there are no details to open.
    pub lists: Vec<NoticeList>,
}

/// The one quiet line Settings › About shows of the open folder, when Jobdesk leaves it as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    NoBase,
    GitFolder,
    BadBase,
}

impl Quiet {
    pub fn words(self) -> &'static str {
        match self {
            Quiet::NoBase => "This folder was set up before Jobdesk could update it, so Jobdesk leaves it as it is.",
            Quiet::GitFolder => "This folder uses git, so updating it is up to you.",
            Quiet::BadBase => "Jobdesk cannot read the copy it keeps of how this folder was set up, so Jobdesk leaves it as it is.",
        }
    }
}

/// Whether a stage is worth telling, so it replaces what the notice said. Each gate that keeps an update from running is
/// silent: the folder's state is no news, and Settings › About says why when the folder stays as it is. A refusal while
/// a task, a session or another Jobdesk held the folder changed nothing, and the next try says how it went.
pub fn told(stage: &Stage) -> bool {
    match stage {
        Stage::Idle | Stage::Working(_) => false,
        Stage::Ended(outcome) => {
            matches!(outcome, Outcome::Recovered(_) | Outcome::Updated(_) | Outcome::Failed(_) | Outcome::RollbackFailed(_) | Outcome::UndoUnfinished)
                && !busy(outcome)
        }
        Stage::Undid(_) => true,
    }
}

fn busy(outcome: &Outcome) -> bool {
    matches!(outcome, Outcome::Failed(Failure::Busy) | Outcome::RollbackFailed(Failure::Busy))
}

/// Which pass a failure ended, so its words say what did not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    Update,
    /// The rollback of an update that stopped midway, at an earlier open.
    Rollback,
    Undo,
}

/// The line About shows after `outcome`, if it says anything of the folder's state: `Some(None)` when it shows that
/// Jobdesk does update the folder, so no line shows, and `None` when it says nothing of it.
pub fn quiet_after(outcome: &Outcome) -> Option<Option<Quiet>> {
    match outcome {
        Outcome::NoBase => Some(Some(Quiet::NoBase)),
        Outcome::GitFolder => Some(Some(Quiet::GitFolder)),
        Outcome::BadBase => Some(Some(Quiet::BadBase)),
        Outcome::NoFolder | Outcome::JobdeskNotLatest | Outcome::NoPin | Outcome::UpToDate | Outcome::Skipped | Outcome::Updated(_) => Some(None),
        Outcome::Recovered(_) | Outcome::Failed(_) | Outcome::RollbackFailed(_) | Outcome::UndoUnfinished | Outcome::UndoPending => None,
    }
}

/// The notice for `stage`, the stage the worker is at, and `told`, the last stage worth telling. A worker that
/// downloads, updates or undoes says so. One that only looks whether an update stopped midway, or whether one is due,
/// leaves what the notice said, so a check every few hours never flickers. `undo` is whether Undo may show, and `current`
/// this Jobdesk's version.
pub fn notice(stage: &Stage, told: Option<&Stage>, undo: bool, current: Option<Version>) -> Option<Notice> {
    let working = |words: String| Notice { mark: NoticeMark::News, words, undo: false, dismiss: false, lists: Vec::new() };
    match stage {
        Stage::Working(Phase::Downloading { .. } | Phase::Updating) => return Some(working(working_words(current))),
        Stage::Working(Phase::Undoing) => return Some(working(UNDOING.to_owned())),
        _ => {}
    }
    let working = matches!(stage, Stage::Working(_));
    let mut notice = match told? {
        Stage::Ended(Outcome::Updated(report)) => {
            // A file the update left alone, a safety file among them, keeps the update from reading as whole.
            let mark = if report.skipped.is_empty() { NoticeMark::Good } else { NoticeMark::News };
            Notice { mark, words: updated_words(current, report.skipped.len()), undo, dismiss: true, lists: report_lists(report) }
        }
        Stage::Ended(Outcome::Recovered(reverted)) => said(NoticeMark::Good, RECOVERED.to_owned(), kept_lists(&reverted.kept)),
        Stage::Ended(Outcome::Failed(failure)) => failed(failure, Pass::Update),
        Stage::Ended(Outcome::RollbackFailed(failure)) => failed(failure, Pass::Rollback),
        // Its Undo finishes what the last one began.
        Stage::Ended(Outcome::UndoUnfinished) => Notice { undo, ..said(NoticeMark::Failed, UNDO_UNFINISHED.to_owned(), Vec::new()) },
        Stage::Undid(UndoOutcome::Undone(reverted)) => undone(reverted),
        Stage::Undid(UndoOutcome::NothingToUndo) => said(NoticeMark::News, NOTHING_TO_UNDO.to_owned(), Vec::new()),
        // The update's record stays, so Undo may be pressed again.
        Stage::Undid(UndoOutcome::Failed(failure)) => Notice { undo, ..failed(failure, Pass::Undo) },
        Stage::Idle | Stage::Working(_) | Stage::Ended(_) => return None,
    };
    // A worker that only looks takes Undo and Dismiss away until it has ended, so neither acts on a folder it reads.
    if working {
        (notice.undo, notice.dismiss) = (false, false);
    }
    Some(notice)
}

const UNDOING: &str = "Undoing the last update of your job search folder…";
const UNDO_UNFINISHED: &str = "Jobdesk did not finish undoing the last update of your job search folder.";
const RECOVERED: &str = "Jobdesk finished undoing an interrupted update of your job search folder.";
/// What follows an Undo, which keeps the same release from coming back.
const AGAIN: &str = "It will update your job search folder again with the next Jobdesk release.";
const NOTHING_TO_UNDO: &str = "There was no update of your job search folder left to undo. Nothing changed.";

fn working_words(current: Option<Version>) -> String {
    match current {
        Some(version) => format!("Updating your job search folder to go with Jobdesk {version}…"),
        None => "Updating your job search folder…".to_owned(),
    }
}

/// The line after an update, which says how many files of it were left alone, if any.
fn updated_words(current: Option<Version>, skipped: usize) -> String {
    let with = current.map(|version| format!(" with Jobdesk {version}")).unwrap_or_default();
    match skipped {
        0 => format!("Your job search folder was updated{with}."),
        1 => format!("Your job search folder was updated{with}, except for 1 file."),
        n => format!("Your job search folder was updated{with}, except for {n} files."),
    }
}

fn said(mark: NoticeMark, words: String, lists: Vec<NoticeList>) -> Notice {
    Notice { mark, words, undo: false, dismiss: true, lists }
}

fn undone(reverted: &Reverted) -> Notice {
    // Undo noted the release, so no later pass applies it again: the next Jobdesk release brings a newer one.
    let words = match reverted.kept.len() {
        0 => format!("Jobdesk undid the update. {AGAIN}"),
        1 => format!("Jobdesk undid the update, except for 1 file that changed since. {AGAIN}"),
        n => format!("Jobdesk undid the update, except for {n} files that changed since. {AGAIN}"),
    };
    said(NoticeMark::Good, words, kept_lists(&reverted.kept))
}

/// One honest line for each failure, as its doc says what happened, in the words of the pass it ended.
fn failed(failure: &Failure, pass: Pass) -> Notice {
    let plain = |words: &str| (words.to_owned(), Vec::new());
    let (words, lists) = match failure {
        // A Busy rollback is silent, so this is a rollback whose files could not be read or written. No update ran.
        Failure::Disk | Failure::Busy | Failure::Offline | Failure::Refused(_) | Failure::Mismatch | Failure::TooLarge if pass == Pass::Rollback => plain(
            "Jobdesk could not finish undoing an interrupted update of your job search folder: a file could not be read or written. It tries again the next time the folder opens.",
        ),
        Failure::Offline => plain(
            "Your job search folder was not updated: the connection failed or dropped during the download. Nothing changed, and Jobdesk tries again next time.",
        ),
        Failure::Refused(code) => (
            format!("Your job search folder was not updated: GitHub answered with the error {code}. Nothing changed, and Jobdesk tries again next time."),
            Vec::new(),
        ),
        Failure::Mismatch => plain("Your job search folder was not updated: the download failed Jobdesk's checks and was not used. Nothing changed."),
        Failure::TooLarge => plain("Your job search folder was not updated: the download grew too large, so Jobdesk stopped and deleted it. Nothing changed."),
        Failure::Disk if pass == Pass::Undo => {
            plain("The undo stopped: a file could not be read or written. Some files may already be back as they were before the update.")
        }
        Failure::Disk => {
            plain("Your job search folder was not updated: a file could not be read or written. Jobdesk undid its changes, or does at the next open.")
        }
        Failure::Kept(files) => {
            let words = match files.len() {
                1 => "An update of your job search folder stopped midway. Jobdesk undid the rest, but left 1 file as it is.".to_owned(),
                n => format!("An update of your job search folder stopped midway. Jobdesk undid the rest, but left {n} files as they are."),
            };
            let lead = match files.len() {
                1 => "It changed after the update wrote it, or the copy to put back was gone:".to_owned(),
                _ => "Each changed after the update wrote it, or the copy to put back was gone:".to_owned(),
            };
            (words, vec![list(lead, files.iter().map(String::as_str))])
        }
        Failure::Stopped => plain(
            "An earlier update of your job search folder stopped midway, and its record does not read, so Jobdesk could not undo it. The folder may hold part of it.",
        ),
        Failure::Busy => plain("Jobdesk could not undo the update just now: a task, another Jobdesk or a changed file was in the way. Nothing changed."),
        Failure::Unnoted => plain("Jobdesk did not undo the update: it could not save a note to keep this version from coming back. Nothing changed."),
    };
    Notice { mark: NoticeMark::Failed, words, undo: false, dismiss: true, lists }
}

/// The lists an update reports, each only when it holds a path: the user's files kept beside the update's copy, the
/// safety files the update replaced beside the user's copy, the safety files it put back, and the files it did not
/// write, each with why.
fn report_lists(report: &Report) -> Vec<NoticeList> {
    let mut lists = Vec::new();
    if !report.conflicts.is_empty() {
        let lead = match report.conflicts.len() {
            1 => "You had changed 1 file that this update also changed. Jobdesk kept yours and saved the new version beside it:".to_owned(),
            n => format!("You had changed {n} files that this update also changed. Jobdesk kept yours and saved the new version beside each:"),
        };
        lists.push(side_list(lead, &report.conflicts, "new version in"));
    }
    if !report.upstream_wins.is_empty() {
        let lead = match report.upstream_wins.len() {
            1 => "Jobdesk replaced 1 safety file with the new version and saved yours beside it:".to_owned(),
            n => format!("Jobdesk replaced {n} safety files with the new version and saved yours beside each:"),
        };
        lists.push(side_list(lead, &report.upstream_wins, "yours in"));
    }
    if !report.restored.is_empty() {
        let lead = match report.restored.len() {
            1 => "Jobdesk put back 1 safety file you had removed:".to_owned(),
            n => format!("Jobdesk put back {n} safety files you had removed:"),
        };
        lists.push(list(lead, report.restored.iter().map(String::as_str)));
    }
    if !report.skipped.is_empty() {
        let lead = match report.skipped.len() {
            1 => "Jobdesk did not write 1 file of this update:".to_owned(),
            n => format!("Jobdesk did not write {n} files of this update:"),
        };
        let mut shown = list(lead, std::iter::empty());
        shown.items = report.skipped.iter().take(LIST_PATHS).map(|skipped| format!("{} {}", shown_path(&skipped.path), skip_words(skipped.reason))).collect();
        shown.more = more(report.skipped.len());
        lists.push(shown);
    }
    lists
}

/// Why the update left a file alone, after its path.
fn skip_words(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Link => "is a link",
        SkipReason::InTheWay => "is in the way",
        SkipReason::RemovedFolder => "sits in a folder you removed",
        SkipReason::BadVersion => "needs a copy beside it that Jobdesk could not name",
    }
}

/// The files an undo or a rollback left as they are, because they changed after the update wrote them.
fn kept_lists(kept: &[String]) -> Vec<NoticeList> {
    let lead = match kept.len() {
        0 => return Vec::new(),
        1 => "Jobdesk left 1 file as it is, because it changed after the update wrote it:".to_owned(),
        n => format!("Jobdesk left {n} files as they are, because they changed after the update wrote them:"),
    };
    vec![list(lead, kept.iter().map(String::as_str))]
}

/// Each file and its copy beside it, on one line: `CLAUDE.md, new version in CLAUDE.md.update-1.8.0`.
fn side_list(lead: String, copies: &[SideCopy], beside: &str) -> NoticeList {
    let mut shown = list(lead, std::iter::empty());
    shown.items = copies.iter().take(LIST_PATHS).map(|copy| format!("{}, {beside} {}", shown_path(&copy.path), shown_path(&copy.side))).collect();
    shown.more = more(copies.len());
    shown
}

fn list<'a>(lead: String, paths: impl ExactSizeIterator<Item = &'a str>) -> NoticeList {
    let count = paths.len();
    NoticeList { lead, items: paths.take(LIST_PATHS).map(shown_path).collect(), more: more(count) }
}

fn more(count: usize) -> Option<String> {
    (count > LIST_PATHS).then(|| format!("and {} more", count - LIST_PATHS))
}

/// A path as a notice shows it: with no line break or tab, the characters that hide, restyle or reorder text removed,
/// variation selectors too, and cut to `PATH_CHARS` with an ellipsis.
pub fn shown_path(path: &str) -> String {
    let visible: Vec<char> = visible_chars(path).filter(|&c| !variation_selector(c) && c != '\t').collect();
    if visible.len() <= PATH_CHARS {
        return visible.into_iter().collect();
    }
    let mut cut: String = visible[..PATH_CHARS - 1].iter().collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::folder_update::domain::Skipped;

    fn version() -> Option<Version> {
        Version::parse("0.4.0")
    }

    fn side(path: &str, side: &str) -> SideCopy {
        SideCopy { path: path.into(), side: side.into() }
    }

    fn updated(report: Report) -> Stage {
        Stage::Ended(Outcome::Updated(report))
    }

    fn shown(stage: &Stage, told: Option<&Stage>) -> Option<Notice> {
        notice(stage, told, true, version())
    }

    #[test]
    fn a_running_update_says_so_with_the_jobdesk_it_goes_with_and_offers_nothing() {
        for phase in [Phase::Downloading { received: 9 }, Phase::Updating] {
            let notice = shown(&Stage::Working(phase), None).unwrap();
            assert_eq!(notice.words, "Updating your job search folder to go with Jobdesk 0.4.0…");
            assert_eq!((notice.mark, notice.undo, notice.dismiss, notice.lists.is_empty()), (NoticeMark::News, false, false, true));
        }
        let unknown = notice(&Stage::Working(Phase::Updating), None, true, None).unwrap();
        assert_eq!(unknown.words, "Updating your job search folder…");
        assert_eq!(shown(&Stage::Working(Phase::Undoing), None).unwrap().words, "Undoing the last update of your job search folder…");
    }

    #[test]
    fn a_clean_update_is_one_line_with_undo_and_no_details() {
        let report = Report { version: "1.8.0".into(), updated: vec!["CLAUDE.md".into()], added: vec!["a.md".into()], ..Report::default() };
        let told = updated(report);
        let clean = shown(&Stage::Idle, Some(&told)).unwrap();
        assert_eq!(clean.words, "Your job search folder was updated with Jobdesk 0.4.0.");
        assert_eq!((clean.mark, clean.undo, clean.dismiss), (NoticeMark::Good, true, true));
        assert!(clean.lists.is_empty(), "no ai-job-search version and nothing to open");
        assert!(!notice(&Stage::Idle, Some(&told), false, version()).unwrap().undo, "Undo only while it is offered");
    }

    #[test]
    fn the_details_list_each_kind_with_its_own_count_and_singular() {
        let report = Report {
            version: "1.8.0".into(),
            conflicts: vec![side("CLAUDE.md", "CLAUDE.md.update-1.8.0"), side("notes/plan.md", "notes/plan.md.update-1.8.0")],
            upstream_wins: vec![side(".claude/settings.json", ".claude/settings.json.yours-1.8.0")],
            restored: vec![".claude/hooks/guard.py".into()],
            ..Report::default()
        };
        let lists = shown(&Stage::Idle, Some(&updated(report))).unwrap().lists;
        let leads: Vec<&str> = lists.iter().map(|list| list.lead.as_str()).collect();
        assert_eq!(
            leads,
            [
                "You had changed 2 files that this update also changed. Jobdesk kept yours and saved the new version beside each:",
                "Jobdesk replaced 1 safety file with the new version and saved yours beside it:",
                "Jobdesk put back 1 safety file you had removed:",
            ]
        );
        assert_eq!(lists[0].items, ["CLAUDE.md, new version in CLAUDE.md.update-1.8.0", "notes/plan.md, new version in notes/plan.md.update-1.8.0"]);
        assert_eq!(lists[1].items, [".claude/settings.json, yours in .claude/settings.json.yours-1.8.0"]);
        assert_eq!(lists[2].items, [".claude/hooks/guard.py"]);

        let one = Report { conflicts: vec![side("a.md", "a.md.update-1.8.0")], restored: vec!["b".into(), "c".into()], ..Report::default() };
        let lists = shown(&Stage::Idle, Some(&updated(one))).unwrap().lists;
        assert_eq!(lists[0].lead, "You had changed 1 file that this update also changed. Jobdesk kept yours and saved the new version beside it:");
        assert_eq!(lists[1].lead, "Jobdesk put back 2 safety files you had removed:");
    }

    /// A file of the update that was left alone, safety file or not, never reads as updated: the line says how many,
    /// and the details name each with its plain reason.
    #[test]
    fn files_the_update_left_alone_are_said_and_listed() {
        let skip = |path: &str, reason| Skipped { path: path.into(), reason };
        let report = Report {
            version: "1.8.0".into(),
            updated: vec!["CLAUDE.md".into()],
            skipped: vec![
                skip(".claude/settings.json", SkipReason::Link),
                skip(".claude/hooks/guard.py", SkipReason::InTheWay),
                skip("notes/new.md", SkipReason::RemovedFolder),
            ],
            ..Report::default()
        };
        let notice = shown(&Stage::Idle, Some(&updated(report))).unwrap();
        assert_eq!(notice.words, "Your job search folder was updated with Jobdesk 0.4.0, except for 3 files.");
        assert_ne!(notice.mark, NoticeMark::Good, "never the look of a clean update");
        assert_eq!(notice.lists[0].lead, "Jobdesk did not write 3 files of this update:");
        assert_eq!(
            notice.lists[0].items,
            [".claude/settings.json is a link", ".claude/hooks/guard.py is in the way", "notes/new.md sits in a folder you removed"]
        );

        let one = Report { skipped: vec![skip("CLAUDE.md", SkipReason::Link)], ..Report::default() };
        let notice = shown(&Stage::Idle, Some(&updated(one))).unwrap();
        assert_eq!(notice.words, "Your job search folder was updated with Jobdesk 0.4.0, except for 1 file.");
        assert_eq!(notice.lists[0].lead, "Jobdesk did not write 1 file of this update:");
    }

    /// A rollback at an open that failed ran no update, so it never says the folder was not updated or that its changes
    /// were undone.
    #[test]
    fn a_failed_rollback_says_that_the_rollback_failed() {
        let notice = shown(&Stage::Idle, Some(&Stage::Ended(Outcome::RollbackFailed(Failure::Disk)))).unwrap();
        assert!(notice.words.starts_with("Jobdesk could not finish undoing an interrupted update of your job search folder"), "{}", notice.words);
        assert!(!notice.words.contains("not updated") && !notice.words.contains("undid its changes"), "{}", notice.words);
        assert_eq!(notice.mark, NoticeMark::Failed);
        let kept = shown(&Stage::Idle, Some(&Stage::Ended(Outcome::RollbackFailed(Failure::Kept(vec!["CLAUDE.md".into()]))))).unwrap();
        assert_eq!(kept.lists[0].items, ["CLAUDE.md"]);
        assert!(!told(&Stage::Ended(Outcome::RollbackFailed(Failure::Busy))), "another Jobdesk held it: nothing changed");
    }

    /// Offline covers a timeout and a download cut short as well as no connection at all.
    #[test]
    fn offline_covers_a_connection_lost_on_the_way() {
        let notice = shown(&Stage::Idle, Some(&Stage::Ended(Outcome::Failed(Failure::Offline)))).unwrap();
        assert_eq!(
            notice.words,
            "Your job search folder was not updated: the connection failed or dropped during the download. Nothing changed, and Jobdesk tries again next time."
        );
    }

    #[test]
    fn a_rollback_and_an_undo_list_the_files_they_left() {
        let kept = Reverted { reverted: vec!["a.md".into()], kept: vec!["CLAUDE.md".into()] };
        let recovered = shown(&Stage::Idle, Some(&Stage::Ended(Outcome::Recovered(kept.clone())))).unwrap();
        assert_eq!(recovered.words, "Jobdesk finished undoing an interrupted update of your job search folder.");
        assert_eq!(recovered.lists[0].lead, "Jobdesk left 1 file as it is, because it changed after the update wrote it:");
        assert_eq!((recovered.undo, recovered.mark), (false, NoticeMark::Good));

        let undone = shown(&Stage::Idle, Some(&Stage::Undid(UndoOutcome::Undone(kept)))).unwrap();
        assert_eq!(
            undone.words,
            "Jobdesk undid the update, except for 1 file that changed since. It will update your job search folder again with the next Jobdesk release."
        );
        assert_eq!(undone.lists[0].items, ["CLAUDE.md"]);
        let clean = shown(&Stage::Idle, Some(&Stage::Undid(UndoOutcome::Undone(Reverted::default())))).unwrap();
        assert_eq!(
            (clean.words.as_str(), clean.lists.len()),
            ("Jobdesk undid the update. It will update your job search folder again with the next Jobdesk release.", 0)
        );
        let nothing = shown(&Stage::Idle, Some(&Stage::Undid(UndoOutcome::NothingToUndo))).unwrap();
        assert_eq!(nothing.words, "There was no update of your job search folder left to undo. Nothing changed.");
    }

    #[test]
    fn each_failure_says_what_happened_and_never_offers_undo() {
        let cases = [
            (Failure::Offline, "Nothing changed, and Jobdesk tries again next time."),
            (Failure::Refused(503), "GitHub answered with the error 503. Nothing changed, and Jobdesk tries again next time."),
            (Failure::Mismatch, "the download failed Jobdesk's checks and was not used. Nothing changed."),
            (Failure::TooLarge, "so Jobdesk stopped and deleted it. Nothing changed."),
            (Failure::Disk, "Jobdesk undid its changes, or does at the next open."),
            (Failure::Stopped, "so Jobdesk could not undo it. The folder may hold part of it."),
            (Failure::Kept(vec!["a".into(), "b".into()]), "Jobdesk undid the rest, but left 2 files as they are."),
        ];
        for (failure, end) in cases {
            let notice = shown(&Stage::Idle, Some(&Stage::Ended(Outcome::Failed(failure.clone())))).unwrap();
            assert!(notice.words.ends_with(end), "{failure:?}: {}", notice.words);
            assert_eq!((notice.mark, notice.undo, notice.dismiss), (NoticeMark::Failed, false, true));
            assert!(!notice.words.contains("ai-job-search") && !notice.words.contains('$'), "{failure:?}");
        }
        let kept = shown(&Stage::Idle, Some(&Stage::Ended(Outcome::Failed(Failure::Kept(vec!["CLAUDE.md".into()]))))).unwrap();
        assert_eq!(kept.words, "An update of your job search folder stopped midway. Jobdesk undid the rest, but left 1 file as it is.");
        assert_eq!(kept.lists[0].items, ["CLAUDE.md"]);
        let undo_disk = shown(&Stage::Idle, Some(&Stage::Undid(UndoOutcome::Failed(Failure::Disk)))).unwrap();
        assert!(undo_disk.words.ends_with("Some files may already be back as they were before the update."));
        let unnoted = shown(&Stage::Idle, Some(&Stage::Undid(UndoOutcome::Failed(Failure::Unnoted)))).unwrap();
        assert_eq!(unnoted.words, "Jobdesk did not undo the update: it could not save a note to keep this version from coming back. Nothing changed.");
        let undo_busy = shown(&Stage::Idle, Some(&Stage::Undid(UndoOutcome::Failed(Failure::Busy)))).unwrap();
        assert!(undo_busy.words.ends_with("Nothing changed."));
    }

    #[test]
    fn the_gates_and_a_busy_folder_are_silent() {
        for outcome in [
            Outcome::NoFolder,
            Outcome::JobdeskNotLatest,
            Outcome::NoPin,
            Outcome::UpToDate,
            Outcome::NoBase,
            Outcome::GitFolder,
            Outcome::BadBase,
            Outcome::Failed(Failure::Busy),
        ] {
            assert!(!told(&Stage::Ended(outcome.clone())), "{outcome:?}");
        }
        assert!(!told(&Stage::Idle) && !told(&Stage::Working(Phase::Updating)));
        assert!(told(&Stage::Undid(UndoOutcome::Failed(Failure::Busy))), "an Undo the user pressed always answers");
        assert_eq!(shown(&Stage::Idle, None), None);
    }

    #[test]
    fn a_look_while_a_notice_shows_keeps_it_with_nothing_to_press() {
        let told = updated(Report::default());
        let notice = shown(&Stage::Working(Phase::Checking), Some(&told)).unwrap();
        assert_eq!(notice.words, "Your job search folder was updated with Jobdesk 0.4.0.");
        assert_eq!((notice.undo, notice.dismiss), (false, false));
        assert_eq!(shown(&Stage::Working(Phase::Checking), None), None, "nothing to say while it only looks");
    }

    #[test]
    fn about_says_why_a_folder_stays_as_it_is() {
        assert_eq!(quiet_after(&Outcome::NoBase), Some(Some(Quiet::NoBase)));
        assert_eq!(Quiet::NoBase.words(), "This folder was set up before Jobdesk could update it, so Jobdesk leaves it as it is.");
        assert_eq!(Quiet::GitFolder.words(), "This folder uses git, so updating it is up to you.");
        assert_eq!(quiet_after(&Outcome::UpToDate), Some(None));
        assert_eq!(quiet_after(&Outcome::Failed(Failure::Offline)), None);
    }

    #[test]
    fn paths_show_as_plain_text_cut_and_counted() {
        assert_eq!(shown_path("docs/\u{202e}gpj.exe\u{200b}\u{1b}[31m\u{fe0f}\tplan.md"), "docs/gpj.exeplan.md");
        let long = "a/".repeat(200);
        let cut = shown_path(&long);
        assert_eq!(cut.chars().count(), PATH_CHARS);
        assert!(cut.ends_with('…'));
        let many: Vec<String> = (0..43).map(|n| format!("file-{n}.md")).collect();
        let lists = kept_lists(&many);
        assert_eq!((lists[0].items.len(), lists[0].more.as_deref()), (LIST_PATHS, Some("and 3 more")));
        assert_eq!(kept_lists(&many[..2])[0].more, None);
    }
}
