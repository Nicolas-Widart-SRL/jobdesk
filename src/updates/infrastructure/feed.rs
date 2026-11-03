use std::io::Read;

use chrono::DateTime;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::updates::domain::{FEED_URL, Failure, Platform, Release, Version};
use crate::updates::infrastructure::http::{Fetch, Transport, read_failure};

/// The most of the feed that is read. A longer one is refused whole.
pub const FEED_LIMIT: u64 = 256 * 1024;
/// The most of the release notes that is kept. Longer notes are cut at a character.
pub const NOTES_LIMIT: usize = 64 * 1024;

/// `latest.json`, as `scripts/update-feed.sh` writes it. Any other field, such as an address, is never read.
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

/// Reads the feed from GitHub and gives the release it names, for `platform`'s build.
pub fn read_feed(transport: &dyn Transport, platform: Option<Platform>) -> Result<Release, Failure> {
    let body = transport.open(FEED_URL, Fetch::Feed)?;
    let mut bytes = Vec::new();
    body.take(FEED_LIMIT + 1).read_to_end(&mut bytes).map_err(|error| read_failure(&error))?;
    if bytes.len() as u64 > FEED_LIMIT {
        return Err(Failure::BadFeed);
    }
    parse_feed(&bytes, platform)
}

/// A feed whose version or date cannot be read is `BadFeed`, and one with no build for `platform` is `NoBuild`.
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
    Ok(Release { version, released, notes: cut(feed.notes), size: build.size, signature: build.signature })
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
mod tests {
    use std::fs;
    use std::io::{self, Cursor};

    use chrono::NaiveDate;

    use super::*;
    use crate::updates::infrastructure::http::Body;
    use crate::updates::infrastructure::signature::tests::{fixture, fixture_signature};

    fn mac() -> Option<Platform> {
        Some(Platform::MacArm)
    }

    fn feed(platforms: &str) -> String {
        format!(r#"{{"version": "0.2.0", "pub_date": "2026-10-30T09:00:00Z", "notes": "Notes", "platforms": {platforms}}}"#)
    }

    struct Serves(Vec<u8>);

    impl Transport for Serves {
        fn open(&self, url: &str, fetch: Fetch) -> Result<Body, Failure> {
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
        assert_eq!(read_feed(&Serves(oversized.into_bytes()), mac()), Err(Failure::BadFeed));
        let fits = fs::read(fixture("latest.json")).unwrap();
        assert!(read_feed(&Serves(fits), mac()).is_ok());
    }

    #[test]
    fn a_feed_that_cannot_be_reached_says_why() {
        struct Down;
        impl Transport for Down {
            fn open(&self, _url: &str, _fetch: Fetch) -> Result<Body, Failure> {
                Err(Failure::Refused(503))
            }
        }
        struct Drops;
        impl Transport for Drops {
            fn open(&self, _url: &str, _fetch: Fetch) -> Result<Body, Failure> {
                Ok(Box::new(Cursor::new(b"{\"version\"".to_vec()).chain(Failing)))
            }
        }
        struct Failing;
        impl Read for Failing {
            fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::TimedOut, "slow"))
            }
        }
        assert_eq!(read_feed(&Down, mac()), Err(Failure::Refused(503)));
        assert_eq!(read_feed(&Drops, mac()), Err(Failure::TimedOut));
    }
}
