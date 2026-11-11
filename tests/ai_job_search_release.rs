//! Whether an `ai-job-search` release has what Jobdesk relies on, before it joins `assets/update/ai-job-search.json`.
//!
//! The ignored test reads the release zip at `JOBDESK_CANDIDATE_ZIP`, the version `JOBDESK_CANDIDATE_VERSION`, the way
//! first-run setup does, and names every file it lacks. It starts no `claude` and costs nothing, and the workflow
//! `.github/workflows/ai-job-search.yml` runs it on every release the list holds, whenever a pull request or a push to
//! `main` changes the list or the code that says what Jobdesk relies on:
//!
//! ```text
//! scripts/ai-job-search.sh download v1.7.5 /tmp/release/source.zip
//! JOBDESK_CANDIDATE_ZIP=/tmp/release/source.zip JOBDESK_CANDIDATE_VERSION=1.7.5 \
//!   cargo test --test ai_job_search_release -- --ignored
//! ```
//!
//! What a release must hold is built from the constants the readers use, so a reader that moves and this check move
//! together. Files that runs create, such as the tracker or `seen_jobs.json`, are not asked for.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use jobdesk::onboarding::application::{DocumentKind, PATH_A, PROPOSAL_WORDS, SETUP, SETUP_COMPLETE};
use jobdesk::portals::application::{BUILT_IN, MANIFEST, SKILLS};
use jobdesk::profile::application::ProfileFile;
use jobdesk::runs::application::CATALOG;
use jobdesk::shared_kernel::file_text::without_bom;
use jobdesk::shared_kernel::installed_source::sha256_hex;
use jobdesk::shared_kernel::release_zip::read_verified;
use jobdesk::updates::application::COMPATIBLE_LIMIT;
use jobdesk::workspace::application::{ACTIVE_TEMPLATE, ASSISTANT, CV_TEMPLATES, LETTER_TEMPLATES, MARKER};

/// The catalog's commands a release may lack, each with why Jobdesk still works without it.
const OPTIONAL: &[(&str, &str)] = &[("/build-cv", "Profile › CV offers a build only when the folder has /build-cv")];

/// The command that writes the `ACTIVE_TEMPLATE` block the template guides are read for.
const TEMPLATE_WRITER: &str = "/add-template";

/// Where Claude Code finds a skill, which is also a slash command of the name its front matter gives, or of its
/// folder's name when the front matter gives none.
const CLAUDE_SKILLS: &str = ".claude/skills";

/// Every file of a release, by its path under the top folder with `/` between parts.
type Files = BTreeMap<String, Vec<u8>>;

/// Where a needed thing is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    /// The file at this path.
    File(String),
    /// The slash command of this name, without its `/`: a command file, or a skill of that name, from its front matter
    /// or else its folder, that the user can invoke.
    Command(&'static str),
}

/// One thing a release must ship, and why Jobdesk needs it.
#[derive(Debug)]
struct Need {
    place: Place,
    /// What the file must hold, or must not, each with why Jobdesk needs that.
    texts: Vec<(Text, &'static str)>,
    why: &'static str,
}

impl Need {
    fn file(path: impl Into<String>, why: &'static str) -> Self {
        Self { place: Place::File(path.into()), texts: Vec::new(), why }
    }
}

/// What a needed file must hold, or must not.
#[derive(Debug, Clone, Copy)]
enum Text {
    /// This text as it is: held (`true`) or never held (`false`).
    Exact(&'static str, bool),
    /// This phrase in any case, as whole words, the way `/setup`'s messages are read.
    Phrase(&'static str),
    /// The folder of this name, as `<folder>/`, not right after a letter or digit.
    Folder(&'static str),
    /// One line that holds each of these words, whole and in any case.
    Line(&'static [&'static str]),
}

impl Text {
    fn met_by(self, text: &str) -> bool {
        match self {
            Self::Exact(wanted, held) => text.contains(wanted) == held,
            Self::Phrase(phrase) => holds_whole(&text.to_lowercase(), &phrase.to_lowercase()),
            Self::Folder(folder) => holds_whole(text, &format!("{folder}/")),
            Self::Line(words) => {
                text.lines().any(|line| words.iter().all(|word| line.split(|c: char| !c.is_alphanumeric()).any(|found| found.eq_ignore_ascii_case(word))))
            }
        }
    }

    /// What the file does wrong, after its path in the line that names an unmet need.
    fn unmet(self) -> String {
        match self {
            Self::Exact(text, true) => format!("does not hold `{text}`"),
            Self::Exact(text, false) => format!("holds `{text}`"),
            Self::Phrase(phrase) => format!("does not hold `{phrase}` in any case, as whole words"),
            Self::Folder(folder) => format!("does not name the folder `{folder}/`"),
            Self::Line(words) => format!("has no line that holds {}, each a whole word in any case", listed(words)),
        }
    }

    /// Text that meets it, for a fictional release, or `None` for text a file must not hold.
    fn sample(self) -> Option<String> {
        match self {
            Self::Exact(text, true) => Some(text.to_owned()),
            Self::Exact(_, false) => None,
            Self::Phrase(phrase) => Some(phrase.to_owned()),
            Self::Folder(folder) => Some(format!("{folder}/")),
            Self::Line(words) => Some(words.join(" ")),
        }
    }
}

/// Whether `text` holds `phrase` with no letter or digit running on at an end of it that is a letter or digit itself.
fn holds_whole(text: &str, phrase: &str) -> bool {
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    let (starts, ends) = (word(phrase.chars().next()), word(phrase.chars().next_back()));
    text.match_indices(phrase).any(|(at, _)| !(starts && word(text[..at].chars().next_back())) && !(ends && word(text[at + phrase.len()..].chars().next())))
}

/// `words` in backticks, as "`a`, `b` and `c`".
fn listed(words: &[&str]) -> String {
    let quoted: Vec<String> = words.iter().map(|word| format!("`{word}`")).collect();
    match quoted.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => quoted.concat(),
    }
}

/// The folder of the command files: the one that holds the file a checkout is known by.
fn commands_folder() -> &'static str {
    MARKER.rsplit_once('/').expect("the marker is in a folder").0
}

/// The command file of the slash command `name`, given without its `/`.
fn command_path(name: &str) -> String {
    format!("{}/{name}.md", commands_folder())
}

/// Everything a release must ship for Jobdesk to work on the folder it sets up.
fn needs() -> Vec<Need> {
    let mut needs = vec![Need::file(MARKER, "Jobdesk opens a folder as a job search folder only when it holds this file")];
    for spec in CATALOG.iter().filter(|spec| !OPTIONAL.iter().any(|(name, _)| *name == spec.name)) {
        let name = spec.name.strip_prefix('/').expect("a command starts with /");
        // The marker is a command file, so its command is needed already, and as that file.
        if command_path(name) == MARKER {
            continue;
        }
        let mut need = Need { place: Place::Command(name), texts: Vec::new(), why: "the launcher starts it" };
        if spec.name == TEMPLATE_WRITER {
            need.why = "the launcher starts it, and it writes the block that says a custom template is active";
            need.texts.push((Text::Exact(ACTIVE_TEMPLATE, true), need.why));
        }
        if spec.name == SETUP {
            need.why = "the launcher starts it, and first-run setup's Profile chapter runs it";
            need.texts = setup_texts();
        }
        needs.push(need);
    }
    for file in ProfileFile::ALL {
        needs.push(Need::file(file.relative_path(), "Profile reads and saves it"));
    }
    for guide in [CV_TEMPLATES, LETTER_TEMPLATES] {
        let why = "Setup reads it for a custom template, so a stock one must not say a custom template is active";
        needs.push(Need { place: Place::File(format!("{ASSISTANT}/{guide}")), texts: vec![(Text::Exact(ACTIVE_TEMPLATE, false), why)], why });
    }
    for (folder, _) in BUILT_IN {
        needs.push(Need::file(format!("{SKILLS}/{folder}/{MANIFEST}"), "Portals lists it as built in"));
    }
    needs
}

/// What `/setup`'s command file must hold for first-run setup's Profile chapter to follow its run.
fn setup_texts() -> Vec<(Text, &'static str)> {
    let mut texts = vec![
        (Text::Phrase(PATH_A), "first-run setup answers the question that names it with the documents path"),
        (Text::Phrase(SETUP_COMPLETE), "first-run setup's Profile chapter is done only on a message that says it"),
        (Text::Line(&PROPOSAL_WORDS), "first-run setup shows a review card only for a proposal that closes with these words"),
    ];
    for kind in DocumentKind::ALL {
        texts.push((Text::Folder(kind.folder()), "first-run setup copies the user's documents into this folder of documents/ for /setup to read"));
    }
    texts
}

/// The value of `key` in a skill's front matter, trimmed, or `None` when the file has no front matter or the front
/// matter does not give it.
fn front_matter(bytes: &[u8], key: &str) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = without_bom(&text).lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    lines.take_while(|line| line.trim_end() != "---").find_map(|line| line.strip_prefix(key)?.strip_prefix(':')).map(|value| value.trim().to_owned())
}

/// The `name:` of a skill's front matter, unquoted.
fn skill_name(bytes: &[u8]) -> Option<String> {
    front_matter(bytes, "name").map(|name| name.trim_matches(['"', '\'']).to_owned())
}

/// Whether the user can start a skill as a slash command: not when its front matter sets `user-invocable` to false.
/// Claude Code reads front matter as YAML, so the value is read the same way, failing closed: a trailing comment
/// (whitespace, then `#`) is dropped, one pair of matching quotes is stripped, and any case of `false` counts. Any
/// other value, or none, counts as invocable.
fn user_invocable(bytes: &[u8]) -> bool {
    let Some(value) = front_matter(bytes, "user-invocable") else {
        return true;
    };
    let comment = value.char_indices().find(|&(at, letter)| letter == '#' && value[..at].ends_with(char::is_whitespace)).map(|(at, _)| at);
    let value = value[..comment.unwrap_or(value.len())].trim();
    let value = ["\"", "'"].into_iter().find_map(|quote| value.strip_prefix(quote)?.strip_suffix(quote)).unwrap_or(value);
    !value.eq_ignore_ascii_case("false")
}

/// Every skill that is named `name`, the user can invoke or not, by its path and bytes. A skill is named by the `name:`
/// of its front matter, or by its folder when it has none, as Claude Code names it: a skill whose `name:` names another
/// command is not this one, whatever its folder.
fn skills_named<'a>(files: &'a Files, name: &'a str) -> impl Iterator<Item = (&'a str, &'a [u8])> + 'a {
    files
        .iter()
        .filter(move |(path, bytes)| skill_folder(path).is_some_and(|folder| skill_name(bytes).as_deref().unwrap_or(folder) == name))
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
}

/// The file that makes `name` a slash command: its command file, or else a skill of that name the user can invoke. A
/// skill whose front matter sets `user-invocable` to false is no slash command, so the launcher cannot start it.
fn command_file<'a>(files: &'a Files, name: &'a str) -> Option<(&'a str, &'a [u8])> {
    if let Some((path, bytes)) = files.get_key_value(&command_path(name)) {
        return Some((path, bytes));
    }
    skills_named(files, name).find(|(_, bytes)| user_invocable(bytes))
}

/// Why the slash command `name` is missing: no command file, and either a skill of that name the user cannot invoke,
/// named by its path, or no skill of that name at all.
fn missing_command(files: &Files, name: &str) -> String {
    let command = command_path(name);
    let hidden: Vec<&str> = skills_named(files, name).map(|(path, _)| path).collect();
    if hidden.is_empty() {
        format!(
            "/{name} is missing: no {command}, and no skill of that name the user can invoke, which is either a {CLAUDE_SKILLS}/<folder>/{MANIFEST} named {name} or a {CLAUDE_SKILLS}/{name}/{MANIFEST} with no name, and in both cases one whose front matter does not set user-invocable to false"
        )
    } else {
        format!(
            "/{name} is missing: no {command}, and the skill of that name, {}, sets user-invocable to false in its front matter, so the user cannot invoke it",
            hidden.join(" and ")
        )
    }
}

/// The folder of a skill's `SKILL.md` right under `CLAUDE_SKILLS`, or `None` for any other path.
fn skill_folder(path: &str) -> Option<&str> {
    path.strip_prefix(CLAUDE_SKILLS)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(|rest| rest.strip_suffix(MANIFEST))
        .and_then(|rest| rest.strip_suffix('/'))
        .filter(|folder| !folder.is_empty() && !folder.contains('/'))
}

/// Every need `files` does not meet, one line each.
fn unmet(files: &Files) -> Vec<String> {
    let mut lines = Vec::new();
    for need in needs() {
        let found = match &need.place {
            Place::File(path) => files.get_key_value(path).map(|(path, bytes)| (path.as_str(), bytes.as_slice())),
            Place::Command(name) => command_file(files, name),
        };
        let Some((path, bytes)) = found else {
            let missing = match &need.place {
                Place::File(path) => format!("{path} is missing"),
                Place::Command(name) => missing_command(files, name),
            };
            lines.push(format!("{missing} ({})", need.why));
            continue;
        };
        let text = String::from_utf8_lossy(bytes);
        for (want, why) in &need.texts {
            if !want.met_by(&text) {
                lines.push(format!("{path} {} ({why})", want.unmet()));
            }
        }
    }
    lines
}

/// An entry's path with `/` between parts, as the needs name it.
fn slashed(path: &Path) -> String {
    path.components()
        .map(|part| match part {
            Component::Normal(part) => part.to_string_lossy().into_owned(),
            other => panic!("an entry of a checked release has only plain parts, not {other:?}"),
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The committed list of compatible releases, as `(version, sha256)` pairs.
fn compatible() -> Vec<(String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/update/ai-job-search.json");
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).expect("the list is committed")).expect("the list is JSON");
    value
        .as_array()
        .expect("the list is an array")
        .iter()
        .map(|entry| (entry["version"].as_str().expect("a version").to_owned(), entry["sha256"].as_str().expect("a sha256").to_owned()))
        .collect()
}

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}: see the top of tests/ai_job_search_release.rs"))
}

#[test]
#[ignore = "needs a downloaded release: set JOBDESK_CANDIDATE_ZIP and JOBDESK_CANDIDATE_VERSION"]
fn the_candidate_release_has_what_jobdesk_relies_on() {
    let zip = required_env("JOBDESK_CANDIDATE_ZIP");
    let version = required_env("JOBDESK_CANDIDATE_VERSION");
    let parts: Vec<&str> = version.split('.').collect();
    assert!(parts.len() == 3 && parts.iter().all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())), "{version} is not X.Y.Z");
    let bytes = fs::read(&zip).unwrap_or_else(|error| panic!("{zip} does not read: {error}"));
    let sha256 = sha256_hex(&bytes);
    match compatible().into_iter().find(|(listed, _)| *listed == version) {
        Some((_, listed)) => assert_eq!(sha256, listed, "{zip} is not the {version} the list pins"),
        None => eprintln!("{version} is not in the list yet, so only what it holds is checked"),
    }
    // Setup's own reading: the size, entry count, names and kinds a release may have.
    let entries = read_verified(&bytes, &sha256).unwrap_or_else(|failure| panic!("Jobdesk refuses {zip}: {failure:?}"));
    let files: Files = entries.into_iter().map(|entry| (slashed(&entry.path), entry.bytes)).collect();
    let unmet = unmet(&files);
    assert!(unmet.is_empty(), "ai-job-search {version} lacks what Jobdesk relies on:\n{}", unmet.join("\n"));
}

#[test]
fn the_required_paths_cover_every_catalog_entry() {
    let needs = needs();
    for spec in CATALOG {
        let name = spec.name.strip_prefix('/').expect("a command starts with /");
        let needed = needs.iter().filter(|need| need.place == Place::Command(name) || need.place == Place::File(command_path(name))).count();
        let optional = OPTIONAL.iter().filter(|(optional, _)| *optional == spec.name).count();
        assert_eq!(needed + optional, 1, "{} is needed or optional, once", spec.name);
    }
    for (name, _) in OPTIONAL {
        assert!(CATALOG.iter().any(|spec| spec.name == *name), "{name} is optional but not in the catalog");
    }
    assert!(CATALOG.iter().any(|spec| spec.name == TEMPLATE_WRITER), "{TEMPLATE_WRITER} is in the catalog");
    assert!(CATALOG.iter().any(|spec| spec.name == SETUP), "{SETUP} is in the catalog");
    assert!(!OPTIONAL.iter().any(|(name, _)| *name == SETUP), "{SETUP} is needed, so what its file must hold is checked");
}

#[test]
fn the_list_holds_at_most_the_feed_limit() {
    let listed = compatible();
    assert!((1..=COMPATIBLE_LIMIT).contains(&listed.len()), "the list holds 1 to {COMPATIBLE_LIMIT} releases, not {}", listed.len());
}

/// A fictional release that meets every need: a command file for each command, and the text each file must hold.
fn complete() -> Files {
    let mut files = Files::new();
    for need in needs() {
        let path = match need.place {
            Place::File(path) => path,
            Place::Command(name) => command_path(name),
        };
        let file = files.entry(path).or_default();
        file.extend_from_slice(b"fictional\n");
        for sample in need.texts.iter().filter_map(|(text, _)| text.sample()) {
            file.extend_from_slice(format!("{sample}\n").as_bytes());
        }
    }
    files
}

#[test]
fn a_complete_release_meets_every_need() {
    assert_eq!(unmet(&complete()), Vec::<String>::new());
}

#[test]
fn every_unmet_need_is_named_at_once() {
    let mut files = complete();
    files.remove(MARKER);
    files.remove(ProfileFile::Candidate.relative_path());
    files.remove(&command_path("rank"));
    let unmet = unmet(&files);
    assert_eq!(unmet.len(), 3, "{unmet:#?}");
    assert!(unmet[0].starts_with(".claude/commands/apply.md is missing"), "{}", unmet[0]);
    assert!(unmet.iter().any(|line| line.starts_with("/rank is missing")), "{unmet:#?}");
    assert!(unmet.iter().any(|line| line.starts_with(".claude/skills/job-application-assistant/01-candidate-profile.md is missing")), "{unmet:#?}");
}

#[test]
fn a_command_may_ship_as_a_skill_of_its_name() {
    let mut files = complete();
    files.remove(&command_path("scrape"));
    files.insert(".claude/skills/job-scraper/SKILL.md".to_owned(), b"---\nname: other\n---\n".to_vec());
    assert_eq!(unmet(&files).len(), 1, "a skill of another name is not /scrape");
    files.insert(".claude/skills/job-scraper/SKILL.md".to_owned(), b"\xef\xbb\xbf---\nname: scrape\ndescription: >\n  Finds jobs.\n---\n".to_vec());
    assert_eq!(unmet(&files), Vec::<String>::new());
    files.insert(".claude/skills/job-scraper/SKILL.md".to_owned(), b"# Scrape\n\nname: scrape\n".to_vec());
    assert_eq!(unmet(&files).len(), 1, "a name outside the front matter is no name");
}

/// Claude Code names a skill after its folder when its front matter gives no name, so such a skill in a folder of the
/// command's name is that command, and one whose `name:` names another command is not.
#[test]
fn a_skill_with_no_name_is_the_command_of_its_folder() {
    let mut files = complete();
    files.remove(&command_path("scrape"));
    for (bytes, why) in [
        (&b"---\ndescription: Finds jobs.\n---\n"[..], "front matter with no name"),
        (&b"# Scrape\n\nFinds jobs.\n"[..], "no front matter"),
        (&b"# Scrape\n\nname: other\n"[..], "a name outside the front matter is no name"),
    ] {
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{why}");
    }
    files.insert(".claude/skills/scrape/SKILL.md".to_owned(), b"---\nname: other\n---\n".to_vec());
    let lines = unmet(&files);
    assert_eq!(lines.len(), 1, "a skill named for another command is not /scrape: {lines:#?}");
    assert!(lines[0].starts_with("/scrape is missing"), "{}", lines[0]);
    files.remove(".claude/skills/scrape/SKILL.md");
    files.insert(".claude/skills/scrape/nested/SKILL.md".to_owned(), b"# Scrape\n".to_vec());
    assert_eq!(unmet(&files).len(), 1, "only a SKILL.md right in the skill's folder counts");
}

/// A skill with `user-invocable: false` is not a slash command the user can start, so the launcher cannot start it,
/// whatever its name.
#[test]
fn a_skill_the_user_cannot_invoke_is_not_the_command() {
    let mut files = complete();
    files.remove(&command_path("scrape"));
    for (bytes, why) in [
        (&b"---\nname: scrape\nuser-invocable: false\n---\n"[..], "named for the command"),
        (&b"---\nuser-invocable: false\ndescription: Finds jobs.\n---\n"[..], "named by its folder"),
        (&b"\xef\xbb\xbf---\nuser-invocable:   false  \nname: scrape\n---\n"[..], "after a BOM, with spaces around the value"),
        (&b"---\nname: scrape\nuser-invocable: false  # internal\n---\n"[..], "a trailing comment"),
        (&b"---\nname: scrape\nuser-invocable: false\t# internal\n---\n"[..], "a trailing comment after a tab"),
        (&b"---\nname: scrape\nuser-invocable: False\n---\n"[..], "False"),
        (&b"---\nname: scrape\nuser-invocable: FALSE\n---\n"[..], "FALSE"),
        (&b"---\nname: scrape\nuser-invocable: \"false\"\n---\n"[..], "double quotes"),
        (&b"---\nname: scrape\nuser-invocable: 'False' # internal\n---\n"[..], "single quotes and a comment"),
    ] {
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: {lines:#?}");
        assert!(lines[0].starts_with("/scrape is missing"), "{why}: {}", lines[0]);
        assert!(
            lines[0].contains(".claude/skills/scrape/SKILL.md, sets user-invocable to false"),
            "{why}: the line names the skill the user cannot invoke: {}",
            lines[0]
        );
    }
    files.remove(".claude/skills/scrape/SKILL.md");
    let lines = unmet(&files);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(
        lines[0].starts_with("/scrape is missing") && !lines[0].contains("the skill of that name,"),
        "no skill at all is not one the user cannot invoke: {}",
        lines[0]
    );
    for (bytes, why) in [
        (&b"---\nname: scrape\nuser-invocable: true\n---\n"[..], "invocable"),
        (&b"---\nname: scrape\nuser-invocable: falsehood\n---\n"[..], "another word"),
        (&b"---\nname: scrape\nuser-invocable: true  # false\n---\n"[..], "false only in the comment"),
        (&b"---\nname: scrape\n---\n"[..], "no user-invocable"),
        (&b"---\nname: scrape\n---\nuser-invocable: false\n"[..], "user-invocable outside the front matter"),
        (&b"# Scrape\n\nuser-invocable: false\n"[..], "no front matter"),
    ] {
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{why}");
    }
}

/// A fictional `/setup` command file in the shape the Profile chapter reads: the path question, the documents
/// subfolders, the proposal's closing ask and the closing summary.
const SETUP_MD: &str = "# /setup\n\n## Step 0\n\n> **Path A: Read my documents folder** - read what is there.\n\n## Step A1\n\n**cv/**, **linkedin/**, **diplomas/**, **references/** and **projects/**.\n\n## Step A6\n\n> **Apply these?**\n> Reply **yes** to apply all, or list the numbers you want to skip.\n\n## Step 4\n\n> **Setup complete!** Here is what changed.\n";

/// The Profile chapter answers the question that names Path A, ends on the message that says setup is complete, copies
/// documents into the subfolders `/setup` reads, and shows a review card for a proposal that closes with its words. A
/// release whose `/setup` drops one of them fails the check, with a line that names it.
#[test]
fn setup_holds_what_the_profile_chapter_reads() {
    let setup = command_path("setup");
    let mut files = complete();
    files.insert(setup.clone(), SETUP_MD.as_bytes().to_vec());
    assert_eq!(unmet(&files), Vec::<String>::new());
    let path_a = format!("does not hold `{PATH_A}`");
    let complete = format!("does not hold `{SETUP_COMPLETE}`");
    let proposal = format!("has no line that holds `{}`, `{}` and `{}`", PROPOSAL_WORDS[0], PROPOSAL_WORDS[1], PROPOSAL_WORDS[2]);
    for (from, to, says) in [
        ("**Path A: Read", "**PATH A: Read", None),
        ("**Setup complete!**", "**SETUP COMPLETE!**", None),
        ("**Path A: Read", "**Path Alpha: Read", Some(path_a.as_str())),
        ("**Path A: Read", "**Pathway: Read", Some(path_a.as_str())),
        ("**Setup complete!**", "**Setup completed.**", Some(complete.as_str())),
        ("**Setup complete!**", "**All done!**", Some(complete.as_str())),
        ("**diplomas/**", "**degrees/**", Some("does not name the folder `diplomas/`")),
        ("**cv/**", "**mycv/**", Some("does not name the folder `cv/`")),
        ("**cv/**", "**cv**", Some("does not name the folder `cv/`")),
        ("to apply all, or list", "to apply all, or\n> list", Some(proposal.as_str())),
        ("Reply **yes**", "Reply **yesterday**", Some(proposal.as_str())),
    ] {
        files.insert(setup.clone(), SETUP_MD.replacen(from, to, 1).into_bytes());
        let lines = unmet(&files);
        match says {
            None => assert_eq!(lines, Vec::<String>::new(), "{to}"),
            Some(says) => {
                assert_eq!(lines.len(), 1, "{to}: {lines:#?}");
                assert!(lines[0].starts_with(&format!("{setup} {says}")), "{to}: {}", lines[0]);
            }
        }
    }
}

#[test]
fn the_template_marker_is_checked_both_ways() {
    let mut files = complete();
    files.insert(command_path("add-template"), b"Writes no block.".to_vec());
    let guide = format!("{ASSISTANT}/{CV_TEMPLATES}");
    files.insert(guide.clone(), format!("<!-- {ACTIVE_TEMPLATE} (managed) -->").into_bytes());
    let unmet = unmet(&files);
    assert_eq!(unmet.len(), 2, "{unmet:#?}");
    assert!(unmet.iter().any(|line| line.starts_with(&format!(".claude/commands/add-template.md does not hold `{ACTIVE_TEMPLATE}`"))), "{unmet:#?}");
    assert!(unmet.iter().any(|line| line.starts_with(&format!("{guide} holds `{ACTIVE_TEMPLATE}`"))), "{unmet:#?}");
}
