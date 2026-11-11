//! What first-run setup says back to `/setup`, and the plain lines the profile card shows while it works. The replies
//! are sent to Claude as the user's decision, so each is built from the numbers Claude wrote and nothing else.

/// Jobdesk's one reply of its own: the answer to `/setup`'s opening question, which picks the documents-folder path.
/// The words are the path's own in ai-job-search v1.7.5, `.claude/commands/setup.md` line 44: "**Path A: Read my
/// documents folder**". Jobdesk copies the documents straight into `documents/`, not into the subfolders its inventory
/// lists, so the reply says where they are.
pub const PATH_A_REPLY: &str =
    "Path A: Read my documents folder. My documents are the PDF files directly in documents/, not in its subfolders, so please read each of them.";

/// The reply to the CV check that says the PDF is fine.
pub const CV_LOOKS_GOOD: &str = "The CV looks good.";

/// The reply that resumes a profile run that stopped, on its own session.
pub const PICK_UP_REPLY: &str = "Please pick up where you stopped.";

/// What an answer field holds when the user left it empty, so the numbers of the reply always match Claude's.
const SKIPPED: &str = "Skipped";

/// The one reply to a batch of numbered questions: a line `<n>. <answer>` for each of `numbers`, the numbers Claude
/// wrote, in Claude's order, each once. `answers` holds the user's answer by number, and the first one given for a
/// number counts. An answer is trimmed, and an empty one, or none, is sent as "Skipped", so no number is ever missing.
/// Each line after the first of a multi-line answer is indented by three spaces and a blank one stays blank, so no line
/// of an answer can start like a numbered item of the reply.
pub fn joined_reply(numbers: &[u32], answers: &[(u32, &str)]) -> String {
    let mut seen = Vec::new();
    let mut lines = Vec::new();
    for &number in numbers {
        if seen.contains(&number) {
            continue;
        }
        seen.push(number);
        let answer = answers.iter().find(|(at, _)| *at == number).map_or("", |(_, answer)| answer.trim());
        lines.push(format!("{number}. {}", if answer.is_empty() { SKIPPED.to_owned() } else { indented(answer) }));
    }
    lines.join("\n")
}

/// The answer with every line after the first moved right by three spaces, blank lines left empty.
fn indented(answer: &str) -> String {
    let mut lines = answer.lines();
    let mut text = lines.next().unwrap_or_default().to_owned();
    for line in lines {
        text.push('\n');
        if !line.trim().is_empty() {
            text.push_str("   ");
            text.push_str(line);
        }
    }
    text
}

/// The reply to a proposal of numbered changes. `numbers` are the changes' own numbers as Claude wrote them, and `ticks`
/// holds whether the user left each one ticked, by number, so the order of `ticks` cannot shift a number.
///
/// All ticked is "Yes, apply these changes.". Some unticked names only those, ascending: "Apply all except 3 and 7.".
/// None ticked has no reply, because nothing is applied. A change with no tick, or with two that differ, also has none:
/// the reply is never built from a guess about what the user chose, and a change left out would count as applied.
pub fn review_reply(numbers: &[u32], ticks: &[(u32, bool)]) -> Option<String> {
    let mut skipped = Vec::new();
    let mut ticked = 0;
    for &number in numbers {
        let mut states = ticks.iter().filter(|(at, _)| *at == number).map(|(_, state)| *state);
        let state = states.next()?;
        if states.any(|other| other != state) {
            return None;
        }
        if state {
            ticked += 1;
        } else if !skipped.contains(&number) {
            skipped.push(number);
        }
    }
    if ticked == 0 {
        return None;
    }
    skipped.sort_unstable();
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

/// The line a tool call lights: the first tool line whose tool is `tool` and whose paths hold `summary`, or, for `Bash`,
/// whose programs the command `summary` runs. `root` is the job search folder and `summary` the path as Claude gave it,
/// absolute or relative to the folder. The path is made relative to `root` and `.` and `..` are resolved by reading,
/// never on disk. A path outside `root`, or one that climbs out of it, lights nothing. The lines Jobdesk's own events
/// light are never returned, because only Jobdesk knows when those happen.
pub fn tool_line(root: &str, tool: &str, summary: &str) -> Option<&'static str> {
    let path = relative_to(root, summary);
    PROGRESS_LINES.iter().find_map(|line| match &line.lit_by {
        LitBy::Tool { tools, paths, programs } => {
            let by_path = tools.contains(&tool) && path.as_deref().is_some_and(|path| paths.iter().any(|at| under(path, at)));
            let by_program = tool == "Bash" && runs_program(summary, programs);
            (by_path || by_program).then_some(line.text)
        }
        _ => None,
    })
}

/// Whether `path` is `at`, or, when `at` ends in `/`, anything under it.
fn under(path: &str, at: &str) -> bool {
    if at.ends_with('/') { path.starts_with(at) } else { path == at }
}

/// The parts of `path` with `.` and `..` resolved by reading, or `None` when a relative path climbs above its start. An
/// absolute path cannot climb above its top. Both `/` and `\` separate parts.
fn parts(path: &str) -> Option<Vec<String>> {
    let path = path.replace('\\', "/");
    let absolute = path.starts_with('/') || path.chars().nth(1) == Some(':');
    let mut parts: Vec<String> = Vec::new();
    for part in path.split('/').filter(|part| !part.is_empty() && *part != ".") {
        if part != ".." {
            parts.push(part.to_owned());
        } else if parts.pop().is_none() && !absolute {
            return None;
        }
    }
    Some(parts)
}

/// `path` as a path under `root` with `/` between its parts, or `None` when it is outside `root` or is `root` itself.
fn relative_to(root: &str, path: &str) -> Option<String> {
    let root = parts(root)?;
    let absolute = path.starts_with(['/', '\\']) || path.chars().nth(1) == Some(':');
    let path = parts(path)?;
    let rest = if absolute {
        // A drive letter is the same in either case.
        let inside = path.len() > root.len() && root.iter().zip(&path).enumerate().all(|(at, (a, b))| a == b || at == 0 && a.eq_ignore_ascii_case(b));
        inside.then(|| &path[root.len()..])?
    } else {
        &path[..]
    };
    (!rest.is_empty()).then(|| rest.join("/"))
}

/// Whether a command runs one of `programs` as the command word of a command: the first word, or the first after `&&`,
/// `||`, `;`, `|` or a line break. A program named as an argument or inside a word does not count.
fn runs_program(command: &str, programs: &[&str]) -> bool {
    command.split(['&', '|', ';', '\n']).any(|segment| {
        let word = segment.split_whitespace().next().unwrap_or_default().trim_matches(['"', '\'']);
        let name = word.rsplit(['/', '\\']).next().unwrap_or_default();
        programs.contains(&name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(ticks: &[(u32, bool)]) -> Option<String> {
        let numbers: Vec<u32> = ticks.iter().map(|(number, _)| *number).collect();
        review_reply(&numbers, ticks)
    }

    #[test]
    fn joined_reply_numbers_each_answer_in_order() {
        assert_eq!(joined_reply(&[1, 2], &[(1, "Design Lead"), (2, "The whole studio")]), "1. Design Lead\n2. The whole studio");
    }

    #[test]
    fn an_empty_or_blank_answer_is_skipped() {
        assert_eq!(joined_reply(&[1, 2, 3], &[(1, ""), (2, "  \n\t "), (3, "Lowmere")]), "1. Skipped\n2. Skipped\n3. Lowmere");
    }

    #[test]
    fn an_answer_is_trimmed() {
        assert_eq!(joined_reply(&[1], &[(1, "  one line \n\n")]), "1. one line");
    }

    #[test]
    fn every_number_appears_once_in_claudes_order() {
        // The answer for 3 is missing, and 3 is still there.
        assert_eq!(joined_reply(&[1, 2, 3, 4], &[(1, "a"), (2, "b"), (4, "d")]), "1. a\n2. b\n3. Skipped\n4. d");
        // The order is the numbers', not the answers'.
        assert_eq!(joined_reply(&[1, 2, 3], &[(3, "c"), (1, "a"), (2, "b")]), "1. a\n2. b\n3. c");
        // A number given twice, as an answer or in the list, counts once.
        assert_eq!(joined_reply(&[1, 2, 2], &[(2, "b"), (1, "a"), (2, "again")]), "1. a\n2. b");
        // An answer to a number Claude never asked is left out.
        assert_eq!(joined_reply(&[1], &[(1, "a"), (9, "stray")]), "1. a");
        assert_eq!(joined_reply(&[], &[(1, "a")]), "");
    }

    #[test]
    fn a_multi_line_answer_cannot_start_a_line_like_an_item() {
        let answer = "1. Design lead\n2. Principal designer";
        assert_eq!(joined_reply(&[1, 2], &[(1, answer)]), "1. 1. Design lead\n   2. Principal designer\n2. Skipped");
    }

    #[test]
    fn a_blank_inner_line_of_an_answer_stays_blank() {
        assert_eq!(joined_reply(&[1, 2], &[(1, "first\n\n  second\n   \nthird"), (2, "b")]), "1. first\n\n     second\n\n   third\n2. b");
    }

    #[test]
    fn all_ticked_applies_everything() {
        assert_eq!(review(&[(1, true), (2, true), (3, true)]).as_deref(), Some("Yes, apply these changes."));
    }

    #[test]
    fn unticked_changes_are_named_as_english() {
        assert_eq!(review(&[(1, true), (2, true), (3, false)]).as_deref(), Some("Apply all except 3."));
        assert_eq!(review(&[(3, false), (4, true), (7, false)]).as_deref(), Some("Apply all except 3 and 7."));
        assert_eq!(review(&[(3, false), (5, false), (6, true), (7, false)]).as_deref(), Some("Apply all except 3, 5 and 7."));
        assert_eq!(review(&[(1, false), (2, false), (3, false), (4, false), (5, true)]).as_deref(), Some("Apply all except 1, 2, 3 and 4."));
    }

    #[test]
    fn none_ticked_has_no_reply() {
        assert_eq!(review(&[(1, false), (2, false), (3, false)]), None);
        assert_eq!(review(&[(1, false)]), None);
        assert_eq!(review(&[]), None);
    }

    #[test]
    fn the_numbers_come_from_the_changes_and_not_from_their_position() {
        // Reordered: the unticked 7 comes first, and still follows the 3.
        assert_eq!(review(&[(7, false), (5, true), (3, false)]).as_deref(), Some("Apply all except 3 and 7."));
        // Gapped: the second entry is change 4, so it is named 4 and never 2.
        assert_eq!(review(&[(1, true), (4, false), (9, true)]).as_deref(), Some("Apply all except 4."));
        // The ticks come in another order than the changes.
        assert_eq!(review_reply(&[1, 2, 3], &[(3, true), (2, false), (1, true)]).as_deref(), Some("Apply all except 2."));
    }

    #[test]
    fn a_change_with_no_tick_has_no_reply() {
        // Change 3 is missing from the ticks, and would count as applied.
        assert_eq!(review_reply(&[1, 2, 3], &[(1, true), (2, false)]), None);
        assert_eq!(review_reply(&[1, 2, 3], &[(1, true), (2, true)]), None);
        assert_eq!(review_reply(&[1, 2], &[]), None);
    }

    #[test]
    fn ticks_for_changes_that_do_not_exist_are_ignored() {
        assert_eq!(review_reply(&[1, 2], &[(1, true), (2, true), (9, false)]).as_deref(), Some("Yes, apply these changes."));
    }

    #[test]
    fn two_ticks_that_differ_have_no_reply() {
        assert_eq!(review_reply(&[1, 2], &[(1, true), (2, false), (2, true)]), None);
        assert_eq!(review_reply(&[1, 2], &[(1, true), (2, false), (2, false)]).as_deref(), Some("Apply all except 2."));
    }

    #[test]
    fn the_fixed_replies_read_as_the_user_would_say_them() {
        assert_eq!(CV_LOOKS_GOOD, "The CV looks good.");
        assert_eq!(PICK_UP_REPLY, "Please pick up where you stopped.");
        assert!(PATH_A_REPLY.starts_with("Path A: Read my documents folder."));
        assert!(!PATH_A_REPLY.contains('\n'), "one line, as a typed reply");
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

    const ROOT: &str = "/Users/someone/ai-job-search";

    fn line(tool: &str, path: &str) -> Option<&'static str> {
        tool_line(ROOT, tool, path)
    }

    #[test]
    fn a_tool_call_lights_its_line() {
        assert_eq!(line("Read", "documents/cv.pdf"), Some("Reading your documents"));
        assert_eq!(line("Write", "CLAUDE.md"), Some("Writing your profile"));
        assert_eq!(line("Edit", ".claude/skills/job-application-assistant/SKILL.md"), Some("Writing your profile"));
        assert_eq!(line("Write", "cv/main.tex"), Some("Making your CV"));
        assert_eq!(line("Bash", "cd cv && lualatex main.tex"), Some("Making your CV"));
        assert_eq!(line("Write", ".agents/skills/portals/harbor.md"), Some("Choosing job sites and search terms"));
        assert_eq!(line("Edit", ".claude/skills/job-scraper/search-queries.md"), Some("Choosing job sites and search terms"));
        assert_eq!(line("Write", ".\\cv\\main.tex"), Some("Making your CV"));
    }

    #[test]
    fn an_absolute_path_is_read_from_the_folder() {
        assert_eq!(line("Read", "/Users/someone/ai-job-search/documents/cv.pdf"), Some("Reading your documents"));
        assert_eq!(line("Write", "/Users/someone/ai-job-search/CLAUDE.md"), Some("Writing your profile"));
        assert_eq!(line("Write", "/Users/someone/ai-job-search/cv/main.tex"), Some("Making your CV"));
        assert_eq!(tool_line("C:\\Users\\someone\\job", "Write", "c:\\Users\\someone\\job\\cv\\main.tex"), Some("Making your CV"));
        assert_eq!(tool_line("/Users/someone/ai-job-search/", "Write", "/Users/someone/ai-job-search/CLAUDE.md"), Some("Writing your profile"));
    }

    #[test]
    fn dots_are_resolved_by_reading_and_never_climb_out() {
        assert_eq!(line("Write", "cv/../CLAUDE.md"), Some("Writing your profile"));
        assert_eq!(line("Write", "documents/../cv/main.tex"), Some("Making your CV"));
        assert_eq!(line("Write", "./cv/./main.tex"), Some("Making your CV"));
        assert_eq!(line("Write", "cv/../../CLAUDE.md"), None);
        assert_eq!(line("Write", "../ai-job-search/CLAUDE.md"), None);
        assert_eq!(line("Write", "/Users/someone/ai-job-search/cv/../../other/CLAUDE.md"), None);
        assert_eq!(line("Write", "/Users/someone/ai-job-search/../ai-job-search/CLAUDE.md"), Some("Writing your profile"));
        assert_eq!(line("Write", "/Users/someone/ai-job-search/.."), None);
    }

    #[test]
    fn a_path_outside_the_folder_lights_nothing() {
        assert_eq!(line("Read", "/Users/someone/documents/cv.pdf"), None);
        assert_eq!(line("Write", "/Users/someone/other/CLAUDE.md"), None);
        assert_eq!(line("Write", "/Users/someone/ai-job-search-copy/CLAUDE.md"), None);
        assert_eq!(line("Write", "/Users/someone/ai-job-search"), None);
        assert_eq!(line("Write", "/CLAUDE.md"), None);
    }

    #[test]
    fn other_calls_light_nothing() {
        assert_eq!(line("Write", "documents/cv.pdf"), None);
        assert_eq!(line("Read", "cv/main.tex"), None);
        assert_eq!(line("Write", "notes/CLAUDE.md"), None);
        assert_eq!(line("Write", "CLAUDE.md.bak"), None);
        assert_eq!(line("Write", "cvs/old.tex"), None);
        assert_eq!(line("Bash", "ls documents"), None);
        assert_eq!(line("Read", ".claude/skills/job-scraper/search-queries.md"), None);
    }

    #[test]
    fn a_latex_program_lights_the_cv_line_only_as_a_command_word() {
        assert_eq!(line("Bash", "lualatex main.tex"), Some("Making your CV"));
        assert_eq!(line("Bash", "cd cv; xelatex main.tex"), Some("Making your CV"));
        assert_eq!(line("Bash", "ls | pdflatex x"), Some("Making your CV"));
        assert_eq!(line("Bash", "cd cv && /Library/TeX/bin/lualatex main.tex"), Some("Making your CV"));
        assert_eq!(line("Bash", "latexmk -pdf main.tex"), Some("Making your CV"));
        assert_eq!(line("Bash", "echo lualatex"), None);
        assert_eq!(line("Bash", "cat notes-about-lualatex.txt"), None);
        assert_eq!(line("Bash", "which xelatex"), None);
        assert_eq!(line("Bash", "mylualatex main.tex"), None);
        assert_eq!(line("Write", "lualatex"), None);
    }
}
