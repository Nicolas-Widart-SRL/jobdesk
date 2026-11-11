use std::io::Read;

use chrono::DateTime;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::shared_kernel::https::{Fetch, Transport, read_failure};
use crate::updates::domain::{COMPATIBLE_LIMIT, FEED_URL, Failure, Platform, Release, SignedPin, SourcePin, Version, source_zip_name};
use crate::updates::infrastructure::signature::{UpdateKey, signs};

/// The most of the feed that is read. A longer one is refused whole.
pub const FEED_LIMIT: u64 = 256 * 1024;
/// The most of the release notes that is kept. Longer notes are cut at a character.
pub const NOTES_LIMIT: usize = 64 * 1024;

/// `latest.json`, as `scripts/update-feed.sh` writes it. Any other field, such as an address, is never read.
/// The `ai-job-search` pins are read on their own, by `parse_pins`, so a pin that does not read never spoils the update.
#[derive(Deserialize)]
struct Feed {
    version: String,
    #[serde(default)]
    pub_date: Option<String>,
    #[serde(default)]
    notes: String,
    platforms: Map<String, Value>,
}

/// One build in the feed: its signature and its size, and no address.
#[derive(Deserialize)]
struct Build {
    signature: String,
    size: u64,
}

/// The feed's `ai_job_search_releases` list and `ai_job_search` field, kept as they are until `Pin` reads each entry.
#[derive(Deserialize)]
struct Pinned {
    #[serde(default)]
    ai_job_search_releases: Option<Value>,
    #[serde(default)]
    ai_job_search: Option<Value>,
}

/// One `ai-job-search` release the feed pins: its version, the SHA-256 of its source zip, and the update key's
/// signature of the two. It holds no address.
#[derive(Deserialize)]
struct Pin {
    version: String,
    sha256: String,
    signature: String,
}

/// Reads the feed from GitHub once and gives the release it names, for `platform`'s build, with the `ai-job-search`
/// releases the same bytes pin when `key` signed them, newest first. With no `key`, no pin is read. A pin that does
/// not check never spoils the release.
pub fn read_feed(transport: &dyn Transport, platform: Option<Platform>, key: Option<&UpdateKey>) -> Result<Release, Failure> {
    let bytes = fetch_feed(transport)?;
    let release = parse_feed(&bytes, platform)?;
    let pins = key.map(|key| parse_pins(&bytes, key)).unwrap_or_default();
    Ok(Release { pins: pins.into_iter().map(SignedPin::checked).collect(), ..release })
}

/// Reads the feed from GitHub and gives the newest `ai-job-search` release it pins, when `key` signed it.
pub fn read_pin(transport: &dyn Transport, key: &UpdateKey) -> Result<Option<SourcePin>, Failure> {
    Ok(parse_pin(&fetch_feed(transport)?, key))
}

/// The feed's bytes, or why they could not be read. A feed over `FEED_LIMIT` is `BadFeed`.
fn fetch_feed(transport: &dyn Transport) -> Result<Vec<u8>, Failure> {
    let body = transport.open(FEED_URL, Fetch::Feed)?;
    let mut bytes = Vec::new();
    body.take(FEED_LIMIT + 1).read_to_end(&mut bytes).map_err(|error| read_failure(&error))?;
    if bytes.len() as u64 > FEED_LIMIT {
        return Err(Failure::BadFeed);
    }
    Ok(bytes)
}

/// A feed whose version or date cannot be read is `BadFeed`, and one with no build for `platform` is `NoBuild`. The
/// release it gives has no pins: `read_feed` adds them.
pub fn parse_feed(bytes: &[u8], platform: Option<Platform>) -> Result<Release, Failure> {
    let feed: Feed = serde_json::from_slice(bytes).map_err(|_| Failure::BadFeed)?;
    let version = Version::parse(&feed.version).ok_or(Failure::BadFeed)?;
    let released = match feed.pub_date {
        None => None,
        Some(date) => Some(DateTime::parse_from_rfc3339(&date).map_err(|_| Failure::BadFeed)?.date_naive()),
    };
    let platform = platform.ok_or(Failure::NoBuild)?;
    let build = feed.platforms.get(platform.feed_key()).ok_or(Failure::NoBuild)?;
    let build = Build::deserialize(build).map_err(|_| Failure::BadFeed)?;
    if build.size == 0 || build.signature.trim().is_empty() {
        return Err(Failure::BadFeed);
    }
    Ok(Release { version, released, notes: cut(feed.notes), size: build.size, signature: build.signature, pins: Vec::new() })
}

/// The newest pin in the feed, once `key` signed it: the first of `parse_pins`.
pub fn parse_pin(bytes: &[u8], key: &UpdateKey) -> Option<SourcePin> {
    parse_pins(bytes, key).into_iter().next()
}

/// Every pin in the feed that `key` signed, newest version first. It reads at most the first `COMPATIBLE_LIMIT` items
/// of `ai_job_search_releases` (a field that is not an array is ignored), then the legacy `ai_job_search` field.
///
/// Each entry is checked alone: Jobdesk rebuilds the signed text from its version and hash, and the signature's trusted
/// comment must name the zip of that version, so a signature of another version, another hash or with another key
/// never passes. An entry that does not read or whose signature fails is dropped, and only itself. A version listed
/// twice counts once, the first seen, the list before the legacy field. The feed is not signed, so its order means
/// nothing: the result is sorted by version, and of more than `COMPATIBLE_LIMIT` signed releases only the newest are kept.
pub fn parse_pins(bytes: &[u8], key: &UpdateKey) -> Vec<SourcePin> {
    let Ok(pinned) = serde_json::from_slice::<Pinned>(bytes) else { return Vec::new() };
    let listed = match pinned.ai_job_search_releases {
        Some(Value::Array(items)) => items,
        _ => Vec::new(),
    };
    let mut found: Vec<SourcePin> = Vec::new();
    for entry in listed.into_iter().take(COMPATIBLE_LIMIT).chain(pinned.ai_job_search) {
        if let Some(pin) = signed_entry(entry, key)
            && !found.iter().any(|seen| seen.version == pin.version)
        {
            found.push(pin);
        }
    }
    newest_first(found)
}

/// `pins`, which are already checked and hold no version twice, newest first and no more than `COMPATIBLE_LIMIT`. The
/// list is read up to the limit and the old field comes after it, so up to one more than the limit can arrive here.
fn newest_first(mut pins: Vec<SourcePin>) -> Vec<SourcePin> {
    pins.sort_by_key(|pin| std::cmp::Reverse(pin.version));
    pins.truncate(COMPATIBLE_LIMIT);
    pins
}

/// One entry of the feed, when it reads and `key` signed it for its own zip.
fn signed_entry(entry: Value, key: &UpdateKey) -> Option<SourcePin> {
    let pin = Pin::deserialize(entry).ok()?;
    let found = SourcePin::parse(&pin.version, &pin.sha256)?;
    signs(key, found.signed_text().as_bytes(), &pin.signature, &source_zip_name(&found.version)).then_some(found)
}

fn cut(mut notes: String) -> String {
    if notes.len() > NOTES_LIMIT {
        let mut end = NOTES_LIMIT;
        while !notes.is_char_boundary(end) {
            end -= 1;
        }
        notes.truncate(end);
    }
    notes
}

#[cfg(test)]
pub mod tests {
    use std::fs;
    use std::io::{self, Cursor};

    use chrono::NaiveDate;

    use super::*;
    use crate::shared_kernel::https::{Body, HttpsFailure};
    use crate::updates::domain::source_zip_url;
    use crate::updates::infrastructure::signature::tests::{fixture, fixture_key, fixture_signature};

    fn mac() -> Option<Platform> {
        Some(Platform::MacArm)
    }

    fn feed(platforms: &str) -> String {
        format!(r#"{{"version": "0.2.0", "pub_date": "2026-10-30T09:00:00Z", "notes": "Notes", "platforms": {platforms}}}"#)
    }

    /// The pin in the fixture feed: the hash in `ai-job-search-v1.4.0.zip.sha256`, which the fixture key signed.
    pub fn fixture_pin() -> SourcePin {
        let line = fs::read_to_string(fixture("ai-job-search-v1.4.0.zip.sha256")).unwrap();
        let (sha256, name) = line.trim_end().split_once("  ").unwrap();
        assert_eq!(name, "ai-job-search-v1.4.0.zip");
        SourcePin::parse("1.4.0", sha256).unwrap()
    }

    /// The release 1.3.0 in the fixture feed: the same hash as 1.4.0, which the fixture key signed under its own name.
    pub fn fixture_pin_130() -> SourcePin {
        SourcePin::parse("1.3.0", &fixture_pin().sha256).unwrap()
    }

    /// The pins in the fixture feed, newest first.
    pub fn fixture_pins() -> Vec<SourcePin> {
        vec![fixture_pin(), fixture_pin_130()]
    }

    /// The fixture feed with its `ai_job_search_releases` list set to `list` and its `ai_job_search` field set to
    /// `legacy`, each removed when `None`.
    fn with_fields(list: Option<Value>, legacy: Option<Value>) -> Vec<u8> {
        let mut feed: Value = serde_json::from_slice(&fs::read(fixture("latest.json")).unwrap()).unwrap();
        let fields = feed.as_object_mut().unwrap();
        for (name, value) in [("ai_job_search_releases", list), ("ai_job_search", legacy)] {
            match value {
                Some(value) => fields.insert(name.to_owned(), value),
                None => fields.remove(name),
            };
        }
        serde_json::to_vec(&feed).unwrap()
    }

    /// The fixture feed with only its `ai_job_search` field, set to `pin`, or with no pin at all when `pin` is `None`.
    fn with_pin(pin: Option<Value>) -> Vec<u8> {
        with_fields(None, pin)
    }

    /// The fixture feed with only the `ai_job_search_releases` list, set to `entries`.
    fn with_list(entries: Vec<Value>) -> Vec<u8> {
        with_fields(Some(Value::Array(entries)), None)
    }

    fn pin(version: &str, sha256: &str, signature: &str) -> Value {
        serde_json::json!({"version": version, "sha256": sha256, "signature": signature})
    }

    /// The signed entry of the fixture for 1.4.0 and 1.3.0.
    fn good_140() -> Value {
        pin("1.4.0", &fixture_pin().sha256, &fixture_signature("ai-job-search-v1.4.0.zip.sha256"))
    }

    fn good_130() -> Value {
        pin("1.3.0", &fixture_pin().sha256, &fixture_signature("ai-job-search-v1.3.0.zip.sha256"))
    }

    fn versions(pins: &[SourcePin]) -> Vec<String> {
        pins.iter().map(|pin| pin.version.to_string()).collect()
    }

    struct Serves(Vec<u8>);

    impl Transport for Serves {
        fn open(&self, url: &str, fetch: Fetch) -> Result<Body, HttpsFailure> {
            assert_eq!((url, fetch), (FEED_URL, Fetch::Feed));
            Ok(Box::new(Cursor::new(self.0.clone())))
        }
    }

    #[test]
    fn the_feed_the_script_writes_reads_for_each_build() {
        let bytes = fs::read(fixture("latest.json")).unwrap();
        for (platform, asset) in [(Platform::MacArm, "Jobdesk-v0.2.0-macos-arm64.app.tar.gz"), (Platform::WindowsX64, "Jobdesk-v0.2.0-windows-x64-setup.exe")] {
            let release = parse_feed(&bytes, Some(platform)).unwrap();
            assert_eq!(release.version, Version { major: 0, minor: 2, patch: 0 });
            assert_eq!(release.released, NaiveDate::from_ymd_opt(2026, 10, 30));
            assert_eq!(release.notes, fs::read_to_string(fixture("notes.md")).unwrap());
            assert_eq!(release.size, fs::metadata(fixture(asset)).unwrap().len());
            assert_eq!(release.signature, fixture_signature(asset).trim());
        }
    }

    /// The fixture is what the script writes today, so the script and the app read the feed the same way.
    #[cfg(unix)]
    #[test]
    fn the_script_still_writes_the_fixture() {
        use std::path::Path;
        use std::process::Command;
        if Command::new("jq").arg("--version").output().is_err() {
            return;
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let output = Command::new("bash")
            .arg(root.join("scripts/update-feed.sh"))
            .args(["v0.2.0", "notes.md", "."])
            .current_dir(fixture(""))
            .env("PUB_DATE", "2026-10-30T09:00:00Z")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8(output.stdout).unwrap(), fs::read_to_string(fixture("latest.json")).unwrap());
    }

    #[test]
    fn a_feed_with_no_build_for_this_computer_is_no_build() {
        let only_linux = feed(r#"{"linux-x86_64": {"signature": "sig", "size": 10}}"#);
        assert_eq!(parse_feed(only_linux.as_bytes(), mac()), Err(Failure::NoBuild));
        assert_eq!(parse_feed(feed(r#"{"darwin-aarch64": {"signature": "sig", "size": 10}}"#).as_bytes(), None), Err(Failure::NoBuild));
    }

    #[test]
    fn a_bad_version_date_or_build_is_a_bad_feed() {
        let good = r#"{"darwin-aarch64": {"signature": "sig", "size": 10}}"#;
        for bad in [
            feed(good).replace("0.2.0", "0.2"),
            feed(good).replace("0.2.0", "0.2.0-beta"),
            feed(good).replace("2026-10-30T09:00:00Z", "30 Oct 2026"),
            feed(r#"{"darwin-aarch64": {"signature": "sig"}}"#),
            feed(r#"{"darwin-aarch64": {"signature": "sig", "size": -1}}"#),
            feed(r#"{"darwin-aarch64": {"signature": "sig", "size": 0}}"#),
            feed(r#"{"darwin-aarch64": {"signature": " ", "size": 10}}"#),
            feed(r#"{"darwin-aarch64": "sig"}"#),
            "not json".to_owned(),
            r#"{"version": "0.2.0"}"#.to_owned(),
        ] {
            assert_eq!(parse_feed(bad.as_bytes(), mac()), Err(Failure::BadFeed), "{bad}");
        }
    }

    #[test]
    fn a_feed_with_no_date_or_notes_still_reads() {
        let bare = r#"{"version": "v0.3.1", "platforms": {"darwin-aarch64": {"signature": "sig", "size": 10}}}"#;
        let release = parse_feed(bare.as_bytes(), mac()).unwrap();
        assert_eq!((release.version.to_string(), release.released, release.notes.as_str()), ("0.3.1".to_owned(), None, ""));
    }

    #[test]
    fn an_address_in_the_feed_is_never_read() {
        let plain = parse_feed(feed(r#"{"darwin-aarch64": {"signature": "sig", "size": 10}}"#).as_bytes(), mac()).unwrap();
        let with_url = feed(r#"{"darwin-aarch64": {"signature": "sig", "size": 10, "url": "https://attacker.example/Jobdesk.tar.gz"}}"#).replacen(
            '{',
            r#"{"url": "https://attacker.example/","#,
            1,
        );
        assert_eq!(parse_feed(with_url.as_bytes(), mac()), Ok(plain));
    }

    #[test]
    fn a_build_the_other_computer_reads_differently_does_not_spoil_this_one() {
        let mixed = feed(r#"{"windows-x86_64": "not a build", "darwin-aarch64": {"signature": "sig", "size": 10}}"#);
        assert_eq!(parse_feed(mixed.as_bytes(), mac()).map(|release| release.size), Ok(10));
    }

    #[test]
    fn long_notes_are_cut_at_a_character() {
        let notes = "é".repeat(NOTES_LIMIT);
        let long = serde_json::json!({"version": "0.2.0", "notes": notes, "platforms": {"darwin-aarch64": {"signature": "sig", "size": 10}}});
        let release = parse_feed(long.to_string().as_bytes(), mac()).unwrap();
        assert_eq!(release.notes.len(), NOTES_LIMIT);
        assert!(release.notes.chars().all(|c| c == 'é'));
    }

    #[test]
    fn an_oversized_feed_is_refused_whole() {
        let padding = " ".repeat(FEED_LIMIT as usize);
        let oversized = feed(r#"{"darwin-aarch64": {"signature": "sig", "size": 10}}"#).replacen('{', &format!("{{{padding}"), 1);
        assert_eq!(read_feed(&Serves(oversized.clone().into_bytes()), mac(), Some(&fixture_key())), Err(Failure::BadFeed));
        assert_eq!(read_pin(&Serves(oversized.into_bytes()), &fixture_key()), Err(Failure::BadFeed));
        let fits = fs::read(fixture("latest.json")).unwrap();
        assert!(read_feed(&Serves(fits), mac(), Some(&fixture_key())).is_ok());
    }

    #[test]
    fn a_feed_that_cannot_be_reached_says_why() {
        struct Down;
        impl Transport for Down {
            fn open(&self, _url: &str, _fetch: Fetch) -> Result<Body, HttpsFailure> {
                Err(HttpsFailure::Refused(503))
            }
        }
        struct Drops;
        impl Transport for Drops {
            fn open(&self, _url: &str, _fetch: Fetch) -> Result<Body, HttpsFailure> {
                Ok(Box::new(Cursor::new(b"{\"version\"".to_vec()).chain(Failing)))
            }
        }
        struct Failing;
        impl Read for Failing {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::TimedOut, "slow"))
            }
        }
        assert_eq!(read_feed(&Down, mac(), Some(&fixture_key())), Err(Failure::Refused(503)));
        assert_eq!(read_feed(&Drops, mac(), Some(&fixture_key())), Err(Failure::TimedOut));
        assert_eq!(read_pin(&Down, &fixture_key()), Err(Failure::Refused(503)));
        assert_eq!(read_pin(&Drops, &fixture_key()), Err(Failure::TimedOut));
    }

    #[test]
    fn a_good_pin_reads() {
        let bytes = fs::read(fixture("latest.json")).unwrap();
        assert_eq!(parse_pin(&bytes, &fixture_key()), Some(fixture_pin()));
        assert_eq!(read_pin(&Serves(bytes), &fixture_key()), Ok(Some(fixture_pin())));
    }

    /// Serves `bytes` as the feed and counts the requests.
    struct Counts(Vec<u8>, std::sync::atomic::AtomicUsize);

    impl Transport for Counts {
        fn open(&self, url: &str, fetch: Fetch) -> Result<Body, HttpsFailure> {
            assert_eq!((url, fetch), (FEED_URL, Fetch::Feed));
            self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Box::new(Cursor::new(self.0.clone())))
        }
    }

    #[test]
    fn the_check_carries_the_pins_of_the_same_read() {
        let feed = Counts(fs::read(fixture("latest.json")).unwrap(), Default::default());
        let release = read_feed(&feed, mac(), Some(&fixture_key())).unwrap();
        assert_eq!(release.pins.iter().map(SignedPin::pin).cloned().collect::<Vec<_>>(), fixture_pins());
        assert_eq!(Release { pins: Vec::new(), ..release }, parse_feed(&feed.0, mac()).unwrap(), "the release is what it was without the pins");
        assert_eq!(feed.1.load(std::sync::atomic::Ordering::SeqCst), 1, "one request gives both");
    }

    #[test]
    fn a_check_with_no_key_or_a_bad_pin_carries_no_pin_and_the_same_release() {
        let feed = Counts(fs::read(fixture("latest.json")).unwrap(), Default::default());
        let plain = parse_feed(&feed.0, mac()).unwrap();
        assert_eq!(read_feed(&feed, mac(), None), Ok(plain.clone()), "a build with no update key reads no pin");
        let other = UpdateKey::decode(&fs::read_to_string(fixture("other.pub")).unwrap()).unwrap();
        assert_eq!(read_feed(&feed, mac(), Some(&other)), Ok(plain.clone()), "a pin another key signed");
        let unpinned = Counts(with_pin(None), Default::default());
        assert_eq!(read_feed(&unpinned, mac(), Some(&fixture_key())), Ok(plain), "no pin in the feed");
    }

    #[test]
    fn a_pin_signed_for_another_version_is_dropped() {
        let older = fixture_signature("ai-job-search-v1.3.0.zip.sha256");
        let sha256 = fixture_pin().sha256;
        assert!(parse_pin(&with_pin(Some(pin("1.3.0", &sha256, &older))), &fixture_key()).is_some(), "the signature itself is good");
        assert_eq!(parse_pin(&with_pin(Some(pin("1.4.0", &sha256, &older))), &fixture_key()), None, "but it pins 1.3.0, not 1.4.0");
        let current = fixture_signature("ai-job-search-v1.4.0.zip.sha256");
        assert_eq!(parse_pin(&with_pin(Some(pin("1.4.1", &sha256, &current))), &fixture_key()), None);
    }

    #[test]
    fn a_pin_signed_for_another_hash_is_dropped() {
        let signature = fixture_signature("ai-job-search-v1.4.0.zip.sha256");
        let other = fixture_pin().sha256.replacen('6', "7", 1);
        assert_eq!(parse_pin(&with_pin(Some(pin("1.4.0", &other, &signature))), &fixture_key()), None);
    }

    #[test]
    fn a_pin_signed_with_another_key_is_dropped() {
        let other = UpdateKey::decode(&fs::read_to_string(fixture("other.pub")).unwrap()).unwrap();
        assert_eq!(parse_pin(&fs::read(fixture("latest.json")).unwrap(), &other), None);
    }

    #[test]
    fn a_bad_pin_leaves_the_update_intact() {
        let plain = with_pin(None);
        assert_eq!(parse_pin(&plain, &fixture_key()), None, "no pin");
        let sha256 = fixture_pin().sha256;
        let signature = fixture_signature("ai-job-search-v1.4.0.zip.sha256");
        for bad in [
            serde_json::json!("1.4.0"),
            serde_json::json!(null),
            serde_json::json!({"version": "1.4.0", "sha256": sha256}),
            pin("1.4.0", &sha256, ""),
            pin("1.4.0", &sha256, "not a signature"),
            pin("1.4.0", &sha256, &fixture_signature("Jobdesk-v0.2.0-windows-x64-setup.exe")),
            pin("1.4.0", &sha256.to_uppercase(), &signature),
            pin("1.4", &sha256, &signature),
            serde_json::json!({"version": 140, "sha256": sha256, "signature": signature}),
        ] {
            let bytes = with_pin(Some(bad.clone()));
            assert_eq!(parse_pin(&bytes, &fixture_key()), None, "{bad}");
            for platform in [Platform::MacArm, Platform::WindowsX64] {
                assert_eq!(parse_feed(&bytes, Some(platform)), parse_feed(&plain, Some(platform)), "{bad}");
                assert!(parse_feed(&bytes, Some(platform)).is_ok());
            }
        }
    }

    #[test]
    fn an_address_beside_the_pin_is_never_read() {
        let mut lured: Value = serde_json::from_slice(&fs::read(fixture("latest.json")).unwrap()).unwrap();
        let mut lure = |entry: &mut Value| {
            let fields = entry.as_object_mut().unwrap();
            fields.insert("url".to_owned(), Value::from("https://attacker.example/ai-job-search.zip"));
            fields.insert("repository".to_owned(), Value::from("attacker/ai-job-search"));
        };
        lure(&mut lured["ai_job_search"]);
        lured["ai_job_search_releases"].as_array_mut().unwrap().iter_mut().for_each(&mut lure);
        let found = parse_pins(&serde_json::to_vec(&lured).unwrap(), &fixture_key());
        assert_eq!(found, fixture_pins());
        assert_eq!(source_zip_url(&found[0].version), "https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/v1.4.0");
        assert_eq!(source_zip_url(&found[1].version), "https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/v1.3.0");
    }

    #[test]
    fn the_feed_lists_every_signed_release_newest_first() {
        let bytes = fs::read(fixture("latest.json")).unwrap();
        assert_eq!(parse_pins(&bytes, &fixture_key()), fixture_pins());
        assert_eq!(versions(&parse_pins(&bytes, &fixture_key())), ["1.4.0", "1.3.0"]);
        assert_eq!(parse_pin(&bytes, &fixture_key()), Some(fixture_pin()), "the newest is the one setup and the folder update use");
        let release = read_feed(&Serves(bytes), mac(), Some(&fixture_key())).unwrap();
        assert_eq!(release.pins.iter().map(SignedPin::pin).cloned().collect::<Vec<_>>(), fixture_pins());
    }

    #[test]
    fn a_bad_entry_drops_only_itself() {
        let sha256 = fixture_pin().sha256;
        let signature = fixture_signature("ai-job-search-v1.4.0.zip.sha256");
        let plain = with_pin(None);
        for bad in [
            pin("1.4.0", &sha256, &fixture_signature("ai-job-search-v1.3.0.zip.sha256")),
            serde_json::json!("1.4.0"),
            serde_json::json!(null),
            serde_json::json!({"version": "1.4.0", "sha256": sha256}),
            pin("1.4.0", &sha256.to_uppercase(), &signature),
            pin("1.4", &sha256, &signature),
        ] {
            // The bad entry comes first, even before a good entry of its own version: it must neither spoil the others
            // nor take their place.
            let bytes = with_list(vec![bad.clone(), good_140(), good_130()]);
            assert_eq!(parse_pins(&bytes, &fixture_key()), fixture_pins(), "{bad}");
            let bytes = with_list(vec![good_140(), bad.clone(), good_130()]);
            assert_eq!(parse_pins(&bytes, &fixture_key()), fixture_pins(), "{bad}");
            let bytes = with_list(vec![good_130(), bad.clone()]);
            assert_eq!(parse_pins(&bytes, &fixture_key()), vec![fixture_pin_130()], "{bad}");
            for platform in [Platform::MacArm, Platform::WindowsX64] {
                assert_eq!(parse_feed(&bytes, Some(platform)), parse_feed(&plain, Some(platform)), "{bad}");
                assert!(parse_feed(&bytes, Some(platform)).is_ok());
            }
        }
    }

    #[test]
    fn an_entry_signed_with_another_key_is_dropped() {
        let other = UpdateKey::decode(&fs::read_to_string(fixture("other.pub")).unwrap()).unwrap();
        let bytes = fs::read(fixture("latest.json")).unwrap();
        assert_eq!(parse_pins(&bytes, &other), Vec::new());
        assert_eq!(read_feed(&Serves(bytes), mac(), Some(&other)).unwrap().pins, Vec::new());
    }

    #[test]
    fn only_the_first_five_entries_are_read() {
        let junk = || serde_json::json!("not an entry");
        let past_the_limit = with_list(vec![junk(), junk(), junk(), junk(), junk(), good_130()]);
        assert_eq!(parse_pins(&past_the_limit, &fixture_key()), Vec::new(), "the sixth is never read, and there is no old field");
        assert_eq!(parse_pin(&past_the_limit, &fixture_key()), None);
        let within = with_list(vec![junk(), junk(), junk(), junk(), good_130()]);
        assert_eq!(parse_pins(&within, &fixture_key()), vec![fixture_pin_130()], "the fifth is");
    }

    #[test]
    fn never_more_than_five_pins_are_kept_and_they_are_the_newest() {
        // Only 1.4.0 and 1.3.0 have signatures, so the cut is tested on pins already checked: five listed and one more
        // from the old field arrive as six.
        let pin = |version: &str| SourcePin::parse(version, &fixture_pin().sha256).unwrap();
        let six = ["1.2.0", "1.6.0", "1.3.0", "1.5.0", "1.4.0", "1.7.0"].map(pin).to_vec();
        assert_eq!(versions(&newest_first(six)), ["1.7.0", "1.6.0", "1.5.0", "1.4.0", "1.3.0"], "the oldest is the one left out");
        assert_eq!(versions(&newest_first(vec![pin("1.3.0"), pin("1.4.0")])), ["1.4.0", "1.3.0"], "fewer are all kept");
        assert_eq!(newest_first(Vec::new()), Vec::new());
        let signed_pair = with_fields(Some(serde_json::json!([good_130()])), Some(good_140()));
        assert_eq!(
            parse_pins(&signed_pair, &fixture_key()),
            fixture_pins(),
            "parse_pins gives its pins newest first, the old field's 1.4.0 above the list's 1.3.0"
        );
    }

    #[test]
    fn a_feed_with_only_the_old_field_still_pins() {
        let bytes = with_pin(Some(good_140()));
        assert_eq!(parse_pins(&bytes, &fixture_key()), vec![fixture_pin()]);
        assert_eq!(parse_pin(&bytes, &fixture_key()), Some(fixture_pin()));
        assert_eq!(read_pin(&Serves(bytes), &fixture_key()), Ok(Some(fixture_pin())));
    }

    #[test]
    fn a_feed_with_only_the_list_pins_its_newest() {
        let bytes = with_list(vec![good_140(), good_130()]);
        assert_eq!(parse_pins(&bytes, &fixture_key()), fixture_pins());
        assert_eq!(parse_pin(&bytes, &fixture_key()), Some(fixture_pin()));
        assert_eq!(read_pin(&Serves(bytes), &fixture_key()), Ok(Some(fixture_pin())));
    }

    #[test]
    fn the_newest_signed_entry_drives_whatever_the_order() {
        let reversed = with_list(vec![good_130(), good_140()]);
        assert_eq!(parse_pins(&reversed, &fixture_key()), fixture_pins());
        assert_eq!(parse_pin(&reversed, &fixture_key()), Some(fixture_pin()));
        let release = read_feed(&Serves(reversed), mac(), Some(&fixture_key())).unwrap();
        assert_eq!(release.pins.first().map(SignedPin::pin), Some(&fixture_pin()));
        let older_on_top = with_fields(Some(serde_json::json!([good_130()])), Some(good_140()));
        assert_eq!(parse_pins(&older_on_top, &fixture_key()), fixture_pins(), "the old field counts with the list");
    }

    #[test]
    fn a_version_listed_twice_counts_once() {
        let twice = with_list(vec![good_130(), good_140(), good_130(), good_140()]);
        assert_eq!(parse_pins(&twice, &fixture_key()), fixture_pins());
        let also_old = with_fields(Some(serde_json::json!([good_140()])), Some(good_140()));
        assert_eq!(parse_pins(&also_old, &fixture_key()), vec![fixture_pin()], "the old field repeats the list's newest");
        // The same version spelled with a v is the same version.
        let spelled = with_list(vec![pin("v1.4.0", &fixture_pin().sha256, &fixture_signature("ai-job-search-v1.4.0.zip.sha256")), good_140()]);
        assert_eq!(parse_pins(&spelled, &fixture_key()), vec![fixture_pin()]);
    }

    #[test]
    fn a_list_that_is_not_an_array_falls_back_to_the_old_field() {
        for not_a_list in [serde_json::json!("1.4.0"), serde_json::json!(null), serde_json::json!(7), good_130()] {
            let bytes = with_fields(Some(not_a_list.clone()), Some(good_140()));
            assert_eq!(parse_pins(&bytes, &fixture_key()), vec![fixture_pin()], "{not_a_list}");
            let release = read_feed(&Serves(bytes), mac(), Some(&fixture_key())).unwrap();
            assert_eq!(release.pins.first().map(SignedPin::pin), Some(&fixture_pin()), "{not_a_list}");
        }
    }

    /// Copies the stand-in builds and the 1.4.0 pin's signature into a folder, and writes one pin file for each of
    /// `versions`, each holding the one line the app rebuilds.
    #[cfg(unix)]
    fn pins_dir(versions: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "Jobdesk-v0.2.0-macos-arm64.app.tar.gz",
            "Jobdesk-v0.2.0-macos-arm64.app.tar.gz.sig",
            "Jobdesk-v0.2.0-windows-x64-setup.exe",
            "Jobdesk-v0.2.0-windows-x64-setup.exe.sig",
        ] {
            fs::copy(fixture(name), dir.path().join(name)).unwrap();
        }
        for version in versions {
            let name = format!("ai-job-search-v{version}.zip");
            fs::write(dir.path().join(format!("{name}.sha256")), format!("{}  {name}\n", fixture_pin().sha256)).unwrap();
            fs::write(dir.path().join(format!("{name}.sha256.sig")), fixture_signature("ai-job-search-v1.4.0.zip.sha256")).unwrap();
        }
        dir
    }

    #[cfg(unix)]
    fn run_script(dir: &std::path::Path) -> std::process::Output {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        std::process::Command::new("bash")
            .arg(root.join("scripts/update-feed.sh"))
            .arg("v0.2.0")
            .arg(fixture("notes.md"))
            .arg(dir)
            .env("PUB_DATE", "2026-10-30T09:00:00Z")
            .output()
            .unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn the_script_refuses_no_pin_or_more_than_five() {
        for (versions, count) in [(vec![], 0), (vec!["1.0.0", "1.1.0", "1.2.0", "1.3.0", "1.4.0", "1.5.0"], 6)] {
            let output = run_script(pins_dir(&versions).path());
            assert!(!output.status.success(), "{count} pins");
            let said = String::from_utf8_lossy(&output.stderr);
            assert!(said.contains(&format!("holds {count} ai-job-search-v*.zip.sha256 files, not 1 to 5")), "{said}");
            assert!(output.stdout.is_empty(), "no feed is written");
        }
    }

    /// One bad pin file among good ones refuses the whole feed: no `latest.json` is written. The bad file is 1.5.0, so it
    /// comes after the good 1.4.0 in the order the script reads them.
    #[cfg(unix)]
    #[test]
    fn the_script_refuses_the_feed_for_one_bad_pin_among_good_ones() {
        let sha256 = fixture_pin().sha256;
        let good = format!("{sha256}  ai-job-search-v1.5.0.zip\n");
        for (what, content) in [
            ("a second line", Some(format!("{good}{good}"))),
            ("a line without its newline", Some(good.trim_end().to_owned())),
            ("another zip's name", Some(format!("{sha256}  ai-job-search-v1.4.0.zip\n"))),
            ("an uppercase hash", Some(good.replacen(&sha256, &sha256.to_uppercase(), 1))),
            ("no signature file", None),
        ] {
            let dir = pins_dir(&["1.4.0", "1.5.0"]);
            let bad = dir.path().join("ai-job-search-v1.5.0.zip.sha256");
            match content {
                Some(content) => fs::write(&bad, content).unwrap(),
                None => fs::remove_file(dir.path().join("ai-job-search-v1.5.0.zip.sha256.sig")).unwrap(),
            }
            let output = run_script(dir.path());
            assert!(!output.status.success(), "{what}");
            assert!(output.stdout.is_empty(), "{what}: no feed is written");
            let said = String::from_utf8_lossy(&output.stderr);
            assert!(said.contains("ai-job-search-v1.5.0.zip.sha256"), "{what}: {said}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_script_lists_up_to_five_newest_first() {
        if std::process::Command::new("jq").arg("--version").output().is_err() {
            return;
        }
        // 1.10.0 is newer than 1.9.0: the order is by number, not by letter.
        let dir = pins_dir(&["1.9.0", "1.10.0", "1.2.0", "1.4.0", "1.3.0"]);
        let output = run_script(dir.path());
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let feed: Value = serde_json::from_slice(&output.stdout).unwrap();
        let listed: Vec<&str> = feed["ai_job_search_releases"].as_array().unwrap().iter().map(|entry| entry["version"].as_str().unwrap()).collect();
        assert_eq!(listed, ["1.10.0", "1.9.0", "1.4.0", "1.3.0", "1.2.0"]);
        assert_eq!(feed["ai_job_search"], feed["ai_job_search_releases"][0], "the old field is the newest");
        let one = run_script(pins_dir(&["1.4.0"]).path());
        let one: Value = serde_json::from_slice(&one.stdout).unwrap();
        assert_eq!(one["ai_job_search_releases"].as_array().unwrap().len(), 1);
        assert_eq!(one["ai_job_search"]["version"], "1.4.0");
    }
}
