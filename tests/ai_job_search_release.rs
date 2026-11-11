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
    // The marker is a command file, and it is needed as a file whatever runs for its command, which is needed as well,
    // and held to the same rules as every other command.
    let mut needs = vec![Need::file(MARKER, "Jobdesk opens a folder as a job search folder only when it holds this file")];
    for spec in CATALOG.iter().filter(|spec| !OPTIONAL.iter().any(|(name, _)| *name == spec.name)) {
        let name = spec.name.strip_prefix('/').expect("a command starts with /");
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
    /// No block.
    Absent,
    /// A block that parses as one YAML mapping, or as nothing at all, such as an empty block or one of comments only.
    Fields(Hash),
    /// A block that does not parse as one YAML mapping, or has no closing line, whatever keys it seems to hold: the file
    /// is in doubt.
    Doubtful(Doubt),
}

/// Front matter in doubt: why, and the names its `name` lines could give, as `raw_names` reads them.
#[derive(Debug, PartialEq, Eq)]
struct Doubt {
    /// What is wrong with the block, worded to follow "the front matter of <file>": "has no closing ---", "does not
    /// parse as YAML: <reason> at line <n> of the file", or "is not one YAML mapping".
    why: String,
    /// The raw value of each `name` line that gives one in printable ASCII, as plain text or in quotes.
    names: Vec<String>,
    /// Whether a `name` line gives no value on its line, so its value could be on the next, one with a character
    /// outside printable ASCII, or one out of quotes that starts with a YAML indicator, so it is no plain text and could
    /// be a list, a mapping, an alias or a block scalar: the name is then not known.
    unknown_name: bool,
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
    let why = if !closed {
        "has no closing ---".to_owned()
    } else {
        match YamlLoader::load_from_str(&block.join("\n")) {
            Ok(mut documents) => match (documents.pop(), documents.is_empty()) {
                (None, _) => return FrontMatter::Fields(Hash::new()),
                (Some(Yaml::Hash(fields)), true) => return FrontMatter::Fields(fields),
                _ => "is not one YAML mapping".to_owned(),
            },
            // The block starts on the file's second line.
            Err(error) => format!("does not parse as YAML: {} at line {} of the file", error.info(), error.marker().line() + 1),
        }
    };
    let (names, unknown_name) = raw_names(&block);
    FrontMatter::Doubtful(Doubt { why, names, unknown_name })
}

/// The raw names a block that does not parse gives, and whether a `name` line gives one that is not known. A `name`
/// line is one whose key, before its first `:`, reads `name` once spaces and tabs around it and one pair of matching
/// quotes around the key are gone. Its value loses a trailing comment, a space or tab then `#` and all after it, then
/// spaces and tabs around it, one pair of matching quotes, and spaces and tabs around what they held. A value that is
/// then empty, or holds a character outside printable ASCII, is not known, nor is one that was in no pair of quotes and
/// starts as no plain YAML text does, nor is one in double quotes that holds a backslash, because YAML reads an escape
/// there and this recovery does not decode it. Inside single quotes a backslash is itself, so it stays in the name.
fn raw_names(block: &[&str]) -> (Vec<String>, bool) {
    let mut names = Vec::new();
    let mut unknown = false;
    for line in block {
        let Some((key, value)) = blank_trimmed(line).split_once(':') else { continue };
        if unquoted(blank_trimmed(key)) != "name" {
            continue;
        }
        let value = blank_trimmed(uncommented(value));
        let quoted = unquoted(value).len() != value.len();
        let name = blank_trimmed(unquoted(value));
        let escaped = value.starts_with('"') && quoted && name.contains('\\');
        if name.is_empty() || !printable_ascii(name) || escaped || (!quoted && starts_as_no_plain_text(name)) {
            unknown = true;
        } else {
            names.push(name.to_owned());
        }
    }
    (names, unknown)
}

/// Whether a raw value out of quotes starts the way no plain YAML text does: with a YAML indicator, such as `[` or `{`
/// for a list or a mapping, `&`, `*` or `!` for an anchor, an alias or a tag, `|` or `>` for a block scalar, or a quote
/// with no matching one at the end, or with `-`, `?` or `:` then a space, a tab or nothing.
fn starts_as_no_plain_text(value: &str) -> bool {
    let mut characters = value.chars();
    match characters.next() {
        Some('[' | ']' | '{' | '}' | ',' | '#' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`') => true,
        Some('-' | '?' | ':') => matches!(characters.next(), None | Some(' ' | '\t')),
        _ => false,
    }
}

/// `text` without the spaces and tabs around it, and nothing else: a wider trim would hide a character the name holds.
fn blank_trimmed(text: &str) -> &str {
    text.trim_matches([' ', '\t'])
}

/// A raw value without its trailing YAML comment: a space or tab, then `#` and all after it.
fn uncommented(value: &str) -> &str {
    value.char_indices().find(|&(at, character)| character == '#' && value[..at].ends_with([' ', '\t'])).map_or(value, |(at, _)| &value[..at])
}

/// Whether every character of `text` is printable ASCII, the space included.
fn printable_ascii(text: &str) -> bool {
    text.bytes().all(|byte| byte == b' ' || byte.is_ascii_graphic())
}

/// `text` out of one pair of matching quotes, or `text` itself when it is not quoted.
fn unquoted(text: &str) -> &str {
    ['"', '\''].iter().find_map(|quote| text.strip_prefix(*quote).and_then(|rest| rest.strip_suffix(*quote))).unwrap_or(text)
}

/// The field `key` of a front matter.
fn field<'a>(fields: &'a Hash, key: &str) -> Option<&'a Yaml> {
    fields.get(&Yaml::String(key.to_owned()))
}

/// What a skill is named by, as Claude Code names it.
#[derive(Debug, PartialEq, Eq)]
enum SkillName {
    /// The `name:` of its front matter: plain, non-empty text in printable ASCII with no spaces around it.
    Given(String),
    /// Its folder: the file has no front matter, or the front matter gives no `name:`.
    Folder,
    /// A `name:` that is not plain, non-empty text in printable ASCII, so the name Claude Code gives the skill is not
    /// known.
    Unknown,
    /// A `name:` of plain text in printable ASCII with spaces around it, held here trimmed: the skill could be the
    /// command of that name, or could not, so it is in doubt.
    Padded(String),
    /// Front matter that is in doubt: the skill could be named by its folder or by any of the raw names its `name`
    /// lines give, and could be one the user cannot invoke.
    Doubtful(Doubt),
}

fn skill_name(bytes: &[u8]) -> SkillName {
    match front_matter(bytes) {
        FrontMatter::Absent => SkillName::Folder,
        FrontMatter::Doubtful(doubt) => SkillName::Doubtful(doubt),
        FrontMatter::Fields(fields) => match field(&fields, "name") {
            None => SkillName::Folder,
            Some(Yaml::String(name)) if name.trim().is_empty() || !printable_ascii(name) => SkillName::Unknown,
            Some(Yaml::String(name)) if name.trim() != name => SkillName::Padded(name.trim().to_owned()),
            Some(Yaml::String(name)) => SkillName::Given(name.clone()),
            Some(_) => SkillName::Unknown,
        },
    }
}

/// Whether the user can start a skill or a command file as a slash command, failing closed: only when its front matter
/// gives no `user-invocable`, or sets it to boolean true, or to the exact text `true`, in lower case with no spaces
/// around it. Any other value fails, with the value as the error, shown as `shown` shows it.
fn user_invocable(fields: &Hash) -> Result<(), String> {
    match field(fields, "user-invocable") {
        None | Some(Yaml::Boolean(true)) => Ok(()),
        Some(Yaml::String(text)) if text == "true" => Ok(()),
        Some(value) => Err(shown(value)),
    }
}

/// A front matter value as a line of the check shows it.
fn shown(value: &Yaml) -> String {
    match value {
        Yaml::Boolean(value) => value.to_string(),
        Yaml::Integer(number) => number.to_string(),
        Yaml::Real(number) => number.clone(),
        Yaml::String(text) => format!("{text:?}"),
        Yaml::Null => "an empty value".to_owned(),
        Yaml::Array(_) => "a list".to_owned(),
        Yaml::Hash(_) => "a mapping".to_owned(),
        other => format!("{other:?}"),
    }
}

/// Every skill `SKILL.md` right under `CLAUDE_SKILLS`, with its folder and name, in path order.
fn skills(files: &Files) -> impl Iterator<Item = (&str, &str, SkillName)> {
    files.iter().filter_map(|(path, bytes)| skill_folder(path).map(|folder| (path.as_str(), folder, skill_name(bytes))))
}

/// Every command file right in the commands folder whose front matter is in doubt, with its path, its command and the
/// doubt, in path order.
fn doubtful_command_files(files: &Files) -> impl Iterator<Item = (&str, &str, Doubt)> {
    files.iter().filter_map(|(path, bytes)| {
        let command = path.strip_prefix(commands_folder())?.strip_prefix('/')?.strip_suffix(".md").filter(|name| !name.is_empty() && !name.contains('/'))?;
        match front_matter(bytes) {
            FrontMatter::Doubtful(doubt) => Some((path.as_str(), command, doubt)),
            _ => None,
        }
    })
}

/// What a `name` line in front matter in doubt gives when its name is not known, worded to follow "gives a name:".
const NOT_KNOWN_IN_DOUBT: &str = "with no value on its line, one outside printable ASCII, one with an escape in double quotes, or one out of quotes that starts with a YAML indicator, such as [, { or >, and so is no plain text";

/// One line for each skill whose `name:` is not plain, non-empty text in printable ASCII, and for each skill or command
/// file in doubt with a `name` line whose name is not known: it could be any command, so the check fails.
fn unknown_names(files: &Files) -> Vec<String> {
    let then = "so the slash command Claude Code makes of it is not known, and it could be any command the launcher starts";
    let in_doubt = |path: &str, doubt: &Doubt| format!("{path} gives a name: {NOT_KNOWN_IN_DOUBT}, in front matter that {}, {then}", doubt.why);
    let mut lines: Vec<String> =
        doubtful_command_files(files).filter(|(_, _, doubt)| doubt.unknown_name).map(|(path, _, doubt)| in_doubt(path, &doubt)).collect();
    for (path, _, named) in skills(files) {
        match named {
            SkillName::Unknown => lines.push(format!("{path} gives a name: that is not plain, non-empty text in printable ASCII, {then}")),
            SkillName::Doubtful(doubt) if doubt.unknown_name => lines.push(in_doubt(path, &doubt)),
            _ => {}
        }
    }
    lines
}

/// The file Claude Code runs for the slash command `name`, given without its `/`, or why there is none it would run.
/// A skill of that name wins over the command file, as it does at run time: a skill named by its front matter's
/// `name:`, or by its folder when it gives none, so a skill whose `name:` names another command is never counted as this
/// one. The file that wins must be one the user can invoke, and when it is not, the command is missing: the other file
/// does not count. Failing closed, the command is missing too, since which file runs, or whether the user can invoke
/// it, is then not known: when a skill's or a command file's front matter is in doubt and its folder, its command or a
/// raw `name` line is `name`, and the line says why each is in doubt; when a skill's `name:` is `name` with spaces
/// around it; when more than one skill is named `name`; or when a skill in the folder `name` gives another `name:`,
/// since some Claude Code versions run a skill for its folder's name too, and the line names the skill, its folder and
/// its name, and says it is a second candidate when another skill is named `name`.
fn command_file<'a>(files: &'a Files, name: &str) -> Result<(&'a str, &'a [u8]), String> {
    let command = command_path(name);
    let could_be = |own: &str, doubt: &Doubt| own == name || doubt.names.iter().any(|given| given == name);
    let mut doubtful: Vec<String> = doubtful_command_files(files)
        .filter(|(_, own, doubt)| could_be(own, doubt))
        .map(|(path, _, doubt)| format!("the front matter of {path} {}", doubt.why))
        .collect();
    doubtful.extend(skills(files).filter_map(|(path, folder, named)| match named {
        SkillName::Doubtful(doubt) if could_be(folder, &doubt) => Some(format!("the front matter of {path} {}", doubt.why)),
        _ => None,
    }));
    if !doubtful.is_empty() {
        return Err(format!(
            "/{name} is missing: {}, so whether Claude Code runs it for /{name}, and whether the user can invoke it, is not known",
            doubtful.join(", and ")
        ));
    }
    let padded: Vec<&str> = skills(files).filter(|(_, _, named)| *named == SkillName::Padded(name.to_owned())).map(|(path, _, _)| path).collect();
    if !padded.is_empty() {
        return Err(format!(
            "/{name} is missing: {} gives name: {name} with spaces around it, so whether Claude Code runs it for /{name} is not known",
            padded.join(" and ")
        ));
    }
    let named: Vec<&str> = skills(files)
        .filter(|(_, folder, named)| match named {
            SkillName::Given(given) => given == name,
            SkillName::Folder => *folder == name,
            SkillName::Unknown | SkillName::Padded(_) | SkillName::Doubtful(_) => false,
        })
        .map(|(path, _, _)| path)
        .collect();
    // A folder holds one `SKILL.md`, so at most one skill is in the folder of this name.
    let aliased = skills(files).find_map(|(path, folder, named)| match named {
        SkillName::Given(given) if folder == name && given != name => Some((path, format!("name: {given}"))),
        SkillName::Padded(given) if folder == name && given != name => Some((path, format!("name: {given} with spaces around it"))),
        _ => None,
    });
    if let Some((alias, gives)) = aliased {
        if named.is_empty() {
            return Err(format!(
                "/{name} is missing: the skill {alias} is in the folder {name} but gives {gives}, and some Claude Code versions run a skill for its folder's name too, so whether Claude Code runs it for /{name} is not known"
            ));
        }
        let candidates: Vec<String> =
            named.iter().map(|path| format!("{path}, a skill named {name}")).chain([format!("{alias}, in the folder {name} but with {gives}")]).collect();
        return Err(format!("/{name} is missing: {}, could each run for /{name}, so the one Claude Code would run is not known", candidates.join(", and ")));
    }
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
        FrontMatter::Doubtful(doubt) => {
            Err(format!("/{name} is missing: the front matter of the {kind} {path} {}, so whether the user can invoke it is not known{shadows}", doubt.why))
        }
        FrontMatter::Fields(fields) => match user_invocable(&fields) {
            Ok(()) => Ok((path.as_str(), bytes.as_slice())),
            Err(value) => Err(format!(
                "/{name} is missing: the {kind} {path} sets user-invocable to {value} in its front matter, and only true, or no user-invocable, is one the user can invoke{shadows}"
            )),
        },
        FrontMatter::Absent => Ok((path.as_str(), bytes.as_slice())),
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
        let needed = needs.iter().filter(|need| need.place == Place::Command(name)).count();
        let optional = OPTIONAL.iter().filter(|(optional, _)| *optional == spec.name).count();
        assert_eq!(needed + optional, 1, "{} is needed or optional, once", spec.name);
        // Only the marker is needed as a command file too: every other command counts by whichever file runs for it.
        let as_file = needs.iter().filter(|need| need.place == Place::File(command_path(name))).count();
        assert_eq!(as_file, usize::from(command_path(name) == MARKER), "{} is needed as its command file only when that file is the marker", spec.name);
    }
    assert!(needs.iter().any(|need| need.place == Place::File(MARKER.to_owned())), "the marker is needed as a file");
    let marker = MARKER.strip_prefix(commands_folder()).and_then(|rest| rest.strip_prefix('/')).and_then(|rest| rest.strip_suffix(".md"));
    assert!(marker.is_some_and(|name| needs.iter().any(|need| need.place == Place::Command(name))), "the marker's command is needed as a command");
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
    assert_eq!(unmet.len(), 4, "{unmet:#?}");
    assert!(unmet[0].starts_with(".claude/commands/apply.md is missing"), "{}", unmet[0]);
    assert!(unmet.iter().any(|line| line.starts_with("/apply is missing: no .claude/commands/apply.md")), "{unmet:#?}");
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

/// Only a skill with no `user-invocable`, or one set to true, is a slash command the user can surely start: any other
/// value leaves the command missing, whatever the skill's name, and the line quotes the value.
#[test]
fn a_skill_the_user_cannot_surely_invoke_is_not_the_command() {
    let mut files = complete();
    files.remove(&command_path("scrape"));
    for (bytes, value, why) in [
        (&b"---\nname: scrape\nuser-invocable: false\n---\n"[..], "false", "named for the command"),
        (&b"---\nuser-invocable: false\ndescription: Finds jobs.\n---\n"[..], "false", "named by its folder"),
        (&b"\xef\xbb\xbf---\nuser-invocable:   false  \nname: scrape\n---\n"[..], "false", "after a BOM, with spaces around the value"),
        (&b"---\nname: scrape\nuser-invocable: false  # internal\n---\n"[..], "false", "a trailing comment"),
        (&b"---\nname: scrape\nuser-invocable: false\t# internal\n---\n"[..], "false", "a trailing comment after a tab"),
        (&b"---\nname: scrape\nuser-invocable: False\n---\n"[..], "false", "False, which YAML reads as the boolean"),
        (&b"---\nname: scrape\nuser-invocable: \"false\"\n---\n"[..], "\"false\"", "double quotes"),
        (&b"---\nname: scrape\nuser-invocable: 'False' # internal\n---\n"[..], "\"False\"", "single quotes and a comment"),
        (&b"---\nname: scrape\nuser-invocable: no\n---\n"[..], "\"no\"", "no"),
        (&b"---\nname: scrape\nuser-invocable: Off\n---\n"[..], "\"Off\"", "Off"),
        (&b"---\nname: scrape\nuser-invocable: 0\n---\n"[..], "0", "0"),
        (&b"---\nname: scrape\nuser-invocable: 0.0\n---\n"[..], "0.0", "0.0, the number 0"),
        (&b"---\nname: scrape\nuser-invocable: '0'\n---\n"[..], "\"0\"", "0 as text"),
        (&b"---\nname: scrape\nuser-invocable: yes\n---\n"[..], "\"yes\"", "yes"),
        (&b"---\nname: scrape\nuser-invocable: 1\n---\n"[..], "1", "1"),
        (&b"---\nname: scrape\nuser-invocable: on\n---\n"[..], "\"on\"", "on"),
        (&b"---\nname: scrape\nuser-invocable: falsehood\n---\n"[..], "\"falsehood\"", "another word"),
        (&b"---\nname: scrape\nuser-invocable: truthy\n---\n"[..], "\"truthy\"", "a word that only starts with true"),
        (&b"---\nname: scrape\nuser-invocable: \"True\"\n---\n"[..], "\"True\"", "True as text"),
        (&b"---\nname: scrape\nuser-invocable: 'TRUE'\n---\n"[..], "\"TRUE\"", "TRUE as text"),
        (&b"---\nname: scrape\nuser-invocable: ' true'\n---\n"[..], "\" true\"", "true as text with a space before it"),
        (&b"---\nname: scrape\nuser-invocable: \"true \"\n---\n"[..], "\"true \"", "true as text with a space after it"),
        (&b"---\nname: scrape\nuser-invocable: ' TRUE '\n---\n"[..], "\" TRUE \"", "TRUE as text with spaces around it"),
        (&b"---\nname: scrape\nuser-invocable: \"\\ttrue\"\n---\n"[..], "\"\\ttrue\"", "true as text after a tab"),
        (&b"---\nname: scrape\nuser-invocable:\n---\n"[..], "an empty value", "empty"),
        (&b"---\nname: scrape\nuser-invocable: null\n---\n"[..], "an empty value", "null"),
        (&b"---\nname: scrape\nuser-invocable: \"\"\n---\n"[..], "\"\"", "empty text"),
        (&b"---\nname: scrape\nuser-invocable: [true]\n---\n"[..], "a list", "a list that holds true"),
        (&b"---\nname: scrape\nuser-invocable: [false]\n---\n"[..], "a list", "a list that holds false"),
        (&b"---\nname: scrape\nuser-invocable: {value: true}\n---\n"[..], "a mapping", "a mapping"),
        (&b"---\r\nname: scrape\r\nuser-invocable: false\r\n---\r\n"[..], "false", "CRLF line ends"),
        (&b"---\nname: \"scrape\"\nuser-invocable:\n  false\n---\n"[..], "false", "a value on the next line"),
    ] {
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: {lines:#?}");
        assert!(lines[0].starts_with("/scrape is missing"), "{why}: {}", lines[0]);
        assert!(
            lines[0].contains(&format!("the skill .claude/skills/scrape/SKILL.md sets user-invocable to {value} in its front matter")),
            "{why}: the line names the skill and quotes the value: {}",
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
        (&b"---\nname: scrape\nuser-invocable: true\n---\n"[..], "true"),
        (&b"---\nname: scrape\nuser-invocable: True\n---\n"[..], "True, which YAML reads as the boolean"),
        (&b"---\nname: scrape\nuser-invocable: \"true\"\n---\n"[..], "the exact text true in double quotes"),
        (&b"---\nname: scrape\nuser-invocable: 'true'\n---\n"[..], "the exact text true in single quotes"),
        (&b"---\nname: scrape\nuser-invocable: true  # false\n---\n"[..], "false only in the comment"),
        (&b"---\nname: scrape\n---\n"[..], "no user-invocable"),
        (&b"---\nname: scrape\n---\nuser-invocable: false\n"[..], "user-invocable outside the front matter"),
        (&b"# Scrape\n\nuser-invocable: false\n"[..], "no front matter"),
        (&b"---\n---\n"[..], "an empty front matter"),
        (&b"---\n# Finds jobs.\n---\n"[..], "a front matter of a comment only"),
    ] {
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{why}");
    }
}

/// Front matter that does not parse as one YAML mapping, or has no closing line, is in doubt whatever keys it seems to
/// hold: the command of the skill's folder and every command a raw `name` line names are missing, even beside a
/// command file.
#[test]
fn front_matter_in_doubt_fails_closed() {
    const YAML: &str = "does not parse as YAML: ";
    const OPEN: &str = "has no closing ---";
    const NOT_ONE: &str = "is not one YAML mapping";
    let doubtful = [
        (&b"---\nname: scrape\ndescription: [unclosed\n---\n"[..], YAML, "an unclosed list"),
        (&b"---\nname: scrape\nname: scrape\n---\n"[..], YAML, "a key given twice"),
        (&b"---\nname: scrape\n  bad: indent\n---\n"[..], YAML, "a bad indent"),
        (&b"---\nname: scrape\n"[..], OPEN, "a block with no closing line"),
        (&b"---\nname: 'scrape'\ndescription: Use when: the user asks\n---\n"[..], YAML, "a colon in a plain value, and a quoted name"),
        (&b"---\n  name: \" scrape \"  \ndescription: Use when: the user asks\n---\n"[..], YAML, "an indented name with spaces inside the quotes"),
        (&b"---\n\"name\": scrape\ndescription: [unclosed\n---\n"[..], YAML, "a key in double quotes"),
        (&b"---\n'name' : 'scrape'\ndescription: [unclosed\n---\n"[..], YAML, "a key in single quotes with a space before the colon"),
        (&b"---\nname : scrape\ndescription: Use when: the user asks\n---\n"[..], YAML, "a space before the colon"),
        (&b"---\nname\t: scrape\n"[..], OPEN, "a tab before the colon, with no closing line"),
        (&b"---\nname: scrape  # finds jobs\ndescription: [unclosed\n---\n"[..], YAML, "a trailing comment"),
        (&b"---\nname: 'scrape'\t# finds jobs\n"[..], OPEN, "a quoted name and a trailing comment after a tab"),
        (&b"---\nname: scrape\n- finds jobs\n---\n"[..], YAML, "a list item after a key"),
    ];
    for (bytes, reason, why) in doubtful {
        assert!(
            matches!(front_matter(bytes), FrontMatter::Doubtful(Doubt { names, unknown_name: false, .. }) if !names.is_empty() && names.iter().all(|name| name == "scrape")),
            "{why}: {:?}",
            front_matter(bytes)
        );
        for folder in ["other", "scrape"] {
            let mut files = complete();
            let skill = format!(".claude/skills/{folder}/SKILL.md");
            files.insert(skill.clone(), bytes.to_vec());
            let lines = unmet(&files);
            assert_eq!(lines.len(), 1, "{why}, in {folder}: {lines:#?}");
            assert!(lines[0].starts_with(&format!("/scrape is missing: the front matter of {skill} {reason}")), "{why}, in {folder}: {}", lines[0]);
        }
    }
    for (bytes, reason, why) in [
        (&b"---\ndescription: Use when: the user asks\nuser-invocable: false\n---\n"[..], YAML, "a colon in a plain value"),
        (&b"---\nuser-invocable: false\ndescription: [unclosed\n---\n"[..], YAML, "an unclosed list"),
        (&b"---\nuser-invocable: false\n"[..], OPEN, "a block with no closing line"),
        (&b"---\ndescription: Use when: the user asks\n---\n"[..], YAML, "a colon in a plain value, with no key the check reads"),
        (&b"---\ndescription: [unclosed\n---\n"[..], YAML, "an unclosed list, with no key the check reads"),
        (&b"---\n- name: scrape\n---\n"[..], NOT_ONE, "a list, not a mapping"),
        (&b"---\njust text\n---\n"[..], NOT_ONE, "text, not a mapping"),
        (&b"---\ndescription: Finds jobs.\n"[..], OPEN, "a block with no closing line, with no key the check reads"),
        (&b"---\nnames: scrape\nmy-name: scrape\nname-x: scrape\n  bad: indent\n---\n"[..], YAML, "keys that only hold name"),
    ] {
        assert!(
            matches!(front_matter(bytes), FrontMatter::Doubtful(Doubt { names, unknown_name: false, .. }) if names.is_empty()),
            "{why}: {:?}",
            front_matter(bytes)
        );
        let mut files = complete();
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: {lines:#?}");
        assert!(lines[0].starts_with(&format!("/scrape is missing: the front matter of .claude/skills/scrape/SKILL.md {reason}")), "{why}: {}", lines[0]);
        files.remove(&command_path("scrape"));
        assert_eq!(unmet(&files), lines, "{why}: with no command file, the skill in doubt is still what fails");
        files.insert(command_path("scrape"), b"Scrapes.".to_vec());
        files.remove(".claude/skills/scrape/SKILL.md");
        files.insert(".claude/skills/other/SKILL.md".to_owned(), bytes.to_vec());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{why}: a skill in doubt in another folder with no name line is not /scrape");
    }
}

/// The line for front matter in doubt says which doubt it is: no closing line, the YAML error with its line in the file,
/// or YAML that is not one mapping.
#[test]
fn the_line_says_why_front_matter_is_in_doubt() {
    for (bytes, said) in [
        (&b"---\nname: scrape\n"[..], "the front matter of .claude/skills/scrape/SKILL.md has no closing ---, "),
        (
            &b"---\nname: scrape\ndescription: Use when: the user asks\n---\n"[..],
            "the front matter of .claude/skills/scrape/SKILL.md does not parse as YAML: mapping values are not allowed in this context at line 3 of the file, ",
        ),
        (
            &b"\xef\xbb\xbf---\r\n# Scrape\r\n\r\ndescription: Use when: the user asks\r\n---\r\n"[..],
            "does not parse as YAML: mapping values are not allowed in this context at line 4 of the file, ",
        ),
        (&b"---\nname: scrape\nname: scrape\n---\n"[..], "duplicated key in mapping at line 3 of the file, "),
        (&b"---\n- name: scrape\n---\n"[..], "the front matter of .claude/skills/scrape/SKILL.md is not one YAML mapping, "),
    ] {
        let mut files = complete();
        files.insert(".claude/skills/scrape/SKILL.md".to_owned(), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        assert!(lines[0].contains(said), "{said:?}: {}", lines[0]);
    }
    let mut files = complete();
    files.insert(command_path("helper"), b"---\nname: scrape\n".to_vec());
    files.insert(".claude/skills/scrape/SKILL.md".to_owned(), b"---\n- name: scrape\n---\n".to_vec());
    let lines = unmet(&files);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(
        lines[0].starts_with(
            "/scrape is missing: the front matter of .claude/commands/helper.md has no closing ---, and the front matter of .claude/skills/scrape/SKILL.md is not one YAML mapping, so"
        ),
        "each file in doubt says its own doubt: {}",
        lines[0]
    );
}

/// A command file whose front matter is in doubt blocks its own command and every command a raw `name` line names.
#[test]
fn a_command_file_in_doubt_blocks_the_commands_it_could_be() {
    let mut files = complete();
    files.insert(command_path("helper"), b"---\nname : scrape\ndescription: Use when: the user asks\n---\n".to_vec());
    let lines = unmet(&files);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].starts_with("/scrape is missing: the front matter of .claude/commands/helper.md does not parse"), "{}", lines[0]);
    files.insert(command_path("helper"), b"---\ndescription: Use when: the user asks\n---\n".to_vec());
    assert_eq!(unmet(&files), Vec::<String>::new(), "a command file in doubt with no name line blocks only its own command");
}

/// A `name:` that is not plain, non-empty text in printable ASCII leaves the command Claude Code makes of the skill
/// unknown, and it could be any command, so the check fails, wherever the skill is and whatever command files there are.
#[test]
fn a_name_that_is_not_text_fails_the_check() {
    for bytes in [
        &b"---\nname: 12\n---\n"[..],
        &b"---\nname: [x]\n---\n"[..],
        &b"---\nname: [add-template]\n---\n"[..],
        &b"---\nname:\n---\n"[..],
        &b"---\nname: ''\n---\n"[..],
        &b"---\nname: '  '\n---\n"[..],
        &b"---\nname: \"\\tscrape\"\n---\n"[..],
        &b"---\nname: \"scr\\x01ape\"\n---\n"[..],
        &b"---\nname: \"\\ufeffscrape\"\n---\n"[..],
        &b"---\nname: scrape\xc2\xa0\n---\n"[..],
        &b"---\nname: \"\xc2\xa0scrape\"\n---\n"[..],
        &b"---\nname: scr\xc3\xa4pe\n---\n"[..],
    ] {
        assert_eq!(skill_name(bytes), SkillName::Unknown, "{:?}", String::from_utf8_lossy(bytes));
        for folder in ["other", "scrape"] {
            let mut files = complete();
            let skill = format!(".claude/skills/{folder}/SKILL.md");
            files.insert(skill.clone(), bytes.to_vec());
            let lines = unmet(&files);
            assert_eq!(lines.len(), 1, "{lines:#?}");
            assert!(lines[0].starts_with(&format!("{skill} gives a name: that is not plain, non-empty text in printable ASCII")), "{}", lines[0]);
        }
    }
}

/// In front matter in doubt, a `name` line with no value on it, whose value could be on the next line, with a value
/// outside printable ASCII, or with a value out of quotes that starts with a YAML indicator, so that it is no plain text,
/// leaves the name unknown, so the check fails and names the file, a skill's or a command file's, wherever it is.
#[test]
fn a_name_line_in_doubt_with_no_known_value_fails_the_check() {
    for bytes in [
        &b"---\nname:\n  scrape\ndescription: Use when: the user asks\n---\n"[..],
        &b"---\nname: # the scraper\n  scrape\ndescription: [unclosed\n---\n"[..],
        &b"---\n\"name\" :\t\nscrape\n"[..],
        &b"---\nname: ''\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: scrape\xc2\xa0\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: \"\xef\xbb\xbfscrape\"\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: scr\x01ape\n"[..],
        &b"---\nname: other\nname: \xc2\xa0\n---\n"[..],
        &b"---\nname: [add-template]\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: >\n  add-template\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: |\n  add-template\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: >- # folded\n  add-template\n"[..],
        &b"---\nname: {add-template}\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: &anchor add-template\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: *anchor\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: !!str add-template\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: %add-template\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: @add-template\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: `add-template`\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: - add-template\n---\n"[..],
        &b"---\nname: -\t\n  - add-template\n"[..],
        &b"---\nname: ]add-template\n"[..],
        &b"---\nname: 'add-template\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: \"add-template\" x\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: ? add-template\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: \"add\\x2dtemplate\"\ndescription: [unclosed\n---\n"[..],
        &b"---\nname: \"x\\\"y\"\ndescription: [unclosed\n---\n"[..],
    ] {
        assert!(
            matches!(front_matter(bytes), FrontMatter::Doubtful(Doubt { unknown_name: true, .. })),
            "{:?}: {:?}",
            String::from_utf8_lossy(bytes),
            front_matter(bytes)
        );
        for file in [".claude/skills/other/SKILL.md".to_owned(), command_path("helper")] {
            let mut files = complete();
            files.insert(file.clone(), bytes.to_vec());
            let lines = unmet(&files);
            assert_eq!(lines.len(), 1, "{lines:#?}");
            assert!(lines[0].starts_with(&format!("{file} gives a name: {NOT_KNOWN_IN_DOUBT}, in front matter that ")), "{}", lines[0]);
        }
    }
}

/// In front matter in doubt, a value in quotes is text whatever it starts with, and a value out of quotes that starts
/// with `-`, `?` or `:` and no space after it is plain text, so each is a name the check knows.
#[test]
fn a_name_line_in_doubt_with_plain_text_is_a_known_name() {
    for (bytes, name) in [
        (&b"---\nname: '[add-template]'\ndescription: [unclosed\n---\n"[..], "[add-template]"),
        (&b"---\nname: \"> add-template\"\ndescription: [unclosed\n---\n"[..], "> add-template"),
        (&b"---\nname: -add-template\ndescription: [unclosed\n---\n"[..], "-add-template"),
        (&b"---\nname: ?add-template\ndescription: [unclosed\n---\n"[..], "?add-template"),
        (&b"---\nname: add-[template]\ndescription: [unclosed\n---\n"[..], "add-[template]"),
        (&b"---\nname: 'add\\template'\ndescription: [unclosed\n---\n"[..], "add\\template"),
    ] {
        assert!(
            matches!(front_matter(bytes), FrontMatter::Doubtful(Doubt { ref names, unknown_name: false, .. }) if names == &[name.to_owned()]),
            "{:?}: {:?}",
            String::from_utf8_lossy(bytes),
            front_matter(bytes)
        );
        let mut files = complete();
        files.insert(".claude/skills/other/SKILL.md".to_owned(), bytes.to_vec());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{:?}: a name that is no command blocks none", String::from_utf8_lossy(bytes));
    }
}

/// A command file follows the same front matter rules as a skill.
#[test]
fn a_command_file_the_user_cannot_invoke_is_not_the_command() {
    let mut files = complete();
    for (bytes, value, why) in [
        (&b"---\nuser-invocable: false\n---\nScrapes.\n"[..], "false", "false"),
        (&b"---\ndescription: Scrapes.\nuser-invocable: off # internal\n---\n"[..], "\"off\"", "off and a comment"),
        (&b"\xef\xbb\xbf---\nuser-invocable: 0\n---\n"[..], "0", "0 after a BOM"),
        (&b"---\nuser-invocable: yes\n---\n"[..], "\"yes\"", "yes"),
        (&b"---\nuser-invocable: ''\n---\n"[..], "\"\"", "empty text"),
        (&b"---\nuser-invocable: \"True\"\n---\n"[..], "\"True\"", "True as text"),
        (&b"---\nuser-invocable: ' true '\n---\n"[..], "\" true \"", "true as text with spaces around it"),
    ] {
        files.insert(command_path("scrape"), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: {lines:#?}");
        assert!(
            lines[0]
                .starts_with(&format!("/scrape is missing: the command file .claude/commands/scrape.md sets user-invocable to {value} in its front matter")),
            "{why}: {}",
            lines[0]
        );
    }
    for (bytes, reason, why) in [
        (&b"---\ndescription: [unclosed\nuser-invocable: false\n---\n"[..], "does not parse as YAML: ", "front matter that does not parse"),
        (&b"---\ndescription: Use when: the user asks\nuser-invocable: true\n---\n"[..], "does not parse as YAML: ", "a colon in a plain value"),
        (&b"---\nuser-invocable: false\n"[..], "has no closing ---", "a block with no closing line"),
        (&b"---\ndescription: Use when: the user asks\n---\n"[..], "does not parse as YAML: ", "front matter that does not parse, with neither key"),
        (&b"---\n- user-invocable: true\n---\n"[..], "is not one YAML mapping", "a list"),
    ] {
        files.insert(command_path("scrape"), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{why}: {lines:#?}");
        assert!(lines[0].starts_with(&format!("/scrape is missing: the front matter of .claude/commands/scrape.md {reason}")), "{why}: {}", lines[0]);
    }
    for (bytes, why) in [
        (&b"---\nuser-invocable: true\n---\n"[..], "true"),
        (&b"---\nuser-invocable: \"true\"\n---\n"[..], "the exact text true"),
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
            "/add-template is missing: the skill .claude/skills/templates/SKILL.md sets user-invocable to false in its front matter, and only true, or no user-invocable, is one the user can invoke, and it is what runs in place of {command}"
        )),
        "{}",
        lines[0]
    );
}

/// A `name:` with spaces around it is in doubt: the command its trimmed text names is missing, whichever file would
/// otherwise run for it, and the line names the skill.
#[test]
fn a_padded_name_blocks_the_command_it_holds() {
    let skill = ".claude/skills/templates/SKILL.md";
    for bytes in [
        format!("---\nname: \" add-template \"\n---\n{ACTIVE_TEMPLATE}\n"),
        format!("---\nname: 'add-template '\n---\n{ACTIVE_TEMPLATE}\n"),
        "---\nname: \" add-template \"\nuser-invocable: false\n---\n".to_owned(),
    ] {
        let mut files = complete();
        assert!(String::from_utf8_lossy(&files[&command_path("add-template")]).contains(ACTIVE_TEMPLATE), "the command file is sound");
        files.insert(skill.to_owned(), bytes.clone().into_bytes());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{bytes:?}: {lines:#?}");
        assert!(lines[0].starts_with(&format!("/add-template is missing: {skill} gives name: add-template with spaces around it")), "{bytes:?}: {}", lines[0]);
        files.insert(".claude/skills/add-template/SKILL.md".to_owned(), format!("{ACTIVE_TEMPLATE}\n").into_bytes());
        assert_eq!(unmet(&files), lines, "{bytes:?}: a sound skill of that name does not stand in for it");
    }
    let mut files = complete();
    files.insert(skill.to_owned(), format!("---\nname: add-template\n---\n{ACTIVE_TEMPLATE}\n").into_bytes());
    files.insert(command_path("add-template"), b"Writes no block.".to_vec());
    assert_eq!(unmet(&files), Vec::<String>::new(), "an exact name is the command it holds, and wins over the command file");
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

/// Some Claude Code versions also run a skill for its folder's name when its `name:` gives another, so a skill in a
/// folder of a command's name with another sound `name:`, or one with spaces around it, leaves that command in doubt,
/// whatever command file there is, and beside another skill named for the command it is a second candidate.
#[test]
fn a_skill_in_a_folder_of_a_command_with_another_name_fails_closed() {
    let skill = ".claude/skills/add-template/SKILL.md";
    for (bytes, gives) in [
        ("---\nname: helper\n---\nWrites no block.\n".to_owned(), "name: helper"),
        (format!("---\nname: helper\n---\n{ACTIVE_TEMPLATE}\n"), "name: helper"),
        ("---\nname: ' helper'\n---\nWrites no block.\n".to_owned(), "name: helper with spaces around it"),
    ] {
        let mut files = complete();
        assert!(String::from_utf8_lossy(&files[&command_path("add-template")]).contains(ACTIVE_TEMPLATE), "the command file is sound");
        files.insert(skill.to_owned(), bytes.into_bytes());
        let alias = format!(
            "/add-template is missing: the skill {skill} is in the folder add-template but gives {gives}, and some Claude Code versions run a skill for its folder's name too, so whether Claude Code runs it for /add-template is not known"
        );
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{gives}: {lines:#?}");
        assert!(lines[0].starts_with(&alias), "{gives}: the line names the skill, its folder and its name: {}", lines[0]);
        files.remove(&command_path("add-template"));
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{gives}, with no command file: {lines:#?}");
        assert!(lines[0].starts_with(&alias), "{gives}, with no command file: {}", lines[0]);
        files.insert(".claude/skills/templates/SKILL.md".to_owned(), format!("---\nname: add-template\n---\n{ACTIVE_TEMPLATE}\n").into_bytes());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{gives}, beside a skill named add-template: {lines:#?}");
        assert!(
            lines[0].starts_with(&format!(
                "/add-template is missing: .claude/skills/templates/SKILL.md, a skill named add-template, and {skill}, in the folder add-template but with {gives}, could each run for /add-template, so the one Claude Code would run is not known"
            )),
            "{gives}: a folder and a name that point at one command are two candidates: {}",
            lines[0]
        );
    }
    for (bytes, why) in [
        (format!("---\nname: add-template\n---\n{ACTIVE_TEMPLATE}\n"), "a name: of its folder"),
        (format!("---\ndescription: Adds a template.\n---\n{ACTIVE_TEMPLATE}\n"), "no name:"),
        (format!("{ACTIVE_TEMPLATE}\n"), "no front matter"),
    ] {
        let mut files = complete();
        files.insert(command_path("add-template"), b"Writes no block.".to_vec());
        files.insert(skill.to_owned(), bytes.into_bytes());
        assert_eq!(unmet(&files), Vec::<String>::new(), "{why}: the skill is the command of its folder, and wins over the command file");
    }
}

/// A release that ships commands as skills in folders of their own names, each named, and helpers in folders that are no
/// command, passes: the shape of v1.7.5, whose `/scrape` is `job-scraper` named `scrape` and `/upskill` is `upskill`.
#[test]
fn a_sound_skill_layout_passes() {
    let mut files = complete();
    files.remove(&command_path("scrape"));
    files.remove(&command_path("upskill"));
    files.remove(&command_path("add-template"));
    files.insert(".claude/skills/job-scraper/SKILL.md".to_owned(), b"---\nname: scrape\ndescription: Finds jobs.\n---\n".to_vec());
    files.insert(".claude/skills/upskill/SKILL.md".to_owned(), b"---\nname: upskill\ndescription: Plans learning.\n---\n".to_vec());
    files.insert(".claude/skills/templates/SKILL.md".to_owned(), format!("---\nname: add-template\n---\n{ACTIVE_TEMPLATE}\n").into_bytes());
    files.insert(".claude/skills/helper-tools/SKILL.md".to_owned(), b"---\nname: helper-tools\n---\n".to_vec());
    files.insert(".claude/skills/notes/SKILL.md".to_owned(), b"---\nname: other-notes\n---\n".to_vec());
    assert_eq!(unmet(&files), Vec::<String>::new());
}

/// `/apply`'s command file is the marker, needed as a file whatever runs for `/apply`, and `/apply` is held to the same
/// rules as every other command.
#[test]
fn apply_is_held_to_the_command_rules_as_well() {
    for (bytes, said) in [
        (&b"---\nuser-invocable: false\n---\nApplies.\n"[..], "/apply is missing: the command file .claude/commands/apply.md sets user-invocable to false"),
        (&b"---\ndescription: Use when: the user asks\n---\n"[..], "/apply is missing: the front matter of .claude/commands/apply.md does not parse as YAML"),
        (&b"---\ndescription: Applies.\n"[..], "/apply is missing: the front matter of .claude/commands/apply.md has no closing ---"),
    ] {
        let mut files = complete();
        files.insert(MARKER.to_owned(), bytes.to_vec());
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        assert!(lines[0].starts_with(said), "{}", lines[0]);
    }
    for (skills, said) in [
        (
            &[(".claude/skills/applying/SKILL.md", &b"---\nname: apply\nuser-invocable: false\n---\n"[..])][..],
            "/apply is missing: the skill .claude/skills/applying/SKILL.md sets user-invocable to false in its front matter, and only true, or no user-invocable, is one the user can invoke, and it is what runs in place of .claude/commands/apply.md",
        ),
        (
            &[(".claude/skills/apply/SKILL.md", &b"# Apply\n"[..]), (".claude/skills/applying/SKILL.md", &b"---\nname: apply\n---\n"[..])][..],
            "/apply is missing: .claude/skills/apply/SKILL.md and .claude/skills/applying/SKILL.md are each a skill named apply",
        ),
        (
            &[(".claude/skills/applying/SKILL.md", &b"---\nname: \" apply \"\n---\n"[..])][..],
            "/apply is missing: .claude/skills/applying/SKILL.md gives name: apply with spaces around it",
        ),
    ] {
        let mut files = complete();
        for (path, bytes) in skills {
            files.insert((*path).to_owned(), bytes.to_vec());
        }
        let lines = unmet(&files);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        assert!(lines[0].starts_with(said), "{}", lines[0]);
    }
    let mut files = complete();
    files.insert(".claude/skills/applying/SKILL.md".to_owned(), b"---\nname: apply\n---\n".to_vec());
    assert_eq!(unmet(&files), Vec::<String>::new(), "a sound skill named apply runs for /apply, beside the marker");
    files.remove(MARKER);
    let lines = unmet(&files);
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].starts_with(".claude/commands/apply.md is missing"), "the marker is needed even when a skill runs for /apply: {}", lines[0]);
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
