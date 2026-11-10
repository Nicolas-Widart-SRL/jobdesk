use std::collections::VecDeque;
use std::io::{self, Read};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;
use ureq::Agent;

use crate::portals::domain::{
    BlobSha, COMMUNITY_REPOSITORY, Codeowners, CommunityFailure, CommunityList, CommunitySource, ListedPortal, parse_codeowners, parse_tree_path,
};
use crate::portals::infrastructure::community_cache::CommunityCache;
use crate::shared_kernel::https::{TimedBody, github_config};

/// The most of the repository's file list that is read. A longer one is refused whole.
pub const TREE_LIMIT: u64 = 1024 * 1024;
/// The most of one `SKILL.md` or of `CODEOWNERS` that is read. A longer one is refused whole.
pub const BLOB_LIMIT: u64 = 256 * 1024;
/// One request, redirects and body included.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// GitHub's own type for its answers, and for a file's bytes as they are.
const JSON: &str = "application/vnd.github+json";
const RAW: &str = "application/vnd.github.raw+json";
/// The longest Jobdesk waits on a rate limit, whatever GitHub says: its limit resets every hour.
const LONGEST_WAIT: TimeDelta = TimeDelta::hours(1);
/// The wait when GitHub names neither a reset time nor a delay, as its documentation advises.
const DEFAULT_WAIT: TimeDelta = TimeDelta::minutes(1);

/// Every file of the repository's default branch, in one request.
pub fn tree_url() -> String {
    format!("https://api.github.com/repos/{COMMUNITY_REPOSITORY}/git/trees/main?recursive=1")
}

fn blob_url(sha: &BlobSha) -> String {
    format!("https://api.github.com/repos/{COMMUNITY_REPOSITORY}/git/blobs/{}", sha.as_str())
}

/// GitHub's REST API, without sign-in. Tests put a fake in its place.
pub trait Api: Send + Sync {
    /// The body of `url` as `accept` asks for it, or why there is none. A body over `limit` bytes is `BadAnswer`.
    fn get(&self, url: &str, accept: &str, limit: u64) -> Result<Vec<u8>, CommunityFailure>;
}

/// GitHub as `github_config` reaches it, with no credential. A status is read here, so a rate limit tells its reset.
pub struct HttpsApi {
    agent: Agent,
}

impl HttpsApi {
    pub fn new(version: &str) -> Self {
        Self { agent: github_config(version).http_status_as_error(false).timeout_global(Some(REQUEST_TIMEOUT)).build().new_agent() }
    }
}

impl Api for HttpsApi {
    fn get(&self, url: &str, accept: &str, limit: u64) -> Result<Vec<u8>, CommunityFailure> {
        let response = self.agent.get(url).header("Accept", accept).header("X-GitHub-Api-Version", "2022-11-28").call().map_err(|error| failure_of(&error))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let header = |name: &str| response.headers().get(name).and_then(|value| value.to_str().ok());
            let limit = RateHeaders { remaining: header("x-ratelimit-remaining"), reset: header("x-ratelimit-reset"), retry_after: header("retry-after") };
            return Err(status_failure(status, &limit, Utc::now()));
        }
        let mut bytes = Vec::new();
        TimedBody(response.into_body().into_reader()).take(limit + 1).read_to_end(&mut bytes).map_err(|error| read_failure(&error))?;
        if bytes.len() as u64 > limit {
            return Err(CommunityFailure::BadAnswer);
        }
        Ok(bytes)
    }
}

/// The headers that tell GitHub's rate limit, as sent.
struct RateHeaders<'a> {
    remaining: Option<&'a str>,
    /// The time the limit resets, in seconds since 1970.
    reset: Option<&'a str>,
    /// Seconds to wait.
    retry_after: Option<&'a str>,
}

/// A 403 or 429 is the rate limit when no request is left or GitHub says how long to wait. It lasts until the reset
/// time, or for the wait GitHub names, or a minute, and never over an hour. Any other status is refused with its code.
fn status_failure(status: u16, headers: &RateHeaders, now: DateTime<Utc>) -> CommunityFailure {
    let used_up = headers.remaining.is_some_and(|remaining| remaining.trim() == "0");
    let wait = headers.retry_after.and_then(|seconds| seconds.trim().parse::<i64>().ok()).and_then(TimeDelta::try_seconds);
    if !matches!(status, 403 | 429) || !used_up && wait.is_none() {
        return CommunityFailure::Refused(status);
    }
    let reset =
        headers.reset.filter(|_| used_up).and_then(|seconds| seconds.trim().parse::<i64>().ok()).and_then(|seconds| DateTime::from_timestamp(seconds, 0));
    let until = reset.or_else(|| wait.and_then(|wait| now.checked_add_signed(wait))).unwrap_or(now + DEFAULT_WAIT);
    CommunityFailure::Limited { until: until.clamp(now, now + LONGEST_WAIT) }
}

/// Jobdesk's own word for why a request got no answer. The server's text is never kept.
fn failure_of(error: &ureq::Error) -> CommunityFailure {
    match error {
        ureq::Error::Timeout(_) => CommunityFailure::TimedOut,
        ureq::Error::Io(error) => read_failure(error),
        _ => CommunityFailure::Unreachable,
    }
}

fn read_failure(error: &io::Error) -> CommunityFailure {
    if error.kind() == io::ErrorKind::TimedOut { CommunityFailure::TimedOut } else { CommunityFailure::Unreachable }
}

/// The file list GitHub gives. Every other field is never read.
#[derive(Deserialize)]
struct Tree {
    tree: Vec<TreeItem>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct TreeItem {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    sha: String,
}

/// Reads the whole list: the file list, then each `SKILL.md` and `CODEOWNERS` whose blob changed since `saved`.
/// All or nothing: any failure ends it, and the saved list stays as it was.
pub fn fetch_list(api: &dyn Api, saved: Option<&CommunityList>, now: DateTime<Utc>) -> Result<CommunityList, CommunityFailure> {
    let tree: Tree = serde_json::from_slice(&api.get(&tree_url(), JSON, TREE_LIMIT)?).map_err(|_| CommunityFailure::BadAnswer)?;
    if tree.truncated {
        return Err(CommunityFailure::BadAnswer);
    }
    let mut portals = Vec::new();
    let mut codeowners_sha = None;
    for item in tree.tree.iter().filter(|item| item.kind == "blob") {
        let sha = || BlobSha::parse(&item.sha).ok_or(CommunityFailure::BadAnswer);
        if item.path == ".github/CODEOWNERS" {
            codeowners_sha = Some(sha()?);
            continue;
        }
        let Some((country, folder)) = parse_tree_path(&item.path) else { continue };
        let sha = sha()?;
        let kept = saved.and_then(|saved| saved.portals.iter().find(|portal| portal.country == country && portal.folder == folder && portal.sha == sha));
        portals.push(match kept {
            Some(portal) => portal.clone(),
            None => ListedPortal::from_manifest(country, folder, sha.clone(), &blob_text(api, &sha)?),
        });
    }
    let codeowners = match (&codeowners_sha, saved) {
        (None, _) => Codeowners::default(),
        (Some(sha), Some(saved)) if saved.codeowners_sha.as_ref() == Some(sha) => saved.codeowners.clone(),
        (Some(sha), _) => parse_codeowners(&blob_text(api, sha)?),
    };
    Ok(CommunityList { saved_at: now, portals, codeowners, codeowners_sha })
}

fn blob_text(api: &dyn Api, sha: &BlobSha) -> Result<String, CommunityFailure> {
    Ok(String::from_utf8_lossy(&api.get(&blob_url(sha), RAW, BLOB_LIMIT)?).into_owned())
}

/// The community list on GitHub. Each read runs on a thread of its own, saves the list it read in the data folder,
/// and its answer comes back through a channel, with a repaint so the window shows it. It starts no `claude` process,
/// uses no API key and writes no run log, so neither Tasks nor Usage ever sees it.
pub struct GithubCommunity {
    api: Arc<dyn Api>,
    cache: Arc<CommunityCache>,
    repaint: Arc<dyn Fn() + Send + Sync>,
    sender: Sender<Result<CommunityList, CommunityFailure>>,
    answers: Receiver<Result<CommunityList, CommunityFailure>>,
}

impl GithubCommunity {
    pub fn new(api: Arc<dyn Api>, cache: CommunityCache, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, answers) = mpsc::channel();
        Self { api, cache: Arc::new(cache), repaint, sender, answers }
    }
}

impl CommunitySource for GithubCommunity {
    fn fetch(&mut self, saved: Option<CommunityList>) {
        let (api, cache, repaint, sender) = (Arc::clone(&self.api), Arc::clone(&self.cache), Arc::clone(&self.repaint), self.sender.clone());
        thread::spawn(move || {
            let answer = fetch_list(api.as_ref(), saved.as_ref(), Utc::now());
            if let Ok(list) = &answer {
                // The list still shows when it cannot be saved: the next launch reads it again.
                let _ = cache.save(list);
            }
            let _ = sender.send(answer);
            repaint();
        });
    }

    fn poll(&mut self) -> Option<Result<CommunityList, CommunityFailure>> {
        self.answers.try_recv().ok()
    }
}

/// A source that answers each read at once from a queue, and never answers once it is empty. For tests and snapshots,
/// so no network or disk is touched.
pub struct ScriptedCommunity {
    script: VecDeque<Result<CommunityList, CommunityFailure>>,
    sent: VecDeque<Result<CommunityList, CommunityFailure>>,
}

impl ScriptedCommunity {
    pub fn new(script: impl IntoIterator<Item = Result<CommunityList, CommunityFailure>>) -> Self {
        Self { script: script.into_iter().collect(), sent: VecDeque::new() }
    }
}

impl CommunitySource for ScriptedCommunity {
    fn fetch(&mut self, _saved: Option<CommunityList>) {
        self.sent.extend(self.script.pop_front());
    }

    fn poll(&mut self) -> Option<Result<CommunityList, CommunityFailure>> {
        self.sent.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use chrono::TimeZone;

    use super::*;
    use crate::portals::domain::{CountryCode, FolderName};

    const JOBNET_SHA: &str = "1111111111111111111111111111111111111111";
    const NORTHFIELD_SHA: &str = "2222222222222222222222222222222222222222";
    const OWNERS_SHA: &str = "3333333333333333333333333333333333333333";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap()
    }

    /// A fake GitHub: the file list and blobs it holds, the requests it saw, and a failure for any blob it lacks.
    struct Fake {
        tree: String,
        blobs: Vec<(&'static str, String)>,
        seen: Mutex<Vec<(String, String)>>,
    }

    impl Fake {
        fn new(entries: &[(&str, &str)]) -> Self {
            let items: Vec<String> =
                entries.iter().map(|(path, sha)| format!(r#"{{"path": "{path}", "mode": "100644", "type": "blob", "sha": "{sha}"}}"#)).collect();
            let tree = format!(
                r#"{{"sha": "abc", "url": "https://attacker.example/", "tree": [{{"path": "portals", "type": "tree", "sha": "x"}}, {}], "truncated": false}}"#,
                items.join(", ")
            );
            let blobs = vec![
                (JOBNET_SHA, "# Jobnet Search Skill\n\nDanish listings from Jobnet.dk.\n".to_owned()),
                (NORTHFIELD_SHA, "# North\u{200B}field Search Skill\n\nBelgian\njobs. More.\n".to_owned()),
                (OWNERS_SHA, "* @fallback-desk\n/portals/dk/jobnet-search/ @ada-quill # unmaintained since 2026-03\n".to_owned()),
            ];
            Self { tree, blobs, seen: Mutex::default() }
        }

        fn count(&self) -> usize {
            self.seen.lock().unwrap().len()
        }
    }

    impl Api for Fake {
        fn get(&self, url: &str, accept: &str, limit: u64) -> Result<Vec<u8>, CommunityFailure> {
            self.seen.lock().unwrap().push((url.to_owned(), accept.to_owned()));
            if url == tree_url() {
                assert_eq!((accept, limit), (JSON, TREE_LIMIT));
                return Ok(self.tree.clone().into_bytes());
            }
            assert_eq!((accept, limit), (RAW, BLOB_LIMIT));
            let sha = url.strip_prefix("https://api.github.com/repos/ai-job-search-dev/community-portals/git/blobs/").expect(url);
            self.blobs.iter().find(|(known, _)| *known == sha).map(|(_, text)| text.clone().into_bytes()).ok_or(CommunityFailure::Refused(404))
        }
    }

    fn full() -> Fake {
        Fake::new(&[
            (".github/CODEOWNERS", OWNERS_SHA),
            ("README.md", "4444444444444444444444444444444444444444"),
            ("portals/be/northfield-search/SKILL.md", NORTHFIELD_SHA),
            ("portals/be/northfield-search/cli/SKILL.md", "5555555555555555555555555555555555555555"),
            ("portals/dk/jobnet-search/SKILL.md", JOBNET_SHA),
            ("portals/dk/../SKILL.md", "6666666666666666666666666666666666666666"),
        ])
    }

    #[test]
    fn the_list_reads_each_portal_and_its_owners() {
        let fake = full();
        let list = fetch_list(&fake, None, now()).unwrap();
        let read: Vec<(&str, &str, Option<&str>)> =
            list.portals.iter().map(|portal| (portal.folder.as_str(), portal.name.as_str(), portal.sentence.as_deref())).collect();
        assert_eq!(read, [("northfield-search", "Northfield", Some("Belgian jobs.")), ("jobnet-search", "Jobnet", Some("Danish listings from Jobnet.dk."))]);
        let dk = CountryCode::parse("dk").unwrap();
        let ownership = list.codeowners.ownership(&dk, &FolderName::parse("jobnet-search").unwrap());
        assert_eq!(ownership.owners, ["@ada-quill"]);
        assert_eq!(list.saved_at, now());
        assert_eq!(fake.count(), 4, "the list, two SKILL.md files and CODEOWNERS");
    }

    #[test]
    fn a_second_read_with_the_same_blobs_makes_one_request() {
        let fake = full();
        let first = fetch_list(&fake, None, now()).unwrap();
        let second = fetch_list(&fake, Some(&first), now() + TimeDelta::hours(2)).unwrap();
        assert_eq!(fake.count(), 5);
        assert_eq!((second.portals.clone(), second.codeowners.clone()), (first.portals.clone(), first.codeowners.clone()));
        let changed = Fake::new(&[(".github/CODEOWNERS", OWNERS_SHA), ("portals/dk/jobnet-search/SKILL.md", NORTHFIELD_SHA)]);
        let third = fetch_list(&changed, Some(&first), now()).unwrap();
        assert_eq!(changed.count(), 2, "only the changed SKILL.md is read again");
        assert_eq!(third.portals[0].name, "Northfield");
    }

    #[test]
    fn a_missing_blob_or_a_bad_answer_fails_the_whole_read() {
        let fake =
            Fake::new(&[("portals/dk/jobnet-search/SKILL.md", JOBNET_SHA), ("portals/dk/gone-search/SKILL.md", "7777777777777777777777777777777777777777")]);
        assert_eq!(fetch_list(&fake, None, now()), Err(CommunityFailure::Refused(404)));
        let bad_sha = Fake::new(&[("portals/dk/jobnet-search/SKILL.md", "../../../../users/x")]);
        assert_eq!(fetch_list(&bad_sha, None, now()), Err(CommunityFailure::BadAnswer));
        assert_eq!(bad_sha.count(), 1, "a bad blob name is never requested");
        let mut truncated = full();
        truncated.tree = truncated.tree.replace(r#""truncated": false"#, r#""truncated": true"#);
        assert_eq!(fetch_list(&truncated, None, now()), Err(CommunityFailure::BadAnswer));
        let mut garbage = full();
        garbage.tree = "<html>".to_owned();
        assert_eq!(fetch_list(&garbage, None, now()), Err(CommunityFailure::BadAnswer));
    }

    #[test]
    fn a_repository_with_no_codeowners_lists_no_owners() {
        let fake = Fake::new(&[("portals/dk/jobnet-search/SKILL.md", JOBNET_SHA)]);
        let list = fetch_list(&fake, None, now()).unwrap();
        assert_eq!((list.codeowners, list.codeowners_sha), (Codeowners::default(), None));
    }

    fn headers<'a>(remaining: Option<&'a str>, reset: Option<&'a str>, retry_after: Option<&'a str>) -> RateHeaders<'a> {
        RateHeaders { remaining, reset, retry_after }
    }

    #[test]
    fn a_used_up_limit_lasts_until_its_reset() {
        let reset = (now() + TimeDelta::minutes(20)).timestamp().to_string();
        let limited = |status| status_failure(status, &headers(Some("0"), Some(&reset), None), now());
        assert_eq!(limited(403), CommunityFailure::Limited { until: now() + TimeDelta::minutes(20) });
        assert_eq!(limited(429), CommunityFailure::Limited { until: now() + TimeDelta::minutes(20) });
        assert_eq!(limited(500), CommunityFailure::Refused(500), "only 403 and 429 are a limit");
        let retry = status_failure(429, &headers(Some("12"), Some(&reset), Some("90")), now());
        assert_eq!(retry, CommunityFailure::Limited { until: now() + TimeDelta::seconds(90) }, "a secondary limit names its wait");
        let no_time = status_failure(403, &headers(Some("0"), Some("soon"), None), now());
        assert_eq!(no_time, CommunityFailure::Limited { until: now() + TimeDelta::minutes(1) });
        let far = (now() + TimeDelta::days(30)).timestamp().to_string();
        assert_eq!(status_failure(403, &headers(Some("0"), Some(&far), None), now()), CommunityFailure::Limited { until: now() + TimeDelta::hours(1) });
        let past = (now() - TimeDelta::hours(3)).timestamp().to_string();
        assert_eq!(status_failure(403, &headers(Some("0"), Some(&past), None), now()), CommunityFailure::Limited { until: now() });
    }

    #[test]
    fn a_refusal_that_is_no_limit_keeps_only_its_code() {
        assert_eq!(status_failure(403, &headers(Some("59"), None, None), now()), CommunityFailure::Refused(403));
        assert_eq!(status_failure(403, &headers(None, None, None), now()), CommunityFailure::Refused(403));
        assert_eq!(status_failure(404, &headers(Some("0"), Some("1"), Some("5")), now()), CommunityFailure::Refused(404));
        assert_eq!(failure_of(&ureq::Error::HostNotFound), CommunityFailure::Unreachable);
        assert_eq!(failure_of(&ureq::Error::Timeout(ureq::Timeout::Global)), CommunityFailure::TimedOut);
        assert_eq!(failure_of(&ureq::Error::Io(io::Error::new(io::ErrorKind::TimedOut, "slow"))), CommunityFailure::TimedOut);
        assert_eq!(failure_of(&ureq::Error::RequireHttpsOnly("http://example.com".into())), CommunityFailure::Unreachable);
    }

    #[test]
    fn a_plain_http_address_is_never_requested() {
        assert_eq!(HttpsApi::new("0.1.0").get("http://127.0.0.1:9/", JSON, 10), Err(CommunityFailure::Unreachable));
    }

    #[test]
    fn a_read_runs_off_the_window_thread_and_saves_the_list() {
        let data = tempfile::tempdir().unwrap();
        let repaints = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&repaints);
        let repaint = Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        let mut source = GithubCommunity::new(Arc::new(full()), CommunityCache::in_data(data.path()), repaint);
        source.fetch(None);
        let deadline = Instant::now() + Duration::from_secs(10);
        let answer = loop {
            if let Some(answer) = source.poll() {
                break answer;
            }
            assert!(Instant::now() < deadline, "no answer");
            thread::yield_now();
        };
        let list = answer.unwrap();
        assert_eq!(CommunityCache::in_data(data.path()).load().map(|saved| saved.portals), Some(list.portals));
        while repaints.load(Ordering::SeqCst) < 1 && Instant::now() < deadline {
            thread::yield_now();
        }
        assert_eq!(repaints.load(Ordering::SeqCst), 1);
    }
}
