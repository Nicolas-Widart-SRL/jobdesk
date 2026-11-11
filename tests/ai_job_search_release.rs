//! Whether an `ai-job-search` release has what Jobdesk relies on, before it joins `assets/update/ai-job-search.json`.
//!
//! The ignored test reads the release zip at `JOBDESK_CANDIDATE_ZIP`, the version `JOBDESK_CANDIDATE_VERSION`, the way
//! first-run setup does, and names every file it lacks. It starts no `claude` and costs nothing, and the workflow
//! `.github/workflows/ai-job-search.yml` runs it on every release the list holds, whenever a pull request or a push to
//! `main` changes the list or the code that says what Jobdesk relies on, and for the commit each Jobdesk release signs:
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
use yaml_rust2::yaml::Hash;
use yaml_rust2::{Yaml, YamlLoader};

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
    /// The slash command of this name, without its `/`: the file Claude Code runs for it, a skill of that name, from its
    /// front matter or else its folder, or else a command file, and one the user can invoke.
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

/// The front matter of a skill or command file: the block between a first line `---` and the next line `---`, after a
/// byte order mark, parsed as YAML.
#[derive(Debug, PartialEq)]
enum FrontMatter {
    /// No block, or a block that does not parse and has no line that starts with `name:` or `user-invocable:`, so it
    /// gives none of the fields the check reads.
    Absent,
    /// A block that is one YAML mapping.
    Fields(Hash),
    /// A block that does not parse as one YAML mapping, or has no closing line, with a line that starts with `name:` or
    /// `user-invocable:`. Claude Code may still read that field from it, so the file is in doubt. It holds the raw text
    /// of each `name:` line, trimmed and out of one pair of matching quotes.
    Doubtful(Vec<String>),
}

fn front_matter(bytes: &[u8]) -> FrontMatter {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = without_bom(&text).lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return FrontMatter::Absent;
    }
    let mut block = Vec::new();
    let mut closed = false;
    for line in lines {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        block.push(line);
    }
    if closed
        && let Ok(mut documents) = YamlLoader::load_from_str(&block.join("\n"))
        && let (Some(Yaml::Hash(fields)), true) = (documents.pop(), documents.is_empty())
    {
        return FrontMatter::Fields(fields);
    }
    let lines = block.iter().map(|line| line.trim());
    if !lines.clone().any(|line| line.starts_with("name:") || line.starts_with("user-invocable:")) {
        return FrontMatter::Absent;
    }
    FrontMatter::Doubtful(lines.filter_map(|line| line.strip_prefix("name:")).map(raw_name).filter(|name| !name.is_empty()).collect())
}

/// The raw value of a `name:` line that did not parse: trimmed, and out of one pair of matching quotes.
fn raw_name(value: &str) -> String {
    let value = value.trim();
    let unquoted = ['"', '\''].iter().find_map(|quote| value.strip_prefix(*quote).and_then(|rest| rest.strip_suffix(*quote)));
    unquoted.unwrap_or(value).trim().to_owned()
}

/// The field `key` of a front matter.
fn field<'a>(fields: &'a Hash, key: &str) -> Option<&'a Yaml> {
    fields.get(&Yaml::String(key.to_owned()))
}

/// What a skill is named by, as Claude Code names it.
#[derive(Debug, PartialEq, Eq)]
enum SkillName {
    /// The `name:` of its front matter, trimmed.
    Given(String),
    /// Its folder: the file has no front matter, or the front matter gives no `name:`.
    Folder,
    /// A `name:` that is not plain, non-empty text, so the name Claude Code gives the skill is not known.
    Unknown,
    /// Front matter that is in doubt, with the raw names its `name:` lines give: the skill could be named by its folder
    /// or by any of them, and could be one the user cannot invoke.
    Doubtful(Vec<String>),
}

fn skill_name(bytes: &[u8]) -> SkillName {
    match front_matter(bytes) {
        FrontMatter::Absent => SkillName::Folder,
        FrontMatter::Doubtful(names) => SkillName::Doubtful(names),
        FrontMatter::Fields(fields) => match field(&fields, "name") {
            None => SkillName::Folder,
            Some(Yaml::String(name)) if !name.trim().is_empty() => SkillName::Given(name.trim().to_owned()),
            Some(_) => SkillName::Unknown,
        },
    }
}

/// Whether the user can start a skill or a command file as a slash command, failing closed: not when its front matter
/// sets `user-invocable` to boolean false, to text that is, in any case and with spaces around it, `false`, `no` or
/// `off`, or to a number, or text that reads as one, equal to zero. Any other value, or none, counts as invocable.
fn user_invocable(fields: &Hash) -> bool {
    let refused = |text: &str| {
        let text = text.trim();
        ["false", "no", "off"].iter().any(|word| text.eq_ignore_ascii_case(word)) || zero(&Yaml::from_str(text))
    };
    match field(fields, "user-invocable") {
        Some(Yaml::Boolean(value)) => *value,
        Some(Yaml::String(text)) => !refused(text),
        Some(value) => !zero(value),
        None => true,
    }
}

/// Whether `value` is a number equal to zero.
fn zero(value: &Yaml) -> bool {
    match value {
        Yaml::Integer(number) => *number == 0,
        Yaml::Real(_) => value.as_f64() == Some(0.0),
        _ => false,
    }
}

/// Every skill `SKILL.md` right under `CLAUDE_SKILLS`, with its folder and name, in path order.
fn skills(files: &Files) -> impl Iterator<Item = (&str, &str, SkillName)> {
    files.iter().filter_map(|(path, bytes)| skill_folder(path).map(|folder| (path.as_str(), folder, skill_name(bytes))))
}

/// Each skill whose `name:` is not plain, non-empty text, one line each: it could be any command, so the check fails.
fn unknown_names(files: &Files) -> Vec<String> {
    skills(files)
        .filter(|(_, _, named)| *named == SkillName::Unknown)
        .map(|(path, _, _)| {
            format!("{path} gives a name: that is not plain, non-empty text, so the slash command Claude Code makes of it is not known, and it could be any command the launcher starts")
        })
        .collect()
}

/// The file Claude Code runs for the slash command `name`, given without its `/`, or why there is none it would run.
/// A skill of that name wins over the command file, as it does at run time: a skill named by its front matter's
/// `name:`, or by its folder when it gives none, so a skill whose `name:` names another command is not this one,
/// whatever its folder. The file that wins must be one the user can invoke, and when it is not, the command is missing:
/// the other file does not count. Failing closed, the command is missing too when more than one skill is named `name`,
/// or when a skill's front matter is in doubt and its folder or a raw `name:` line is `name`, or when the command
/// file's front matter is in doubt, since which file runs, or whether the user can invoke it, is then not known.
fn command_file<'a>(files: &'a Files, name: &str) -> Result<(&'a str, &'a [u8]), String> {
    let command = command_path(name);
    let doubtful: Vec<&str> = skills(files)
        .filter(|(_, folder, named)| match named {
            SkillName::Doubtful(names) => *folder == name || names.iter().any(|given| given == name),
            _ => false,
        })
        .map(|(path, _, _)| path)
        .collect();
    if !doubtful.is_empty() {
        return Err(format!(
            "/{name} is missing: the front matter of {} does not parse as one YAML mapping, or has no closing ---, yet gives name: or user-invocable:, so whether Claude Code runs it for /{name}, and whether the user can invoke it, is not known",
            doubtful.join(" and ")
        ));
    }
    let named: Vec<&str> = skills(files)
        .filter(|(_, folder, named)| match named {
            SkillName::Given(given) => given == name,
            SkillName::Folder => *folder == name,
            SkillName::Unknown | SkillName::Doubtful(_) => false,
        })
        .map(|(path, _, _)| path)
        .collect();
    let (path, skill) = match named.as_slice() {
        [] if files.contains_key(&command) => (command.as_str(), false),
        [] => {
            return Err(format!(
                "/{name} is missing: no {command}, and no skill of that name, which is either a {CLAUDE_SKILLS}/<folder>/{MANIFEST} named {name} or a {CLAUDE_SKILLS}/{name}/{MANIFEST} with no name"
            ));
        }
        [path] => (*path, true),
        several => {
            return Err(format!("/{name} is missing: {} are each a skill named {name}, so the one Claude Code would run is not known", several.join(" and ")));
        }
    };
    let (path, bytes) = files.get_key_value(path).expect("the file is one of the release's");
    let (kind, shadows) = match skill {
        true if files.contains_key(&command) => ("skill", format!(", and it is what runs in place of {command}")),
        true => ("skill", String::new()),
        false => ("command file", String::new()),
    };
    match front_matter(bytes) {
        FrontMatter::Doubtful(_) => Err(format!(
            "/{name} is missing: the front matter of the {kind} {path} does not parse as one YAML mapping, or has no closing ---, yet gives name: or user-invocable:, so whether the user can invoke it is not known{shadows}"
        )),
        FrontMatter::Fields(fields) if !user_invocable(&fields) => {
            Err(format!("/{name} is missing: the {kind} {path} sets user-invocable to false in its front matter, so the user cannot invoke it{shadows}"))
        }
        _ => Ok((path.as_str(), bytes.as_slice())),
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
    let mut lines = unknown_names(files);
    for need in needs() {
        let found = match &need.place {
            Place::File(path) => files.get_key_value(path).map(|(path, bytes)| (path.as_str(), bytes.as_slice())).ok_or_else(|| format!("{path} is missing")),
            Place::Command(name) => command_file(files, name),
        };
        let (path, bytes) = match found {
            Ok(found) => found,
            Err(missing) => {
                lines.push(format!("{missing} ({})", need.why));
                continue;
            }
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
        (&b"---\nname: scrape\nuser-invocable: false  # internal\n---\n"[..], "a trailing comment"),
        (&b"---\nname: scrape\nuser-invocable: false\t# internal\n---\n"[..], "a trailing comment after a tab"),
        (&b"---\nname: scrape\nuser-invocable: False\n---\n"[..], "False"),
        (&b"---\nname: scrape\nuser-invocable: FALSE\n---\n"[..], "FALSE"),
        (&b"---\nname: scrape\nuser-invocable: \"false\"\n---\n"[..], "double quotes"),
        (&b"---\nname: scrape\nuser-invocable: 'False' # internal\n---\n"[..], "single quotes and a comment"),
        (&b"---\nname: scrape\nuser-invocable: no\n---\n"[..], "no"),
        (&b"---\nname: scrape\nuser-invocable: Off\n---\n"[..], "Off"),
        (&b"---\nname: scrape\nuser-invocable: NO\n---\n"[..], "NO"),
        (&b"---\nname: scrape\nuser-invocable: 0\n---\n"[..], "0"),
        (&b"---\nname: scrape\nuser-invocable: 0.0\n---\n"[..], "0.0, the number 0"),
        (&b"---\nname: scrape\nuser-invocable: '0'\n---\n"[..], "0 as text"),
        (&b"---\nname: scrape\nuser-invocable: 'false '\n---\n"[..], "false with a space inside the quotes"),
        (&b"---\nname: scrape\nuser-invocable: ' no'\n---\n"[..], "no with a space inside the quotes"),
        (&b"---\nname: scrape\nuser-invocable: '0.0'\n---\n"[..], "0.0 as text"),
        (&b"---\nname: scrape\nuser-invocable: \" -0 \"\n---\n"[..], "-0 as text with spaces"),
        (&b"---\nname: scrape\nuser-invocable: '0x0'\n---\n"[..], "0x0 as text"),
        (&b"---\nname: ' scrape '\nuser-invocable: false\n---\n"[..], "a name with spaces inside the quotes"),
        (&b"---\r\nname: scrape\r\nuser-invocable: false\r\n---\r\n"[..], "CRLF line ends"),
        (&b"---\nname: \"scrape\"\nuser-invocable:\n  false\n---\n"[..], "a value on the next line"),
    ] {
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: {lines:#?}");
        assert!(lines[0].starts_with("/scrape is missing"), "{why}: {}", lines[0]);
        assert!(
            lines[0].contains("the skill .claude/skills/scrape/SKILL.md sets user-invocable to false"),
            "{why}: the line names the skill the user cannot invoke: {}",
            lines[0]
        );
    }
    files.remove(".claude/skills/scrape/SKILL.md");
    let lines = unmet(&files);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(
        lines[0].starts_with("/scrape is missing: no .claude/commands/scrape.md, and no skill of that name"),
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
        (&b"---\nname: scrape\nuser-invocable: yes\n---\n"[..], "yes"),
        (&b"---\nname: scrape\nuser-invocable: 1\n---\n"[..], "1"),
        (&b"---\nname: scrape\nuser-invocable: [false]\n---\n"[..], "a list"),
        (&b"---\nname: scrape\nuser-invocable: 'no way'\n---\n"[..], "text that only starts with no"),
        (&b"---\nname: scrape\nuser-invocable: '0.1'\n---\n"[..], "a number other than zero as text"),
        (&b"---\ndescription: Use when: the user asks\n---\n"[..], "front matter that does not parse, with neither key"),
    ] {
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{why}");
    }
}

/// Front matter that does not parse, or has no closing line, may still be read by Claude Code, so when it has a line
/// that gives `name:` or `user-invocable:`, the skill is in doubt: the command of its folder and the command its raw
/// `name:` line names are missing, even beside a command file.
#[test]
fn front_matter_in_doubt_fails_closed() {
    let doubtful = [
        (&b"---\nname: scrape\ndescription: [unclosed\n---\n"[..], "an unclosed list"),
        (&b"---\nname: scrape\nname: scrape\n---\n"[..], "a key given twice"),
        (&b"---\nname: scrape\n  bad: indent\n---\n"[..], "a bad indent"),
        (&b"---\nname: scrape\n"[..], "a block with no closing line"),
        (&b"---\nname: 'scrape'\ndescription: Use when: the user asks\n---\n"[..], "a colon in a plain value, and a quoted name"),
        (&b"---\n  name: \" scrape \"  \ndescription: Use when: the user asks\n---\n"[..], "an indented name with spaces inside the quotes"),
    ];
    for (bytes, why) in doubtful {
        assert!(
            matches!(front_matter(bytes), FrontMatter::Doubtful(names) if !names.is_empty() && names.iter().all(|name| name == "scrape")),
            "{why}: {:?}",
            front_matter(bytes)
        );
        for folder in ["other", "scrape"] {
            let mut files = complete();
            let skill = format!(".claude/skills/{folder}/SKILL.md");
            files.insert(skill.clone(), bytes.to_vec());
            let lines = unmet(&files);
            assert_eq!(lines.len(), 1, "{why}, in {folder}: {lines:#?}");
            assert!(
                lines[0].starts_with(&format!("/scrape is missing: the front matter of {skill} does not parse as one YAML mapping, or has no closing ---")),
                "{why}, in {folder}: {}",
                lines[0]
            );
        }
    }
    for (bytes, why) in [
        (&b"---\ndescription: Use when: the user asks\nuser-invocable: false\n---\n"[..], "a colon in a plain value"),
        (&b"---\nuser-invocable: false\ndescription: [unclosed\n---\n"[..], "an unclosed list"),
        (&b"---\nuser-invocable: false\n"[..], "a block with no closing line"),
    ] {
        assert_eq!(front_matter(bytes), FrontMatter::Doubtful(Vec::new()), "{why}");
        let mut files = complete();
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: {lines:#?}");
        assert!(lines[0].starts_with("/scrape is missing: the front matter of .claude/skills/scrape/SKILL.md does not parse"), "{why}: {}", lines[0]);
        files.remove(".claude/skills/scrape/SKILL.md");
        files.insert(".claude/skills/other/SKILL.md".to_owned(), bytes.to_vec());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{why}: a doubtful skill in another folder with no name: line is not /scrape");
    }
}

/// Front matter that does not parse, or is not a mapping, and has no line that gives `name:` or `user-invocable:`
/// gives none of the fields the check reads: the skill is named by its folder, and the user can invoke it.
#[test]
fn front_matter_that_does_not_parse_with_neither_key_names_the_folder() {
    let mut files = complete();
    files.remove(&command_path("scrape"));
    for (bytes, why) in [
        (&b"---\ndescription: Use when: the user asks\n---\n"[..], "a colon in a plain value"),
        (&b"---\ndescription: [unclosed\n---\n"[..], "an unclosed list"),
        (&b"---\n- name: scrape\n---\n"[..], "a list, not a mapping"),
        (&b"---\ndescription: Finds jobs.\n"[..], "a block with no closing line"),
    ] {
        assert_eq!(front_matter(bytes), FrontMatter::Absent, "{why}: no front matter");
        files.insert(".claude/skills/other/SKILL.md".to_owned(), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: the skill in other is not /scrape: {lines:#?}");
        assert!(lines[0].starts_with("/scrape is missing: no .claude/commands/scrape.md"), "{why}: {}", lines[0]);
        files.remove(".claude/skills/other/SKILL.md");
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{why}: the skill in scrape is /scrape by its folder");
        files.remove(".claude/skills/scrape/SKILL.md");
    }
}

/// A `name:` that is not plain, non-empty text leaves the command Claude Code makes of the skill unknown, and it could
/// be any command, so the check fails, wherever the skill is and whatever command files there are.
#[test]
fn a_name_that_is_not_text_fails_the_check() {
    for bytes in [
        &b"---\nname: 12\n---\n"[..],
        &b"---\nname: [x]\n---\n"[..],
        &b"---\nname: [add-template]\n---\n"[..],
        &b"---\nname:\n---\n"[..],
        &b"---\nname: ''\n---\n"[..],
        &b"---\nname: '  '\n---\n"[..],
    ] {
        for folder in ["other", "scrape"] {
            let mut files = complete();
            let skill = format!(".claude/skills/{folder}/SKILL.md");
            files.insert(skill.clone(), bytes.to_vec());
            let lines = unmet(&files);
            assert_eq!(lines.len(), 1, "{lines:#?}");
            assert!(lines[0].starts_with(&format!("{skill} gives a name: that is not plain, non-empty text")), "{}", lines[0]);
        }
    }
}

/// A command file follows the same front matter rules as a skill.
#[test]
fn a_command_file_the_user_cannot_invoke_is_not_the_command() {
    let mut files = complete();
    for (bytes, why) in [
        (&b"---\nuser-invocable: false\n---\nScrapes.\n"[..], "false"),
        (&b"---\ndescription: Scrapes.\nuser-invocable: off # internal\n---\n"[..], "off and a comment"),
        (&b"\xef\xbb\xbf---\nuser-invocable: 0\n---\n"[..], "0 after a BOM"),
    ] {
        files.insert(command_path("scrape"), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: {lines:#?}");
        assert!(lines[0].starts_with("/scrape is missing: the command file .claude/commands/scrape.md sets user-invocable to false"), "{why}: {}", lines[0]);
    }
    for (bytes, why) in [
        (&b"---\ndescription: [unclosed\nuser-invocable: false\n---\n"[..], "front matter that does not parse"),
        (&b"---\ndescription: Use when: the user asks\nuser-invocable: true\n---\n"[..], "a colon in a plain value"),
        (&b"---\nuser-invocable: false\n"[..], "a block with no closing line"),
    ] {
        files.insert(command_path("scrape"), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: {lines:#?}");
        assert!(
            lines[0].starts_with("/scrape is missing: the front matter of the command file .claude/commands/scrape.md does not parse as one YAML mapping"),
            "{why}: {}",
            lines[0]
        );
    }
    for (bytes, why) in [
        (&b"---\nuser-invocable: true\n---\n"[..], "true"),
        (&b"---\ndescription: Use when: the user asks\n---\n"[..], "front matter that does not parse, with neither key"),
        (&b"Scrapes.\n\nuser-invocable: false\n"[..], "no front matter"),
    ] {
        files.insert(command_path("scrape"), bytes.to_vec());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{why}");
    }
}

/// Claude Code runs a skill over a command file of the same name, so the skill is what is checked, and a skill the user
/// cannot invoke leaves the command missing, the command file notwithstanding.
#[test]
fn a_skill_wins_over_a_command_file_of_its_name() {
    let mut files = complete();
    let command = command_path("add-template");
    assert!(String::from_utf8_lossy(&files[&command]).contains(ACTIVE_TEMPLATE), "the command file holds the marker");
    files.insert(".claude/skills/templates/SKILL.md".to_owned(), b"---\nname: add-template\n---\nWrites no block.\n".to_vec());
    let lines = unmet(&files);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].starts_with(&format!(".claude/skills/templates/SKILL.md does not hold `{ACTIVE_TEMPLATE}`")), "{}", lines[0]);
    files.insert(".claude/skills/templates/SKILL.md".to_owned(), format!("---\nname: add-template\n---\n{ACTIVE_TEMPLATE}\n").into_bytes());
    files.insert(command.clone(), b"Writes no block.".to_vec());
    assert_eq!(unmet(&files), Vec::<String>::new(), "the skill holds the marker");
    files.insert(
        ".claude/skills/templates/SKILL.md".to_owned(),
        format!("---\nname: add-template\nuser-invocable: false\n---\n{ACTIVE_TEMPLATE}\n").into_bytes(),
    );
    files.insert(command.clone(), ACTIVE_TEMPLATE.as_bytes().to_vec());
    let lines = unmet(&files);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(
        lines[0].starts_with(&format!(
            "/add-template is missing: the skill .claude/skills/templates/SKILL.md sets user-invocable to false in its front matter, so the user cannot invoke it, and it is what runs in place of {command}"
        )),
        "{}",
        lines[0]
    );
}

/// A `name:` with spaces around it names the command it holds, so it wins over that command's file too.
#[test]
fn a_padded_name_is_the_command_it_holds() {
    let mut files = complete();
    let command = command_path("add-template");
    files.insert(".claude/skills/templates/SKILL.md".to_owned(), b"---\nname: \" add-template \"\nuser-invocable: false\n---\n".to_vec());
    let lines = unmet(&files);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(
        lines[0].starts_with(&format!(
            "/add-template is missing: the skill .claude/skills/templates/SKILL.md sets user-invocable to false in its front matter, so the user cannot invoke it, and it is what runs in place of {command}"
        )),
        "{}",
        lines[0]
    );
    files.insert(".claude/skills/templates/SKILL.md".to_owned(), format!("---\nname: \" add-template \"\n---\n{ACTIVE_TEMPLATE}\n").into_bytes());
    files.insert(command, b"Writes no block.".to_vec());
    assert_eq!(unmet(&files), Vec::<String>::new(), "the padded skill is what is read");
}

/// Two skills of one name leave which one runs unknown, so the command is missing.
#[test]
fn two_skills_of_one_name_fail_closed() {
    let mut files = complete();
    files.insert(".claude/skills/scrape/SKILL.md".to_owned(), b"# Scrape\n".to_vec());
    files.insert(".claude/skills/job-scraper/SKILL.md".to_owned(), b"---\nname: scrape\n---\n".to_vec());
    let lines = unmet(&files);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(
        lines[0].starts_with("/scrape is missing: .claude/skills/job-scraper/SKILL.md and .claude/skills/scrape/SKILL.md are each a skill named scrape"),
        "{}",
        lines[0]
    );
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
