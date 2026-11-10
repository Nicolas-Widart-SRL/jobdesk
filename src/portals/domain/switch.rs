use crate::shared_kernel::LoadError;
use crate::shared_kernel::file_text::BOM;

/// What turning a portal on or off makes of its `SKILL.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// The text already reads as asked, so nothing is written.
    Unchanged,
    /// The new text. Only `enabled:` lines differ, or one `enabled: false` line is added.
    Changed(String),
    /// Off needs an `enabled: false` line, and the text has no frontmatter to hold one.
    NoFrontmatter,
}

/// Why a portal was not switched or removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeError {
    /// A run works, so nothing was read or written. A run may read or write the portal folders.
    Busy,
    /// A portal that ships with `ai-job-search` is only switched off, never removed.
    BuiltIn,
    /// The `SKILL.md` has no frontmatter, so it cannot hold an `enabled: false` line. Nothing was written.
    NoFrontmatter,
    /// The folder or its `SKILL.md` was refused, or could not be read, written or moved.
    Failed(LoadError),
}

/// The two writes Jobdesk makes to a portal folder, by its name under `.agents/skills`.
pub trait PortalStore {
    /// Turns the portal on or off in its `SKILL.md`, as `set_enabled` edits it.
    fn set_enabled(&self, folder: &str, on: bool) -> Result<(), ChangeError>;
    /// Moves the portal's folder to the system Trash, from where the user can put it back.
    fn trash(&self, folder: &str) -> Result<(), ChangeError>;
}

/// Turns a `SKILL.md` on or off, as `parse_manifest` then reads it. Every top-level `enabled:` line in the frontmatter
/// gets `true` or `false`, with its comment and the space around its value kept. A line that already holds that word
/// stays as it is. Off with no such line adds `enabled: false` just before the closing `---`. On with no such line, or
/// with no frontmatter, changes nothing, because the portal already reads as on. A byte order mark and the line
/// endings stay.
pub fn set_enabled(text: &str, on: bool) -> Edit {
    let (bom, rest) = text.strip_prefix(BOM).map_or((false, text), |rest| (true, rest));
    let lines = lines(rest);
    let fence = |line: &(&str, &str)| line.0.trim_end() == "---";
    let close = lines.first().filter(|line| fence(line)).and_then(|_| lines.iter().skip(1).position(fence)).map(|at| at + 1);
    let Some(close) = close else { return if on { Edit::Unchanged } else { Edit::NoFrontmatter } };
    let word = if on { "true" } else { "false" };
    let mut out = String::with_capacity(text.len() + 16);
    if bom {
        out.push(BOM);
    }
    let mut found = false;
    for (at, (line, ending)) in lines.iter().enumerate() {
        if at == close && !found && !on {
            out.push_str("enabled: false");
            out.push_str(lines[close - 1].1);
        }
        match enabled_line(line).filter(|_| at > 0 && at < close) {
            Some((head, value, tail)) => {
                found = true;
                if holds(value, on) {
                    out.push_str(line);
                } else {
                    let space_before = if head.ends_with(':') { " " } else { "" };
                    let space_after = if tail.starts_with('#') { " " } else { "" };
                    out.push_str(&format!("{head}{space_before}{word}{space_after}{tail}"));
                }
            }
            None => out.push_str(line),
        }
        out.push_str(ending);
    }
    if out == text { Edit::Unchanged } else { Edit::Changed(out) }
}

/// Whether a value already holds the word YAML reads as on or off.
fn holds(value: &str, on: bool) -> bool {
    if on { matches!(value, "true" | "True" | "TRUE") } else { matches!(value, "false" | "False" | "FALSE") }
}

/// Each line with its ending: CRLF, LF or a lone CR, as `with_lf` breaks lines. The last line may have none.
fn lines(text: &str) -> Vec<(&str, &str)> {
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let Some(at) = rest.find(['\r', '\n']) else {
            lines.push((rest, ""));
            break;
        };
        let end = if rest[at..].starts_with("\r\n") { at + 2 } else { at + 1 };
        lines.push((&rest[..at], &rest[at..end]));
        rest = &rest[end..];
    }
    lines
}

/// A top-level `enabled` line, as `parse_manifest` finds one, split in three: the key, the colon and the space after it,
/// then the value, then the space after the value and any comment. A `#` after a space or a tab opens a comment.
fn enabled_line(line: &str) -> Option<(&str, &str, &str)> {
    let colon = line.find(':')?;
    if line[..colon].trim_end() != "enabled" {
        return None;
    }
    let after = &line[colon + 1..];
    let comment = after.char_indices().find(|&(at, c)| c == '#' && (at == 0 || after[..at].ends_with([' ', '\t'])));
    let before_comment = &after[..comment.map_or(after.len(), |(at, _)| at)];
    let value = before_comment.trim();
    let start = colon + 1 + if value.is_empty() { before_comment.len() } else { before_comment.len() - before_comment.trim_start().len() };
    Some((&line[..start], value, &line[start + value.len()..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portals::domain::parse_manifest;

    /// Shaped as upstream's Danish demos: off by a line with a long comment.
    const JOBNET: &str = "---\nname: jobnet-search\ncontext: fork\nenabled: false  # Danish demo portal - ships opt-in; /setup enables it when your market is Denmark, or set true here yourself\n---\n\n# Jobnet-Search Skill\n\nAccess live Danish job listings. enabled: false stays in prose.\n";

    fn changed(text: &str, on: bool) -> String {
        match set_enabled(text, on) {
            Edit::Changed(text) => text,
            other => panic!("expected a change, got {other:?}"),
        }
    }

    fn reads(text: &str) -> bool {
        parse_manifest(text, "northfield-search").enabled
    }

    #[test]
    fn on_keeps_the_upstream_comment() {
        let on = changed(JOBNET, true);
        assert_eq!(on, JOBNET.replace("enabled: false  # Danish", "enabled: true  # Danish"), "only the value changes, and the prose keeps its own words");
        assert!(reads(&on));
        assert_eq!(changed(&on, false), JOBNET);
        assert!(!reads(JOBNET));
    }

    #[test]
    fn a_tab_comment_and_a_space_before_the_colon_are_kept() {
        let text = "---\nname: n\nenabled : false\t# off for now\n---\n";
        let on = changed(text, true);
        assert_eq!(on, "---\nname: n\nenabled : true\t# off for now\n---\n");
        assert!(reads(&on));
        assert_eq!(changed("---\nenabled:\tfalse\n---\n", true), "---\nenabled:\ttrue\n---\n");
    }

    #[test]
    fn a_value_with_no_space_or_only_a_comment_gets_one() {
        assert_eq!(changed("---\nenabled:false\n---\n", true), "---\nenabled: true\n---\n");
        assert_eq!(changed("---\nenabled: #false\n---\n", false), "---\nenabled: false #false\n---\n");
        assert_eq!(changed("---\nenabled:\n---\n", false), "---\nenabled: false\n---\n");
        assert!(!reads("---\nenabled: false #false\n---\n"));
    }

    #[test]
    fn every_top_level_line_is_edited_so_the_last_one_agrees() {
        let text = "---\nenabled: false\nname: n\nenabled: true # later wins\n---\n";
        let off = changed(text, false);
        assert_eq!(off, "---\nenabled: false\nname: n\nenabled: false # later wins\n---\n");
        assert!(!reads(&off));
        let on = changed(&off, true);
        assert_eq!(on, "---\nenabled: true\nname: n\nenabled: true # later wins\n---\n");
        assert!(reads(&on));
    }

    #[test]
    fn a_line_that_already_holds_the_word_stays() {
        assert_eq!(set_enabled("---\nenabled: TRUE  # on\n---\n", true), Edit::Unchanged);
        assert_eq!(set_enabled(JOBNET, false), Edit::Unchanged);
        assert_eq!(changed("---\nenabled: 'False'\n---\n", false), "---\nenabled: false\n---\n", "quotes make a YAML string");
        assert_eq!(changed("---\nenabled: no\n---\n", true), "---\nenabled: true\n---\n");
    }

    #[test]
    fn off_without_a_line_adds_one_before_the_closing_fence() {
        let text = "---\nname: northfield-search\ndescription: >\n  Search Northfield.\n---\n\n# Northfield Search Skill\n";
        let off = changed(text, false);
        assert_eq!(off, "---\nname: northfield-search\ndescription: >\n  Search Northfield.\nenabled: false\n---\n\n# Northfield Search Skill\n");
        assert!(!reads(&off));
        assert_eq!(changed("---\n---\nbody\n", false), "---\nenabled: false\n---\nbody\n");
        assert_eq!(changed("---\nname: n\n---", false), "---\nname: n\nenabled: false\n---", "a closing fence with no line break");
    }

    #[test]
    fn on_without_a_line_or_a_frontmatter_changes_nothing() {
        assert_eq!(set_enabled("---\nname: n\n---\n", true), Edit::Unchanged);
        assert_eq!(set_enabled("# Northfield Search Skill\n", true), Edit::Unchanged);
        assert_eq!(set_enabled("---\nname: n\n", true), Edit::Unchanged, "an unclosed frontmatter is none");
    }

    #[test]
    fn off_without_a_frontmatter_cannot_be_written() {
        assert_eq!(set_enabled("# Northfield Search Skill\n\nenabled: true\n", false), Edit::NoFrontmatter);
        assert_eq!(set_enabled("---\nname: n\nenabled: true\n", false), Edit::NoFrontmatter, "an unclosed frontmatter is none");
        assert_eq!(set_enabled("", false), Edit::NoFrontmatter);
    }

    #[test]
    fn only_top_level_lines_inside_the_frontmatter_change() {
        let text = "---\nnested:\n  enabled: true\nname: n\n---\nenabled: true\n";
        let off = changed(text, false);
        assert_eq!(off, "---\nnested:\n  enabled: true\nname: n\nenabled: false\n---\nenabled: true\n");
        assert!(!reads(&off));
        assert_eq!(changed("---\nenabledx: true\n---\n", false), "---\nenabledx: true\nenabled: false\n---\n");
    }

    #[test]
    fn a_byte_order_mark_and_crlf_stay() {
        let windows = format!("\u{feff}{}", JOBNET.replace('\n', "\r\n"));
        let on = changed(&windows, true);
        assert_eq!(on, format!("\u{feff}{}", JOBNET.replace("enabled: false  #", "enabled: true  #").replace('\n', "\r\n")));
        assert!(reads(&on));
        let off = changed("\u{feff}---\r\nname: n\r\n---\r\n", false);
        assert_eq!(off, "\u{feff}---\r\nname: n\r\nenabled: false\r\n---\r\n");
        assert!(!reads(&off));
        let old_mac = changed("---\rname: n\r---\r", false);
        assert_eq!(old_mac, "---\rname: n\renabled: false\r---\r");
        assert!(!reads(&old_mac));
    }

    /// Every shape `parse_manifest` reads as a line reads back as the state asked for, both ways.
    #[test]
    fn each_line_shape_round_trips_through_the_reader() {
        let shapes = [
            "enabled: false",
            "enabled: FALSE",
            "enabled: \"false\"",
            "enabled: 'False'  # off for now",
            "enabled:false",
            "enabled: false\t# off for now",
            "enabled : false",
            "enabled:\tfalse",
            "enabled: true # not false",
            "enabled: #false",
            "enabled: yes",
            "enabled: fa#lse",
        ];
        for shape in shapes {
            let text = format!("---\nname: n\n{shape}\n---\n\nenabled: false in prose\n");
            for on in [true, false] {
                let after = match set_enabled(&text, on) {
                    Edit::Changed(after) => after,
                    Edit::Unchanged => text.clone(),
                    Edit::NoFrontmatter => panic!("{shape}"),
                };
                assert_eq!(reads(&after), on, "{shape} turned {on}");
                assert!(after.ends_with("\n---\n\nenabled: false in prose\n"), "{shape}");
            }
        }
    }
}
