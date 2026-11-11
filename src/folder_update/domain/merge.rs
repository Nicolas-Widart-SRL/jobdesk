//! What a folder update does with one file, from three copies of it: `base` in the release the folder was set up
//! from, `ours` in the folder now, and `theirs` in the new release. Like `git merge` of a release tag, one file at a
//! time: a change on one side only is taken, the same change on both is kept, and changes to different lines merge.
//! Where both sides changed the same lines, the user's file stays and upstream's goes beside it, except for the files
//! that guard what Claude may do, where upstream's goes in place and the user's goes beside it. A file the user deleted
//! stays deleted, except one of those files when upstream changed it: then upstream's copy comes back.

use std::borrow::Cow;

/// The files whose upstream copy wins a conflict, because they hold the rules that guard what Claude may do.
pub const SECURITY_FILES: [&str; 2] = [".claude/settings.json", "tools/security_guards.py"];

/// What the update does with one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Leave the folder's file as it is, or leave it absent.
    Keep,
    /// Write these bytes over the folder's file, or add it.
    Write(Vec<u8>),
    /// Remove the folder's file.
    Delete,
    /// Keep the folder's file, and save upstream's copy beside it under [`side_name`].
    Conflict { theirs: Vec<u8> },
    /// Write `theirs` over the folder's file, and save the user's copy `ours` beside it under [`side_name`].
    UpstreamWins { ours: Vec<u8>, theirs: Vec<u8> },
    /// Write these bytes back as the file: upstream changed one of the [`SECURITY_FILES`] the user had deleted, so its
    /// fix lands. Nothing of the user's is saved beside it, and the notice names the file as restored.
    Restore(Vec<u8>),
    /// Add nothing: upstream added the file into a folder the user has removed.
    Skip,
}

/// What to do with the file at `path`, relative to the folder with `/` separators. `None` is a file that is absent: in
/// the base release, in the folder now, or in the new release. `parent_kept` is false when any ancestor folder of the
/// file existed in base and the user has removed it. It only affects files new upstream: a deleted security file that
/// upstream changed is restored whatever it says. The same inputs always give the same action and bytes.
pub fn decide(path: &str, base: Option<&[u8]>, ours: Option<&[u8]>, theirs: Option<&[u8]>, parent_kept: bool) -> Action {
    let crlf = takes_crlf(ours, base, theirs);
    let (base, theirs) = (base.map(|base| with_crlf(base, crlf)), theirs.map(|theirs| with_crlf(theirs, crlf)));
    let (base, theirs) = (base.as_deref(), theirs.as_deref());
    if ours == theirs {
        return Action::Keep;
    }
    match (base, ours, theirs) {
        (Some(base), Some(ours), theirs) if ours == base => theirs.map_or(Action::Delete, |theirs| Action::Write(theirs.to_vec())),
        // Upstream left the file alone, so the user's edit or deletion stays.
        _ if theirs == base => Action::Keep,
        (None, None, Some(theirs)) if parent_kept => Action::Write(theirs.to_vec()),
        (None, None, Some(_)) => Action::Skip,
        (None, Some(ours), Some(theirs)) => conflict(path, ours, theirs),
        // The user deleted a file that guards what Claude may do, and upstream changed it: upstream's fix lands.
        (Some(_), None, Some(theirs)) if SECURITY_FILES.contains(&path) => Action::Restore(theirs.to_vec()),
        // The user deleted the file, or changed one upstream removed: either way the user's choice stays.
        (Some(_), None, Some(_)) | (_, _, None) => Action::Keep,
        (Some(base), Some(ours), Some(theirs)) => merged(path, base, ours, theirs),
    }
}

/// The name of the copy saved beside `path` when the update meets a conflict in it: upstream's copy, or the user's
/// when upstream wins. `version` is the new release's plain version, such as `1.8.0`: digits split by single dots. So
/// the name ends in a digit and never in `.md`, `.json` or `.py`: Claude Code never loads it as a command, an agent, a
/// setting or a module. `None` when `version` is anything else.
pub fn side_name(path: &str, version: &str, upstream_wins: bool) -> Option<String> {
    let plain = version.split('.').all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    if !plain {
        return None;
    }
    let side = if upstream_wins { "yours" } else { "update" };
    Some(format!("{path}.{side}-{version}"))
}

/// Rule 7: all three copies differ. Text merges line by line, and a clean merge is written. Binary never merges.
fn merged(path: &str, base: &[u8], ours: &[u8], theirs: &[u8]) -> Action {
    if [base, ours, theirs].into_iter().all(is_text) {
        match merge_lines(base, ours, theirs) {
            Some(merged) if merged == ours => return Action::Keep,
            Some(merged) => return Action::Write(merged),
            None => {}
        }
    }
    conflict(path, ours, theirs)
}

fn conflict(path: &str, ours: &[u8], theirs: &[u8]) -> Action {
    if SECURITY_FILES.contains(&path) {
        Action::UpstreamWins { ours: ours.to_vec(), theirs: theirs.to_vec() }
    } else {
        Action::Conflict { theirs: theirs.to_vec() }
    }
}

/// The three-way line merge, or `None` when both sides changed the same lines.
fn merge_lines(base: &[u8], ours: &[u8], theirs: &[u8]) -> Option<Vec<u8>> {
    diffy::merge_bytes(base, ours, theirs).ok()
}

/// Text holds no NUL byte and is valid UTF-8. Anything else is binary.
fn is_text(bytes: &[u8]) -> bool {
    !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

/// The user's text uses CRLF throughout and upstream's uses LF, as a Windows checkout or editor leaves it. Then
/// upstream's copies take CRLF, so a line ending alone never counts as a change.
fn takes_crlf(ours: Option<&[u8]>, base: Option<&[u8]>, theirs: Option<&[u8]>) -> bool {
    ours.is_some_and(|ours| is_text(ours) && crlf_throughout(ours)) && base.into_iter().chain(theirs).all(|bytes| is_text(bytes) && !bytes.contains(&b'\r'))
}

/// Every LF in `bytes` is the end of a CRLF, and there is at least one.
fn crlf_throughout(bytes: &[u8]) -> bool {
    let mut ends = bytes.iter().enumerate().filter(|(_, byte)| **byte == b'\n').peekable();
    ends.peek().is_some() && ends.all(|(at, _)| at > 0 && bytes[at - 1] == b'\r')
}

/// `bytes` with each LF made CRLF when `crlf`, otherwise as they are.
fn with_crlf(bytes: &[u8], crlf: bool) -> Cow<'_, [u8]> {
    if !crlf {
        return Cow::Borrowed(bytes);
    }
    let mut converted = Vec::with_capacity(bytes.len() + bytes.len() / 16);
    for &byte in bytes {
        if byte == b'\n' {
            converted.push(b'\r');
        }
        converted.push(byte);
    }
    Cow::Owned(converted)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "notes/plan.md";

    fn decide_text(path: &str, base: Option<&str>, ours: Option<&str>, theirs: Option<&str>) -> Action {
        decide(path, base.map(str::as_bytes), ours.map(str::as_bytes), theirs.map(str::as_bytes), true)
    }

    fn write(text: &str) -> Action {
        Action::Write(text.as_bytes().to_vec())
    }

    fn conflict_with(theirs: &str) -> Action {
        Action::Conflict { theirs: theirs.as_bytes().to_vec() }
    }

    #[test]
    fn rule_1_the_same_file_on_both_sides_is_kept() {
        assert_eq!(decide_text(PATH, Some("a\n"), Some("b\n"), Some("b\n")), Action::Keep);
        assert_eq!(decide_text(PATH, Some("a\n"), None, None), Action::Keep, "both removed it");
        assert_eq!(decide_text(PATH, None, Some("b\n"), Some("b\n")), Action::Keep, "both added it");
    }

    #[test]
    fn rule_2_an_untouched_file_takes_upstream() {
        assert_eq!(decide_text(PATH, Some("a\n"), Some("a\n"), Some("b\n")), write("b\n"));
        assert_eq!(decide_text(PATH, Some("a\n"), Some("a\n"), None), Action::Delete, "upstream removed it");
    }

    #[test]
    fn rule_3_upstream_left_alone_keeps_the_user_edit_or_deletion() {
        assert_eq!(decide_text(PATH, Some("a\n"), Some("b\n"), Some("a\n")), Action::Keep);
        assert_eq!(decide_text(PATH, Some("a\n"), None, Some("a\n")), Action::Keep, "the user deleted it");
        assert_eq!(decide_text(PATH, None, Some("mine\n"), None), Action::Keep, "a file only the user has");
    }

    #[test]
    fn rule_4_a_new_upstream_file_is_added() {
        assert_eq!(decide_text(PATH, None, None, Some("new\n")), write("new\n"));
    }

    #[test]
    fn rule_4_a_new_file_in_a_removed_folder_is_skipped() {
        assert_eq!(decide(PATH, None, None, Some(b"new\n"), false), Action::Skip);
    }

    #[test]
    fn rule_4_a_new_file_the_user_also_made_is_a_conflict() {
        assert_eq!(decide_text(PATH, None, Some("mine\n"), Some("new\n")), conflict_with("new\n"));
    }

    #[test]
    fn rule_5_a_file_the_user_deleted_stays_deleted() {
        assert_eq!(decide_text(PATH, Some("a\n"), None, Some("b\n")), Action::Keep);
    }

    #[test]
    fn rule_5_a_security_file_the_user_deleted_comes_back_when_upstream_changed_it() {
        for path in SECURITY_FILES {
            assert_eq!(decide_text(path, Some("a\n"), None, Some("b\n")), Action::Restore(b"b\n".to_vec()), "{path}");
            assert_eq!(decide_text(path, Some("a\n"), None, Some("a\n")), Action::Keep, "{path}: upstream left it alone");
            let removed_folder = decide(path, Some(b"a\n"), None, Some(b"b\n"), false);
            assert_eq!(removed_folder, Action::Restore(b"b\n".to_vec()), "{path}: parent_kept only affects files new upstream");
        }
    }

    #[test]
    fn rule_6_a_file_the_user_changed_stays_when_upstream_removed_it() {
        assert_eq!(decide_text(PATH, Some("a\n"), Some("b\n"), None), Action::Keep);
        assert_eq!(decide_text("tools/security_guards.py", Some("a\n"), Some("b\n"), None), Action::Keep, "a security file too");
    }

    #[test]
    fn rule_7_changes_to_different_lines_merge() {
        let base = "one\ntwo\nthree\nfour\nfive\n";
        let ours = "ONE\ntwo\nthree\nfour\nfive\n";
        let theirs = "one\ntwo\nthree\nfour\nFIVE\n";
        assert_eq!(decide_text(PATH, Some(base), Some(ours), Some(theirs)), write("ONE\ntwo\nthree\nfour\nFIVE\n"));
    }

    #[test]
    fn rule_7_both_sides_on_the_same_line_is_a_conflict() {
        let theirs = "one\nTwo, upstream\nthree\n";
        assert_eq!(decide_text(PATH, Some("one\ntwo\nthree\n"), Some("one\nTwo, mine\nthree\n"), Some(theirs)), conflict_with(theirs));
    }

    #[test]
    fn rule_7_a_merge_that_only_gives_the_user_file_keeps_it() {
        let ours = "ONE\ntwo\nTHREE\n";
        assert_eq!(decide_text(PATH, Some("one\ntwo\nthree\n"), Some(ours), Some("ONE\ntwo\nthree\n")), Action::Keep);
    }

    #[test]
    fn upstream_wins_a_conflict_in_a_security_file() {
        let base = "{\n  \"allow\": [\"Read\"]\n}\n";
        let ours = "{\n  \"allow\": [\"Read\", \"Bash\"]\n}\n";
        let theirs = "{\n  \"allow\": [\"Read\", \"Grep\"]\n}\n";
        let wins = Action::UpstreamWins { ours: ours.as_bytes().to_vec(), theirs: theirs.as_bytes().to_vec() };
        assert_eq!(decide_text(".claude/settings.json", Some(base), Some(ours), Some(theirs)), wins);
        assert_eq!(decide_text("tools/security_guards.py", Some(base), Some(ours), Some(theirs)), wins);
        assert_eq!(decide_text("notes/settings.json", Some(base), Some(ours), Some(theirs)), conflict_with(theirs), "only those paths");
        let added = Action::UpstreamWins { ours: b"mine\n".to_vec(), theirs: b"new\n".to_vec() };
        assert_eq!(decide_text(".claude/settings.json", None, Some("mine\n"), Some("new\n")), added);
    }

    #[test]
    fn a_security_file_still_merges_when_the_changes_do_not_meet() {
        let base = "a = 1\nb = 2\nc = 3\nd = 4\n";
        let ours = "a = 10\nb = 2\nc = 3\nd = 4\n";
        let theirs = "a = 1\nb = 2\nc = 3\nd = 40\n";
        assert_eq!(decide_text("tools/security_guards.py", Some(base), Some(ours), Some(theirs)), write("a = 10\nb = 2\nc = 3\nd = 40\n"));
    }

    #[test]
    fn the_same_change_on_both_sides_is_kept() {
        let changed = "# Plan\n\nApply to three studios a week.\n";
        assert_eq!(decide_text(PATH, Some("# Plan\n\nApply to two studios a week.\n"), Some(changed), Some(changed)), Action::Keep);
    }

    #[test]
    fn a_setup_fill_merges_with_an_upstream_edit_to_another_paragraph() {
        let base = "# Candidate Profile\n\nName: [YOUR_NAME]\nCity: [YOUR_CITY]\n\n## How to apply\n\nWrite one letter per role.\n";
        let ours = "# Candidate Profile\n\nName: Ada Quill\nCity: Lisbonne\n\n## How to apply\n\nWrite one letter per role.\n";
        let theirs = "# Candidate Profile\n\nName: [YOUR_NAME]\nCity: [YOUR_CITY]\n\n## How to apply\n\nWrite one short letter per role, under a page.\n";
        let merged = "# Candidate Profile\n\nName: Ada Quill\nCity: Lisbonne\n\n## How to apply\n\nWrite one short letter per role, under a page.\n";
        assert_eq!(decide_text("CLAUDE.md", Some(base), Some(ours), Some(theirs)), write(merged));
    }

    #[test]
    fn a_portal_switch_merges_with_an_upstream_body_edit() {
        let base = "---\nname: example-board\ndescription: Jobs from Example Board\nenabled: true\n---\n\n# Example Board\n\nSearch the board.\n\nRead each posting.\n";
        let ours = base.replace("enabled: true", "enabled: false");
        let theirs = base.replace("Read each posting.", "Read each posting, then its company page.");
        let merged = ours.replace("Read each posting.", "Read each posting, then its company page.");
        let path = ".claude/skills/example-board/SKILL.md";
        assert_eq!(decide_text(path, Some(base), Some(&ours), Some(&theirs)), write(&merged));
    }

    #[test]
    fn a_crlf_folder_compares_and_merges_in_crlf() {
        let base = "one\ntwo\nthree\nfour\n";
        let theirs = "one\ntwo\nthree\nFOUR\n";
        assert_eq!(decide_text(PATH, Some(base), Some("one\r\ntwo\r\nthree\r\nfour\r\n"), Some(theirs)), write("one\r\ntwo\r\nthree\r\nFOUR\r\n"), "untouched");
        let ours = "ONE\r\ntwo\r\nthree\r\nfour\r\n";
        assert_eq!(decide_text(PATH, Some(base), Some(ours), Some(theirs)), write("ONE\r\ntwo\r\nthree\r\nFOUR\r\n"), "merged");
        let same = "ONE\r\ntwo\r\nthree\r\nfour\r\n";
        assert_eq!(decide_text(PATH, Some(base), Some(same), Some("ONE\ntwo\nthree\nfour\n")), Action::Keep, "line endings alone");
    }

    #[test]
    fn line_endings_change_only_when_the_user_file_is_all_crlf() {
        let mixed = "ONE\r\ntwo\nthree\nfour\n";
        let theirs = "one\ntwo\nthree\nFOUR\n";
        assert_eq!(decide_text(PATH, Some("one\ntwo\nthree\nfour\n"), Some(mixed), Some(theirs)), write("ONE\r\ntwo\nthree\nFOUR\n"));
        let mixed_upstream = "one\r\nTWO\n";
        let action = decide_text(PATH, Some("one\ntwo\n"), Some("one\r\ntwo\r\n"), Some(mixed_upstream));
        assert_eq!(action, conflict_with(mixed_upstream), "upstream not all LF");
    }

    #[test]
    fn binary_never_merges() {
        // Each pair of changes leaves untouched lines between them, so as text they would merge.
        let base = b"\x00one\ntwo\nthree\nfour\nfive\n".as_slice();
        let ours = b"\x00ONE\ntwo\nthree\nfour\nfive\n".as_slice();
        let theirs = b"\x00one\ntwo\nthree\nfour\nFIVE\n".as_slice();
        assert_eq!(decide(PATH, Some(base), Some(ours), Some(theirs), true), Action::Conflict { theirs: theirs.to_vec() });
        let base = b"one\ntwo\nthree\nfour\n\xffa\n".as_slice();
        let ours = b"ONE\ntwo\nthree\nfour\n\xffa\n".as_slice();
        let theirs = b"one\ntwo\nthree\nfour\n\xffb\n".as_slice();
        assert_eq!(decide(PATH, Some(base), Some(ours), Some(theirs), true), Action::Conflict { theirs: theirs.to_vec() }, "not UTF-8");
        assert_eq!(decide(PATH, Some(base), Some(base), Some(theirs), true), Action::Write(theirs.to_vec()), "untouched binary takes upstream");
    }

    #[test]
    fn binary_keeps_its_line_endings() {
        let theirs = b"\x00b\n".as_slice();
        let action = decide(PATH, Some(b"\x00a\n"), Some(b"\x00c\r\n"), Some(theirs), true);
        assert_eq!(action, Action::Conflict { theirs: theirs.to_vec() });
    }

    #[test]
    fn the_same_inputs_give_the_same_bytes() {
        let base = "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\n";
        let ours = "alpha\nBETA\ngamma\ndelta\nepsilon\nzeta\nmine\n";
        let theirs = "theirs\nalpha\nbeta\ngamma\ndelta\nEPSILON\nzeta\n";
        let first = decide_text(PATH, Some(base), Some(ours), Some(theirs));
        assert!(matches!(first, Action::Write(_)));
        for _ in 0..20 {
            assert_eq!(decide_text(PATH, Some(base), Some(ours), Some(theirs)), first);
        }
    }

    #[test]
    fn side_names_say_whose_copy_and_never_load_as_claude_code_files() {
        assert_eq!(side_name("CLAUDE.md", "1.8.0", false).as_deref(), Some("CLAUDE.md.update-1.8.0"));
        assert_eq!(side_name(".claude/settings.json", "1.8.0", true).as_deref(), Some(".claude/settings.json.yours-1.8.0"));
        for path in [".claude/commands/apply.md", ".claude/settings.json", "tools/security_guards.py", "cv/main_example.tex"] {
            for upstream_wins in [false, true] {
                let name = side_name(path, "2.10.3", upstream_wins).expect("a plain version");
                assert!(name.starts_with(path), "{name}");
                assert!(![".md", ".json", ".py"].iter().any(|end| name.ends_with(end)), "{name}");
            }
        }
    }

    #[test]
    fn side_names_take_only_a_plain_version() {
        assert_eq!(side_name("CLAUDE.md", "7", false).as_deref(), Some("CLAUDE.md.update-7"));
        for version in ["", "1.8.0.md", "1.8.0.json", "1.8/0", "../1.8.0", "v1.8.0", "1.8.0-beta", "1.8.", ".1.8", "1..8", "1.8 0", "1.8.0\n"] {
            assert_eq!(side_name("CLAUDE.md", version, false), None, "{version:?}");
            assert_eq!(side_name("CLAUDE.md", version, true), None, "{version:?}");
        }
    }
}
