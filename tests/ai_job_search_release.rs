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
    /// Text the file must hold (`true`) or must not hold (`false`).
    text: Option<(&'static str, bool)>,
    why: &'static str,
}

impl Need {
    fn file(path: impl Into<String>, why: &'static str) -> Self {
        Self { place: Place::File(path.into()), text: None, why }
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
        let text = (spec.name == TEMPLATE_WRITER).then_some((ACTIVE_TEMPLATE, true));
        let why =
            if text.is_some() { "the launcher starts it, and it writes the block that says a custom template is active" } else { "the launcher starts it" };
        needs.push(Need { place: Place::Command(name), text, why });
    }
    for file in ProfileFile::ALL {
        needs.push(Need::file(file.relative_path(), "Profile reads and saves it"));
    }
    for guide in [CV_TEMPLATES, LETTER_TEMPLATES] {
        needs.push(Need {
            place: Place::File(format!("{ASSISTANT}/{guide}")),
            text: Some((ACTIVE_TEMPLATE, false)),
            why: "Setup reads it for a custom template, so a stock one must not say a custom template is active",
        });
    }
    for (folder, _) in BUILT_IN {
        needs.push(Need::file(format!("{SKILLS}/{folder}/{MANIFEST}"), "Portals lists it as built in"));
    }
    needs
}

/// The value of `key` in a skill's front matter, unquoted, or `None` when the file has no front matter or the front
/// matter does not give it.
fn front_matter(bytes: &[u8], key: &str) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = without_bom(&text).lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    lines
        .take_while(|line| line.trim_end() != "---")
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
        .map(|value| value.trim().trim_matches(['"', '\'']).to_owned())
}

/// Whether the user can start a skill as a slash command: not when its front matter says `user-invocable: false`.
fn user_invocable(bytes: &[u8]) -> bool {
    front_matter(bytes, "user-invocable").as_deref() != Some("false")
}

/// The file that makes `name` a slash command: its command file, or else a skill of that name the user can start. A
/// skill is named by the `name:` of its front matter, or by its folder when it has none, as Claude Code names it: a
/// skill whose `name:` names another command is not this one, whatever its folder. A skill with `user-invocable: false`
/// is no slash command, so the launcher cannot start it.
fn command_file<'a>(files: &'a Files, name: &str) -> Option<(&'a str, &'a [u8])> {
    if let Some((path, bytes)) = files.get_key_value(&command_path(name)) {
        return Some((path, bytes));
    }
    files
        .iter()
        .find(|(path, bytes)| {
            skill_folder(path).is_some_and(|folder| front_matter(bytes, "name").as_deref().unwrap_or(folder) == name) && user_invocable(bytes)
        })
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
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
                Place::Command(name) => format!(
                    "/{name} is missing: no {}, no {CLAUDE_SKILLS}/<folder>/{MANIFEST} named {name}, and no {CLAUDE_SKILLS}/{name}/{MANIFEST} with no name, that is not user-invocable: false",
                    command_path(name)
                ),
            };
            lines.push(format!("{missing} ({})", need.why));
            continue;
        };
        if let Some((text, wanted)) = need.text {
            let holds = String::from_utf8_lossy(bytes).contains(text);
            if holds != wanted {
                let says = if wanted { "does not hold" } else { "holds" };
                lines.push(format!("{path} {says} `{text}` ({})", need.why));
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
        let text = match need.text {
            Some((text, true)) => text,
            _ => "fictional",
        };
        files.entry(path).or_default().extend_from_slice(text.as_bytes());
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
    ] {
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: {lines:#?}");
        assert!(lines[0].starts_with("/scrape is missing"), "{why}: {}", lines[0]);
    }
    for (bytes, why) in [
        (&b"---\nname: scrape\nuser-invocable: true\n---\n"[..], "invocable"),
        (&b"---\nname: scrape\n---\n"[..], "no user-invocable"),
        (&b"---\nname: scrape\n---\nuser-invocable: false\n"[..], "user-invocable outside the front matter"),
        (&b"# Scrape\n\nuser-invocable: false\n"[..], "no front matter"),
    ] {
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{why}");
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
