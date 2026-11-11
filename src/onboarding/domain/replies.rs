//! What first-run setup says back to `/setup`, and the plain lines the profile card shows while it works. The replies
//! are sent to Claude as the user's decision, so each is built from the numbers Claude wrote and nothing else.

/// The reply to the CV check that says the PDF is fine.
pub const CV_LOOKS_GOOD: &str = "The CV looks good.";

/// The reply that resumes a profile run that stopped, on its own session.
pub const PICK_UP_REPLY: &str = "Please pick up where you stopped.";

/// What an answer field holds when the user left it empty, so the numbers of the reply always match Claude's.
const SKIPPED: &str = "Skipped";

/// The one reply to a batch of numbered questions: a line `<n>. <answer>` for each question, in Claude's order. Each
/// answer is trimmed and keeps its own line breaks, and an empty one is sent as "Skipped". Every number appears once:
/// the first answer given for a number counts. `answers` holds each question's number as Claude wrote it.
pub fn joined_reply<'a>(answers: impl IntoIterator<Item = (u32, &'a str)>) -> String {
    let mut answers: Vec<(u32, &str)> = answers.into_iter().collect();
    answers.sort_by_key(|(number, _)| *number);
    answers.dedup_by_key(|(number, _)| *number);
    let lines: Vec<String> = answers
        .into_iter()
        .map(|(number, answer)| {
            let answer = answer.trim();
            format!("{number}. {}", if answer.is_empty() { SKIPPED } else { answer })
        })
        .collect();
    lines.join("\n")
}

/// The reply to a proposal of numbered changes, from `ticks`: each change's own number as Claude wrote it, and whether
/// the user left it ticked. A number is never read from a position, so the order of `ticks` cannot shift one.
///
/// All ticked is "Yes, apply these changes.". Some unticked names only those, ascending: "Apply all except 3 and 7.".
/// None ticked, or no changes at all, has no reply, because nothing is applied.
pub fn review_reply(ticks: impl IntoIterator<Item = (u32, bool)>) -> Option<String> {
    let mut ticks: Vec<(u32, bool)> = ticks.into_iter().collect();
    ticks.sort_by_key(|(number, _)| *number);
    ticks.dedup_by_key(|(number, _)| *number);
    if ticks.iter().all(|(_, ticked)| !ticked) {
        return None;
    }
    let skipped: Vec<u32> = ticks.iter().filter(|(_, ticked)| !ticked).map(|(number, _)| *number).collect();
    if skipped.is_empty() {
        return Some("Yes, apply these changes.".to_owned());
    }
    Some(format!("Apply all except {}.", english_list(&skipped)))
}

/// "3", "3 and 7", "3, 5 and 7".
fn english_list(numbers: &[u32]) -> String {
    let words: Vec<String> = numbers.iter().map(u32::to_string).collect();
    match words.split_last() {
        None => String::new(),
        Some((only, [])) => only.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// What lights a progress line. Jobdesk's own events light three of them, and a tool call lights the rest.
#[derive(Debug, PartialEq, Eq)]
pub enum LitBy {
    /// Jobdesk's own copy of the documents, before the run starts.
    Copy,
    /// Claude's first message after the reads, before the first batch of questions.
    FirstMessage,
    /// The writes that come after the reply to the review card.
    ReviewReply,
    /// A tool call: its name, and either a path relative to the job search folder or, for `Bash`, a program in the command.
    Tool { tools: &'static [&'static str], paths: &'static [&'static str], programs: &'static [&'static str] },
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProgressLine {
    pub text: &'static str,
    pub lit_by: LitBy,
}

const WRITES: &[&str] = &["Write", "Edit", "MultiEdit"];

/// The plain lines the card shows while `/setup` writes the profile, in the order of the handoff's table. A path ending
/// in `/` stands for everything under it.
pub const PROGRESS_LINES: [ProgressLine; 7] = [
    ProgressLine { text: "Documents copied into your folder", lit_by: LitBy::Copy },
    ProgressLine { text: "Reading your documents", lit_by: LitBy::Tool { tools: &["Read"], paths: &["documents/"], programs: &[] } },
    ProgressLine { text: "Comparing your CV with your other documents", lit_by: LitBy::FirstMessage },
    ProgressLine {
        text: "Writing your profile",
        lit_by: LitBy::Tool { tools: WRITES, paths: &["CLAUDE.md", ".claude/skills/job-application-assistant/"], programs: &[] },
    },
    ProgressLine { text: "Applying the changes you approved", lit_by: LitBy::ReviewReply },
    ProgressLine { text: "Making your CV", lit_by: LitBy::Tool { tools: WRITES, paths: &["cv/"], programs: &["lualatex", "xelatex", "pdflatex", "latexmk"] } },
    ProgressLine {
        text: "Choosing job sites and search terms",
        lit_by: LitBy::Tool { tools: WRITES, paths: &[".agents/skills/", ".claude/skills/job-scraper/search-queries.md"], programs: &[] },
    },
];

/// The line a tool call lights: the first tool line whose tool is `tool` and whose paths hold `summary`, a path
/// relative to the job search folder, or, for `Bash`, whose programs the command `summary` runs. The lines Jobdesk's own
/// events light are never returned, because only Jobdesk knows when those happen.
pub fn tool_line(tool: &str, summary: &str) -> Option<&'static str> {
    let path = summary.replace('\\', "/");
    let path = path.trim_start_matches("./");
    PROGRESS_LINES.iter().find_map(|line| match &line.lit_by {
        LitBy::Tool { tools, paths, .. } if tools.contains(&tool) && paths.iter().any(|at| under(path, at)) => Some(line.text),
        LitBy::Tool { programs, .. } if tool == "Bash" && programs.iter().any(|program| summary.contains(program)) => Some(line.text),
        _ => None,
    })
}

/// Whether `path` is `at`, or, when `at` ends in `/`, anything under it.
fn under(path: &str, at: &str) -> bool {
    if at.ends_with('/') { path.starts_with(at) } else { path == at }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joined_reply_numbers_each_answer_in_order() {
        assert_eq!(joined_reply([(1, "Design Lead"), (2, "The whole studio")]), "1. Design Lead\n2. The whole studio");
    }

    #[test]
    fn an_empty_or_blank_answer_is_skipped() {
        assert_eq!(joined_reply([(1, ""), (2, "  \n\t "), (3, "Lowmere")]), "1. Skipped\n2. Skipped\n3. Lowmere");
    }

    #[test]
    fn an_answer_is_trimmed_and_keeps_its_inner_line_breaks() {
        assert_eq!(joined_reply([(1, "  first line\n  second line\n\n")]), "1. first line\n  second line");
    }

    #[test]
    fn a_number_appears_once_and_keeps_its_own_number() {
        assert_eq!(joined_reply([(3, "c"), (1, "a"), (3, "again"), (2, "b")]), "1. a\n2. b\n3. c");
        assert_eq!(joined_reply([(1, "a"), (2, "b"), (4, "d")]), "1. a\n2. b\n4. d");
        assert_eq!(joined_reply([]), "");
    }

    #[test]
    fn all_ticked_applies_everything() {
        assert_eq!(review_reply([(1, true), (2, true), (3, true)]).as_deref(), Some("Yes, apply these changes."));
    }

    #[test]
    fn unticked_changes_are_named_as_english() {
        assert_eq!(review_reply([(1, true), (2, true), (3, false)]).as_deref(), Some("Apply all except 3."));
        assert_eq!(review_reply([(3, false), (4, true), (7, false)]).as_deref(), Some("Apply all except 3 and 7."));
        assert_eq!(review_reply([(3, false), (5, false), (6, true), (7, false)]).as_deref(), Some("Apply all except 3, 5 and 7."));
        assert_eq!(review_reply([(1, false), (2, false), (3, false), (4, false), (5, true)]).as_deref(), Some("Apply all except 1, 2, 3 and 4."));
    }

    #[test]
    fn none_ticked_has_no_reply() {
        assert_eq!(review_reply([(1, false), (2, false), (3, false)]), None);
        assert_eq!(review_reply([(1, false)]), None);
        assert_eq!(review_reply([]), None);
    }

    #[test]
    fn the_numbers_come_from_the_changes_and_not_from_their_position() {
        // Reordered: the unticked 7 comes first, and still follows the 3.
        assert_eq!(review_reply([(7, false), (5, true), (3, false)]).as_deref(), Some("Apply all except 3 and 7."));
        // Gapped: the second entry is change 4, so it is named 4 and never 2.
        assert_eq!(review_reply([(1, true), (4, false), (9, true)]).as_deref(), Some("Apply all except 4."));
        // A number given twice counts once.
        assert_eq!(review_reply([(2, false), (2, false), (3, true)]).as_deref(), Some("Apply all except 2."));
    }

    #[test]
    fn the_fixed_replies_read_as_the_user_would_say_them() {
        assert_eq!(CV_LOOKS_GOOD, "The CV looks good.");
        assert_eq!(PICK_UP_REPLY, "Please pick up where you stopped.");
    }

    #[test]
    fn the_progress_lines_are_the_handoffs_table_word_for_word() {
        let texts: Vec<&str> = PROGRESS_LINES.iter().map(|line| line.text).collect();
        assert_eq!(
            texts,
            [
                "Documents copied into your folder",
                "Reading your documents",
                "Comparing your CV with your other documents",
                "Writing your profile",
                "Applying the changes you approved",
                "Making your CV",
                "Choosing job sites and search terms",
            ]
        );
    }

    #[test]
    fn a_tool_call_lights_its_line() {
        assert_eq!(tool_line("Read", "documents/cv.pdf"), Some("Reading your documents"));
        assert_eq!(tool_line("Write", "CLAUDE.md"), Some("Writing your profile"));
        assert_eq!(tool_line("Edit", ".claude/skills/job-application-assistant/SKILL.md"), Some("Writing your profile"));
        assert_eq!(tool_line("Write", "cv/main.tex"), Some("Making your CV"));
        assert_eq!(tool_line("Bash", "cd cv && lualatex main.tex"), Some("Making your CV"));
        assert_eq!(tool_line("Write", ".agents/skills/portals/harbor.md"), Some("Choosing job sites and search terms"));
        assert_eq!(tool_line("Edit", ".claude/skills/job-scraper/search-queries.md"), Some("Choosing job sites and search terms"));
        assert_eq!(tool_line("Write", ".\\cv\\main.tex"), Some("Making your CV"));
    }

    #[test]
    fn other_calls_light_nothing() {
        assert_eq!(tool_line("Write", "documents/cv.pdf"), None);
        assert_eq!(tool_line("Read", "cv/main.tex"), None);
        assert_eq!(tool_line("Write", "notes/CLAUDE.md"), None);
        assert_eq!(tool_line("Write", "CLAUDE.md.bak"), None);
        assert_eq!(tool_line("Write", "cvs/old.tex"), None);
        assert_eq!(tool_line("Read", "/Users/someone/documents/cv.pdf"), None);
        assert_eq!(tool_line("Bash", "ls documents"), None);
        assert_eq!(tool_line("Read", ".claude/skills/job-scraper/search-queries.md"), None);
    }
}
