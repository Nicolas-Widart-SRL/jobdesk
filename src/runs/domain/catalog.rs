use crate::runs::domain::Match;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandGroup {
    Find,
    Apply,
    Analyse,
    Sync,
    Profile,
}

impl CommandGroup {
    pub const ALL: [Self; 5] = [Self::Find, Self::Apply, Self::Analyse, Self::Sync, Self::Profile];

    pub fn label(self) -> &'static str {
        match self {
            Self::Find => "Find",
            Self::Apply => "Apply",
            Self::Analyse => "Analyse",
            Self::Sync => "Sync",
            Self::Profile => "Profile",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FieldSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub flag: Option<&'static str>,
    pub default: &'static str,
    pub multiline: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub summary: &'static str,
    pub writes: &'static str,
    pub group: CommandGroup,
    pub fields: &'static [FieldSpec],
    pub budget_usd: u32,
    pub destructive: bool,
}

const fn text(key: &'static str, label: &'static str) -> FieldSpec {
    FieldSpec { key, label, flag: None, default: "", multiline: false }
}

const fn flag(key: &'static str, label: &'static str, flag: &'static str, default: &'static str) -> FieldSpec {
    FieldSpec { key, label, flag: Some(flag), default, multiline: false }
}

const fn spec(
    name: &'static str,
    summary: &'static str,
    writes: &'static str,
    group: CommandGroup,
    fields: &'static [FieldSpec],
    budget_usd: u32,
) -> CommandSpec {
    CommandSpec { name, summary, writes, group, fields, budget_usd, destructive: false }
}

/// Forms only build the argument string. Each command's own spec stays the authority.
pub const CATALOG: &[CommandSpec] = &[
    spec("/scrape", "Find new postings on installed portals", "seen_jobs.json", CommandGroup::Find, &[text("focus", "Focus area, broad, or health")], 2),
    spec(
        "/rank",
        "Score new postings and build a shortlist",
        "seen_jobs.json",
        CommandGroup::Find,
        &[text("focus", "Focus area"), flag("limit", "Maximum jobs to score", "--limit", "10"), flag("top", "Shortlist size", "--top", "5")],
        3,
    ),
    spec(
        "/apply",
        "Evaluate a posting, then draft and review documents",
        "cv, cover_letters, tracker",
        CommandGroup::Apply,
        &[FieldSpec { key: "posting", label: "Posting URL or pasted text", flag: None, default: "", multiline: true }],
        5,
    ),
    spec("/interview", "Build a prep pack for one application", "interview_prep_<stage>.md", CommandGroup::Apply, &[text("target", "Company and role")], 2),
    spec(
        "/outcome",
        "Record progress, follow up, or sweep stale rows",
        "tracker, outcome.md",
        CommandGroup::Apply,
        &[text("target", "Company, followup [N], or stale [N]")],
        2,
    ),
    spec(
        "/upskill",
        "Find skill gaps and build a learning plan",
        "upskill/report-*.md",
        CommandGroup::Analyse,
        &[text("url", "Posting URL, empty for all jobs")],
        3,
    ),
    spec("/html-report", "Rebuild the portable HTML dashboard", "reports/*.html", CommandGroup::Analyse, &[text("path", "Output path, optional")], 1),
    spec(
        "/gmail-sync",
        "Read application mail and update status",
        "tracker, state.json",
        CommandGroup::Sync,
        &[text("target", "Company, or since YYYY-MM-DD")],
        2,
    ),
    spec(
        "/notion-sync",
        "Push ranked jobs and applications to Notion",
        "notion_sync.json",
        CommandGroup::Sync,
        &[flag("score", "Minimum score", "--min-score", "60")],
        2,
    ),
    spec(
        "/setup",
        "Build or update your profile, calibrate scoring",
        "skill files",
        CommandGroup::Profile,
        &[flag("section", "Section, optional", "--section", "")],
        5,
    ),
    spec("/expand", "Find new competency signals in your documents", "skill files", CommandGroup::Profile, &[], 3),
    spec("/add-portal", "Generate a search skill for a job board", ".agents/skills/", CommandGroup::Profile, &[text("url", "Portal URL")], 3),
    spec("/add-template", "Register a CV or cover letter template", "templates/", CommandGroup::Profile, &[text("path", "Template file path")], 2),
    spec(
        "/build-cv",
        "Build CV PDFs from their .tex files",
        "cv/*.pdf",
        CommandGroup::Profile,
        &[text("name", "CV name, --all, --list, or empty for the master")],
        1,
    ),
    CommandSpec {
        destructive: true,
        ..spec(
            "/reset",
            "Clear profile data, documents, or both",
            "skill files, documents",
            CommandGroup::Profile,
            &[text("scope", "profile, documents, or all")],
            1,
        )
    },
];

pub fn spec_for_line(line: &str) -> Option<&'static CommandSpec> {
    let name = line.split_whitespace().next()?;
    CATALOG.iter().find(|spec| spec.name == name)
}

/// A canned answer to a stop point's question.
#[derive(Debug, PartialEq, Eq)]
pub struct QuickReply {
    pub button: &'static str,
    pub text: &'static str,
}

/// A step of the command's rail at which it ends its turn and waits for the user.
#[derive(Debug, PartialEq, Eq)]
pub struct StopPoint {
    /// The label of that step in `StepRail::labels`.
    pub after: &'static str,
    pub label: &'static str,
    pub replies: &'static [QuickReply],
}

const APPLY_STOPS: &[StopPoint] = &[StopPoint {
    after: "Evaluate",
    label: "proceed with drafting",
    replies: &[
        QuickReply { button: "Go ahead with the draft", text: "Yes, proceed with drafting." },
        QuickReply { button: "Stop here", text: "No, stop here. Do not draft." },
    ],
}];

/// The stop points of the command on `line`, found by its command word like `spec_for_line`.
pub fn stop_points(line: &str) -> &'static [StopPoint] {
    match line.split_whitespace().next() {
        Some("/apply") => APPLY_STOPS,
        _ => &[],
    }
}

/// What one kind of tool call is called in a step's sentence. Both texts are fixed here, so no run text reaches the sentence.
#[derive(Debug, PartialEq, Eq)]
pub struct StepPhrase {
    pub on: Match,
    /// A lowercase clause for a finished call: "read your profile".
    pub done: &'static str,
    /// A full sentence for a call still running.
    pub now: &'static str,
}

const fn phrase(on: Match, done: &'static str, now: &'static str) -> StepPhrase {
    StepPhrase { on, done, now }
}

const EDIT: &[&str] = &["Edit", "MultiEdit"];

const APPLY_PHRASES: &[StepPhrase] = &[
    phrase(Match::Agent(&["posting-fetcher"]), "fetched the posting", "Fetching the posting now."),
    phrase(Match::Program { names: &["posting_guard"], args: &[] }, "checked it for hidden instructions", "Checking the posting now."),
    phrase(Match::Program { names: &["cli.ts"], args: &[] }, "read it from the job board", "Reading the job board now."),
    phrase(Match::File { tools: &["Read"], any_of: &["01-candidate-profile"] }, "read your profile", "Reading your profile now."),
    phrase(Match::File { tools: &["Read"], any_of: &["04-job-evaluation"] }, "read the evaluation guide", "Reading the evaluation guide now."),
    phrase(Match::Program { names: &["salary_lookup"], args: &[] }, "looked up salary data", "Looking up salary data now."),
    phrase(Match::File { tools: &["Read"], any_of: &["05-cv-tailoring"] }, "read the CV guide", "Reading the CV guide now."),
    phrase(Match::File { tools: &["Read"], any_of: &["cv/templates/"] }, "read the CV templates", "Reading the CV templates now."),
    phrase(Match::File { tools: &["Write"], any_of: &["cv/main_"] }, "wrote the CV", "Writing the CV now."),
    phrase(Match::File { tools: &["Write"], any_of: &["cover_letters/cover_"] }, "wrote the cover letter", "Writing the cover letter now."),
    phrase(Match::Agent(&["application-reviewer"]), "had the reviewer critique both documents", "The reviewer is reading both documents now."),
    phrase(Match::File { tools: &["WebFetch"], any_of: &[""] }, "read the company's web pages", "Reading the company's web pages now."),
    phrase(Match::File { tools: EDIT, any_of: &["cv/main_"] }, "edited the CV", "Editing the CV now."),
    phrase(Match::File { tools: EDIT, any_of: &["cover_letters/cover_"] }, "edited the cover letter", "Editing the cover letter now."),
    phrase(Match::Program { names: &["lualatex", "xelatex"], args: &[] }, "built the PDFs", "Building the PDFs now."),
    phrase(Match::Program { names: &["verify_pdf"], args: &[] }, "checked the page count", "Checking the page count now."),
    phrase(Match::Program { names: &["verify_layout"], args: &[] }, "checked the layout", "Checking the layout now."),
    phrase(Match::File { tools: &["Write", "Edit", "MultiEdit"], any_of: &["job_search_tracker.csv"] }, "updated the tracker", "Updating the tracker now."),
    phrase(Match::File { tools: &["Write"], any_of: &["job_posting.md"] }, "archived the posting", "Archiving the posting now."),
];

/// The phrases of the command on `line`, found by its command word like `spec_for_line`. A command without any has no step sentences.
pub fn phrases_for(line: &str) -> &'static [StepPhrase] {
    match line.split_whitespace().next() {
        Some("/apply") => APPLY_PHRASES,
        _ => &[],
    }
}

/// One user message. Only multi-line fields may carry line breaks.
pub fn compose_line(spec: &CommandSpec, value_of: impl Fn(&FieldSpec) -> String) -> String {
    let mut line = spec.name.to_owned();
    for field in spec.fields {
        let raw = value_of(field);
        let value = if field.multiline { raw.trim().to_owned() } else { raw.split_whitespace().collect::<Vec<_>>().join(" ") };
        if value.is_empty() {
            continue;
        }
        line.push(' ');
        if let Some(flag) = field.flag {
            line.push_str(flag);
            line.push(' ');
        }
        line.push_str(&value);
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::domain::StepRail;

    #[test]
    fn composes_flags_and_skips_empty_fields() {
        let rank = spec_for_line("/rank --all").unwrap();
        let line = compose_line(rank, |field| if field.key == "focus" { String::new() } else { field.default.to_owned() });
        assert_eq!(line, "/rank --limit 10 --top 5");
    }

    #[test]
    fn single_line_fields_lose_line_breaks() {
        let outcome = spec_for_line("/outcome").unwrap();
        assert_eq!(compose_line(outcome, |_| "acme\n/reset all".into()), "/outcome acme /reset all");
    }

    #[test]
    fn only_apply_stops_to_ask() {
        let stops = stop_points("/apply https://x.example");
        assert_eq!((stops.len(), stops[0].after), (1, "Evaluate"));
        assert_eq!(stops[0].replies[1].text, "No, stop here. Do not draft.");
        assert!(stop_points("/rank").is_empty() && stop_points("").is_empty());
    }

    #[test]
    fn every_stop_names_a_step_of_its_rail() {
        for spec in CATALOG {
            let labels = StepRail::for_line(spec.name).map_or(&[][..], |rail| rail.labels);
            assert!(stop_points(spec.name).iter().all(|stop| labels.contains(&stop.after)), "{}", spec.name);
        }
        assert!(!stop_points("/apply x").is_empty());
    }

    #[test]
    fn only_apply_has_step_phrases() {
        assert_eq!(phrases_for("/apply x").len(), 19);
        assert!(phrases_for("/rank").is_empty() && phrases_for("").is_empty());
    }

    #[test]
    fn phrase_clauses_are_lowercase_and_sentences_end_in_a_stop() {
        for phrase in phrases_for("/apply") {
            assert!(phrase.done.starts_with(char::is_lowercase) && !phrase.done.ends_with('.'), "{}", phrase.done);
            assert!(phrase.now.starts_with(char::is_uppercase) && phrase.now.ends_with('.'), "{}", phrase.now);
        }
    }

    #[test]
    fn every_command_is_listed_once_and_only_reset_is_destructive() {
        assert_eq!(CATALOG.len(), 15);
        assert!(CATALOG.iter().all(|spec| spec.destructive == (spec.name == "/reset")));
    }

    #[test]
    fn build_cv_takes_one_plain_name() {
        let build = spec_for_line("/build-cv main_x").unwrap();
        assert_eq!((build.group, build.fields.len(), build.fields[0].flag), (CommandGroup::Profile, 1, None));
        assert_eq!(compose_line(build, |_| " main_x \n".into()), "/build-cv main_x");
        assert_eq!(compose_line(build, |_| String::new()), "/build-cv");
        let profile: Vec<_> = CATALOG.iter().filter(|spec| spec.group == CommandGroup::Profile).map(|spec| spec.name).collect();
        assert_eq!(profile, ["/setup", "/expand", "/add-portal", "/add-template", "/build-cv", "/reset"]);
    }
}
