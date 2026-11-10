use chrono::{DateTime, NaiveDate, Utc};

use crate::portals::domain::manifest::{NAME_CHARS, SENTENCE_CHARS, parse_manifest};
use crate::shared_kernel::visible_text::visible_chars;

/// The repository of community portals on GitHub, as `owner/name`.
pub const COMMUNITY_REPOSITORY: &str = "ai-job-search-dev/community-portals";

/// A country folder of the community list: two lower-case letters, such as `dk`, or `global`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CountryCode(String);

impl CountryCode {
    pub fn parse(text: &str) -> Option<Self> {
        let iso = text.len() == 2 && text.bytes().all(|b| b.is_ascii_lowercase());
        (iso || text == "global").then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_global(&self) -> bool {
        self.0 == "global"
    }
}

/// A portal's folder name, such as `jobnet-search`: lower-case letters, digits and `-`, at most 64, with neither end a `-`.
/// It goes into a path, an address and a task's words, so nothing else gets through.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FolderName(String);

impl FolderName {
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let allowed = bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
        let ends = bytes.first().is_some_and(|b| *b != b'-') && bytes.last().is_some_and(|b| *b != b'-');
        (allowed && ends && bytes.len() <= 64).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A git object name: 40 lower-case hex digits. It goes into a blob's address.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlobSha(String);

impl BlobSha {
    pub fn parse(text: &str) -> Option<Self> {
        (text.len() == 40 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The country and folder of a path in the community repository, which must be exactly `portals/<country>/<folder>/SKILL.md`.
pub fn parse_tree_path(path: &str) -> Option<(CountryCode, FolderName)> {
    let inner = path.strip_prefix("portals/")?.strip_suffix("/SKILL.md")?;
    let (country, folder) = inner.split_once('/')?;
    Some((CountryCode::parse(country)?, FolderName::parse(folder)?))
}

/// The address of a portal's folder on GitHub, built only from validated parts.
pub fn code_url(country: &CountryCode, folder: &FolderName) -> String {
    format!("https://github.com/{COMMUNITY_REPOSITORY}/tree/main/portals/{}/{}", country.as_str(), folder.as_str())
}

/// The task Install… opens in the launcher, in plain words on one line, built only from validated parts.
/// Claude copies the folder, explains its code, and asks before it installs the portal's command-line tools.
pub fn install_prompt(country: &CountryCode, folder: &FolderName) -> String {
    format!(
        "Copy {} into `.agents/skills/`. Read its code and tell me what it does. Then ask me before you run `python3 tools/install_portal_clis.py`.",
        code_url(country, folder)
    )
}

/// One portal of the community list, as its `SKILL.md` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedPortal {
    pub country: CountryCode,
    pub folder: FolderName,
    /// One line of plain text, at most 60 characters.
    pub name: String,
    /// One line of plain text, at most 200 characters.
    pub sentence: Option<String>,
    /// The `SKILL.md` blob it was read from, so an unchanged file is not fetched again.
    pub sha: BlobSha,
}

impl ListedPortal {
    /// Reads the name and sentence from a fetched `SKILL.md`, by the rules of installed portals.
    pub fn from_manifest(country: CountryCode, folder: FolderName, sha: BlobSha, text: &str) -> Self {
        let manifest = parse_manifest(text, folder.as_str());
        let name = Some(one_line(&manifest.name, NAME_CHARS)).filter(|name| !name.is_empty()).unwrap_or_else(|| folder.as_str().to_owned());
        Self { country, folder, name, sentence: clean_sentence(manifest.sentence.as_deref()), sha }
    }

    /// A portal from saved words, cleaned again: `None` when no name is left.
    pub fn from_parts(country: CountryCode, folder: FolderName, sha: BlobSha, name: &str, sentence: Option<&str>) -> Option<Self> {
        let name = one_line(name, NAME_CHARS);
        (!name.is_empty()).then(|| Self { country, folder, name, sentence: clean_sentence(sentence), sha })
    }
}

fn clean_sentence(sentence: Option<&str>) -> Option<String> {
    sentence.map(|sentence| one_line(sentence, SENTENCE_CHARS)).filter(|sentence| !sentence.is_empty())
}

/// `text` on one line: a line break or other space becomes one space, then hidden characters and variation selectors
/// go, and it keeps at most `limit` characters with no space at either end. A variation selector passes `visible_chars`,
/// but after a letter it shows nothing and can carry a payload.
fn one_line(text: &str, limit: usize) -> String {
    let spaced: String = text.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).collect();
    let visible: String = visible_chars(&spaced).filter(|c| !matches!(c, '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')).collect();
    let joined = visible.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(limit).collect::<String>().trim_end().to_owned()
}

/// Who looks after a portal, from `.github/CODEOWNERS`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ownership {
    /// GitHub handles such as `@jdoe` or `@org/team`.
    pub owners: Vec<String>,
    /// The first day of the month a `# unmaintained since YYYY-MM` comment names.
    pub unmaintained_since: Option<NaiveDate>,
}

/// The most owners a line keeps.
const MAX_OWNERS: usize = 10;

/// The lines of a `CODEOWNERS` file Jobdesk reads: `*` for every folder, or a plain path, each with its owners.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Codeowners {
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    /// `None` for `*` or `**`, which match every path. Otherwise the path without a leading `/` or a trailing `/` or `/**`.
    path: Option<String>,
    ownership: Ownership,
}

/// Reads a `CODEOWNERS` file. A line holds a pattern, then owners, then perhaps a `#` comment. Only `*`, `**` and paths
/// without any other wildcard are read, because that is all the community list uses: other lines are left out. Owners
/// that are not GitHub handles, such as e-mail addresses, are dropped, and a line keeps at most 10.
pub fn parse_codeowners(text: &str) -> Codeowners {
    Codeowners { rules: text.lines().filter_map(rule).collect() }
}

fn rule(line: &str) -> Option<Rule> {
    let comment_at = line.char_indices().find(|&(at, c)| c == '#' && (at == 0 || line[..at].ends_with([' ', '\t']))).map(|(at, _)| at);
    let (rule, comment) = comment_at.map_or((line, ""), |at| (&line[..at], &line[at + 1..]));
    let mut words = rule.split_whitespace();
    let path = match words.next()? {
        "*" | "**" => None,
        pattern => {
            let path = pattern.trim_start_matches('/');
            let path = path.strip_suffix("/**").unwrap_or(path).trim_end_matches('/');
            if path.is_empty() || path.contains(['*', '?', '[', '\\', '!']) {
                return None;
            }
            Some(path.to_owned())
        }
    };
    let owners = words.filter(|word| is_handle(word)).take(MAX_OWNERS).map(str::to_owned).collect();
    Some(Rule { path, ownership: Ownership { owners, unmaintained_since: unmaintained_since(comment) } })
}

/// `@user`, with 1 to 39 letters, digits or `-`, or `@org/team`, with a team name of at most 100 characters.
fn is_handle(word: &str) -> bool {
    let Some(handle) = word.strip_prefix('@') else { return false };
    let (user, team) = handle.split_once('/').map_or((handle, None), |(user, team)| (user, Some(team)));
    let user_ok = (1..=39).contains(&user.len()) && user.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let team_ok = team.is_none_or(|team| (1..=100).contains(&team.len()) && team.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-')));
    user_ok && team_ok
}

/// `unmaintained since 2026-03`, in any case, as the first of that month. Any other comment, or a month that does not
/// exist, reads as none.
fn unmaintained_since(comment: &str) -> Option<NaiveDate> {
    let words: Vec<&str> = comment.split_whitespace().collect();
    let [first, second, date] = words[..] else { return None };
    if !first.eq_ignore_ascii_case("unmaintained") || !second.eq_ignore_ascii_case("since") {
        return None;
    }
    let (year, month) = date.split_once('-')?;
    let digits = |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    if !digits(year, 4) || !digits(month, 2) {
        return None;
    }
    NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, 1)
}

impl Codeowners {
    /// Who owns `portals/<country>/<folder>`: the last line whose pattern matches it wins, as on GitHub, even one with
    /// no owners left. A folder no line matches has no owners.
    pub fn ownership(&self, country: &CountryCode, folder: &FolderName) -> Ownership {
        let target = format!("portals/{}/{}", country.as_str(), folder.as_str());
        let matches =
            |rule: &&Rule| rule.path.as_deref().is_none_or(|path| target == path || target.strip_prefix(path).is_some_and(|rest| rest.starts_with('/')));
        self.rules.iter().rev().find(matches).map(|rule| rule.ownership.clone()).unwrap_or_default()
    }

    /// The lines read, written again as `CODEOWNERS` text that `parse_codeowners` reads back to the same rules.
    pub fn to_text(&self) -> String {
        self.rules
            .iter()
            .map(|rule| {
                let mut line = rule.path.as_ref().map_or_else(|| "*".to_owned(), |path| format!("/{path}/"));
                for owner in &rule.ownership.owners {
                    line.push(' ');
                    line.push_str(owner);
                }
                if let Some(since) = rule.ownership.unmaintained_since {
                    line.push_str(&since.format(" # unmaintained since %Y-%m").to_string());
                }
                line + "\n"
            })
            .collect()
    }
}

/// The community list as last read in full from GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityList {
    pub saved_at: DateTime<Utc>,
    pub portals: Vec<ListedPortal>,
    pub codeowners: Codeowners,
    /// The `CODEOWNERS` blob it was read from, if the repository has one.
    pub codeowners_sha: Option<BlobSha>,
}

/// Why the community list could not be read, in Jobdesk's own words. No text from the server is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunityFailure {
    /// GitHub's limit for requests without sign-in is used up until this time.
    Limited {
        until: DateTime<Utc>,
    },
    /// GitHub answered with this status code.
    Refused(u16),
    TimedOut,
    Unreachable,
    /// GitHub's answer was too large or could not be read.
    BadAnswer,
}

/// Reads the community list from GitHub. Reading it is not a run: it starts no `claude` and writes no run log.
pub trait CommunitySource: Send {
    /// Starts reading the list again. `saved` is the list read last, whose unchanged files are not fetched again.
    /// The answer comes back through `poll`.
    fn fetch(&mut self, saved: Option<CommunityList>);

    fn poll(&mut self) -> Option<Result<CommunityList, CommunityFailure>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dk() -> CountryCode {
        CountryCode::parse("dk").unwrap()
    }

    fn folder(name: &str) -> FolderName {
        FolderName::parse(name).unwrap()
    }

    #[test]
    fn only_a_skill_file_two_folders_down_is_a_portal() {
        assert_eq!(parse_tree_path("portals/dk/jobnet-search/SKILL.md"), Some((dk(), folder("jobnet-search"))));
        assert_eq!(parse_tree_path("portals/global/a/SKILL.md").map(|(country, _)| country.is_global()), Some(true));
        for bad in [
            "portals/dk/../SKILL.md",
            "portals/../dk/x/SKILL.md",
            "portals/dk/../../etc/SKILL.md",
            "portals/DK/jobnet-search/SKILL.md",
            "portals/dk/Jobnet-search/SKILL.md",
            "portals/dk/job net/SKILL.md",
            "portals/dk/jobnet-search/cli/SKILL.md",
            "portals/dk/jobnet-search/skill.md",
            "portals/dk/SKILL.md",
            "portals/dnk/x/SKILL.md",
            "portals/dk/-x/SKILL.md",
            "portals/dk/x-/SKILL.md",
            "portals/dk//SKILL.md",
            "/portals/dk/x/SKILL.md",
            "other/dk/x/SKILL.md",
            "portals/dk/x/SKILL.md/",
            "portals/dk/x\n/SKILL.md",
        ] {
            assert_eq!(parse_tree_path(bad), None, "{bad:?}");
        }
        assert!(FolderName::parse(&"a".repeat(64)).is_some() && FolderName::parse(&"a".repeat(65)).is_none());
    }

    #[test]
    fn a_blob_name_is_forty_lower_case_hex_digits() {
        assert!(BlobSha::parse("fee7eb5d913f1d5b584b84032d9f16a7b2d14757").is_some());
        for bad in ["FEE7EB5D913F1D5B584B84032D9F16A7B2D14757", "fee7eb5d", "../../../../x", "fee7eb5d913f1d5b584b84032d9f16a7b2d1475g"] {
            assert!(BlobSha::parse(bad).is_none(), "{bad}");
        }
    }

    const OWNERS: &str = "\
# Fallback first
* @maintainers-fictional
/portals/dk/jobnet-search/ @ada-quill @calder/portal_team.v2 # unmaintained since 2026-03
/portals/dk/jobnet-search/ @bram-oak not-a-handle bram@example.com @@x @toolong-handle-that-goes-past-thirty-nine-chars
portals/be/northfield-search @lena-vos # no longer looked after
/portals/be/ @belgium-desk
/portals/global/orphan-search/
/portals/*/glob-search/ @never
/portals/dk/old-search/ @old # Unmaintained since 2025-13
";

    #[test]
    fn the_last_matching_line_wins() {
        let owners = parse_codeowners(OWNERS);
        let jobnet = owners.ownership(&dk(), &folder("jobnet-search"));
        assert_eq!(jobnet, Ownership { owners: vec!["@bram-oak".into()], unmaintained_since: None }, "the second line wins, its bad handles dropped");
        let belgium = CountryCode::parse("be").unwrap();
        assert_eq!(owners.ownership(&belgium, &folder("northfield-search")).owners, ["@belgium-desk"], "a later folder line covers it");
        assert_eq!(owners.ownership(&belgium, &folder("northfield-searcher")).owners, ["@belgium-desk"]);
        assert_eq!(owners.ownership(&dk(), &folder("jobindex-search")).owners, ["@maintainers-fictional"], "the fallback");
        let global = CountryCode::parse("global").unwrap();
        assert_eq!(owners.ownership(&global, &folder("orphan-search")), Ownership::default(), "a line with no owners still wins");
        assert_eq!(owners.ownership(&global, &folder("glob-search")).owners, ["@maintainers-fictional"], "other wildcards are not read");
        assert_eq!(parse_codeowners("").ownership(&dk(), &folder("x")), Ownership::default());
    }

    #[test]
    fn an_unmaintained_comment_names_a_real_month() {
        let owners = parse_codeowners(OWNERS);
        let first = parse_codeowners(&OWNERS.lines().take(3).collect::<Vec<_>>().join("\n"));
        let jobnet = first.ownership(&dk(), &folder("jobnet-search"));
        assert_eq!(jobnet.owners, ["@ada-quill", "@calder/portal_team.v2"]);
        assert_eq!(jobnet.unmaintained_since, NaiveDate::from_ymd_opt(2026, 3, 1));
        assert_eq!(owners.ownership(&dk(), &folder("old-search")).unmaintained_since, None, "month 13");
        for comment in [
            "unmaintained since 2026-3",
            "unmaintained since 26-03",
            "unmaintained since 2026-03 maybe",
            "maintained since 2026-03",
            "unmaintained since 2026-00",
        ] {
            assert_eq!(unmaintained_since(comment), None, "{comment}");
        }
        assert_eq!(unmaintained_since(" UNMAINTAINED Since 2024-12 "), NaiveDate::from_ymd_opt(2024, 12, 1));
    }

    #[test]
    fn a_line_keeps_ten_owners_and_a_team_name_of_a_hundred_characters() {
        let many: Vec<String> = (0..12).map(|n| format!("@owner-{n}")).collect();
        let owners = parse_codeowners(&format!("* {}", many.join(" ")));
        assert_eq!(owners.ownership(&dk(), &folder("x")).owners, many[..10]);
        assert!(is_handle(&format!("@org/{}", "t".repeat(100))));
        assert!(!is_handle(&format!("@org/{}", "t".repeat(101))));
    }

    #[test]
    fn codeowners_written_again_read_back_the_same() {
        let owners = parse_codeowners(OWNERS);
        assert_eq!(parse_codeowners(&owners.to_text()), owners);
        assert!(!owners.to_text().contains("not-a-handle"));
    }

    #[test]
    fn a_name_with_hidden_characters_and_breaks_becomes_one_clean_line() {
        let sha = BlobSha::parse(&"a".repeat(40)).unwrap();
        let text = "# North\u{200B}field\u{202E} Search Skill\n\nFinds   jobs\u{1b}[31m in\nBelgium. More text.\n";
        let portal = ListedPortal::from_manifest(dk(), folder("northfield-search"), sha.clone(), text);
        assert_eq!(portal.name, "Northfield");
        assert_eq!(portal.sentence.as_deref(), Some("Finds jobs in Belgium."));
        let saved = ListedPortal::from_parts(dk(), folder("x"), sha.clone(), " Two\nlines\u{2028}here \t", Some("\u{200B}\n")).unwrap();
        assert_eq!((saved.name.as_str(), saved.sentence), ("Two lines here", None));
        assert_eq!(ListedPortal::from_parts(dk(), folder("x"), sha.clone(), "\u{200B}\n", None), None);
        let long = ListedPortal::from_parts(dk(), folder("x"), sha, &format!("{} b", "a".repeat(59)), None).unwrap();
        assert_eq!(long.name, "a".repeat(59), "cut, with no space left at the end");
    }

    #[test]
    fn variation_selectors_are_dropped_from_names_and_sentences() {
        let sha = BlobSha::parse(&"a".repeat(40)).unwrap();
        let name = "North\u{FE0F}field\u{E0100}";
        let portal = ListedPortal::from_parts(dk(), folder("x"), sha, name, Some("Finds\u{FE00} jobs\u{E01EF}.")).unwrap();
        assert_eq!((portal.name.as_str(), portal.sentence.as_deref()), ("Northfield", Some("Finds jobs.")));
    }

    #[test]
    fn the_prompt_and_the_address_take_only_validated_parts() {
        let global = CountryCode::parse("global").unwrap();
        assert_eq!(
            code_url(&global, &folder("freehire-search")),
            "https://github.com/ai-job-search-dev/community-portals/tree/main/portals/global/freehire-search"
        );
        let prompt = install_prompt(&dk(), &folder("jobnet-search"));
        assert_eq!(
            prompt,
            "Copy https://github.com/ai-job-search-dev/community-portals/tree/main/portals/dk/jobnet-search into `.agents/skills/`. Read its code and tell me what it does. Then ask me before you run `python3 tools/install_portal_clis.py`."
        );
        assert!(!prompt.contains('\n') && visible_chars(&prompt).eq(prompt.chars()));
    }
}
