//! The list of ai-job-search releases this Jobdesk is compatible with, and the script that maintains it.
//! The script tests use a temporary list and zips made here, and never touch the network.

use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn entries_of(text: &str) -> Vec<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(text).expect("the list is JSON");
    value
        .as_array()
        .expect("the list is an array")
        .iter()
        .map(|entry| {
            let object = entry.as_object().expect("an entry is an object");
            assert_eq!(object.len(), 2, "an entry holds a version and a sha256 only");
            (object["version"].as_str().expect("a version is text").to_owned(), object["sha256"].as_str().expect("a sha256 is text").to_owned())
        })
        .collect()
}

fn numbers(version: &str) -> Vec<u64> {
    assert!(
        version.split('.').count() == 3 && version.split('.').all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
        "{version} is not X.Y.Z"
    );
    version.split('.').map(|part| part.parse().unwrap()).collect()
}

#[test]
fn the_committed_list_is_well_formed() {
    let text = fs::read_to_string(root().join("assets/update/ai-job-search.json")).expect("the list is committed");
    assert!(text.ends_with("}\n]\n"), "the list ends in a newline");
    let entries = entries_of(&text);
    assert!((1..=5).contains(&entries.len()), "the list holds 1 to 5 releases, not {}", entries.len());
    for (version, sha256) in &entries {
        numbers(version);
        assert!(sha256.len() == 64 && sha256.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')), "{version}: {sha256} is not 64 lowercase hex");
    }
    for pair in entries.windows(2) {
        assert!(numbers(&pair[0].0) > numbers(&pair[1].0), "{} must come before {}: newest first, strictly", pair[0].0, pair[1].0);
    }
}

#[cfg(unix)]
mod script {
    use std::io::{Cursor, Write};
    use std::process::{Command, Output};

    use sha2::{Digest, Sha256};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;

    fn tools_present() -> bool {
        let found = |program: &str, argument: &str| Command::new(program).arg(argument).output().map(|output| output.status.success()).unwrap_or(false);
        let present = found("jq", "--version") && Command::new("unzip").arg("-v").output().is_ok();
        if !present {
            eprintln!("skipped: the script needs jq and unzip");
        }
        present
    }

    struct Sandbox {
        dir: tempfile::TempDir,
    }

    impl Sandbox {
        fn new() -> Self {
            Self { dir: tempfile::tempdir().unwrap() }
        }

        fn list(&self) -> PathBuf {
            self.dir.path().join("ai-job-search.json")
        }

        /// A zip named after `name` whose bytes depend on `seed`, and the SHA-256 of it.
        fn zip(&self, name: &str, seed: &str) -> (PathBuf, String) {
            let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
            writer.start_file("ai-job-search/README.md", SimpleFileOptions::default().compression_method(CompressionMethod::Stored)).unwrap();
            writer.write_all(seed.as_bytes()).unwrap();
            let bytes = writer.finish().unwrap().into_inner();
            let path = self.dir.path().join(name);
            fs::write(&path, &bytes).unwrap();
            let sha256 = Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect();
            (path, sha256)
        }

        fn run(&self, arguments: &[&str]) -> Output {
            Command::new(root().join("scripts/ai-job-search.sh")).args(arguments).env("AI_JOB_SEARCH_LIST", self.list()).output().expect("the script runs")
        }

        /// Adds the release `version` with a zip seeded by `seed`, and says it worked.
        fn add(&self, version: &str, seed: &str) -> (String, Output) {
            let (zip, sha256) = self.zip(&format!("{version}-{seed}.zip"), seed);
            let output = self.run(&["add", &format!("v{version}"), zip.to_str().unwrap()]);
            assert!(output.status.success(), "add v{version}: {}", text(&output));
            (sha256, output)
        }

        fn versions(&self) -> Vec<String> {
            entries_of(&fs::read_to_string(self.list()).unwrap()).into_iter().map(|(version, _)| version).collect()
        }

        fn bytes(&self) -> Vec<u8> {
            fs::read(self.list()).unwrap()
        }

        /// The list holds `versions`, so full or nearly so, each with a zip of its own.
        fn fill(&self, versions: &[&str]) {
            for version in versions {
                self.add(version, version);
            }
        }
    }

    fn text(output: &Output) -> String {
        format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
    }

    fn assert_refused(output: &Output) {
        assert_eq!(output.status.code(), Some(1), "{}", text(output));
    }

    #[test]
    fn the_newest_goes_on_top() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        let (first, _) = sandbox.add("1.0.0", "one");
        let (second, _) = sandbox.add("1.1.0", "two");
        assert_eq!(sandbox.versions(), ["1.1.0", "1.0.0"]);
        let entries = entries_of(&fs::read_to_string(sandbox.list()).unwrap());
        assert_eq!(entries[0].1, second);
        assert_eq!(entries[1].1, first);
        let written = String::from_utf8(sandbox.bytes()).unwrap();
        assert!(written.starts_with("[\n  {\n    \"version\": \"1.1.0\",\n    \"sha256\": \""), "{written}");
        assert!(written.ends_with("}\n]\n"), "{written}");
    }

    #[test]
    fn a_tenth_minor_sorts_above_a_ninth() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        sandbox.fill(&["1.9.0", "1.10.0", "1.2.0"]);
        assert_eq!(sandbox.versions(), ["1.10.0", "1.9.0", "1.2.0"]);
    }

    #[test]
    fn a_sixth_entry_drops_the_oldest_and_says_so() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        sandbox.fill(&["1.1.0", "1.2.0", "1.3.0", "1.4.0", "1.5.0"]);
        let (_, output) = sandbox.add("1.6.0", "six");
        assert_eq!(sandbox.versions(), ["1.6.0", "1.5.0", "1.4.0", "1.3.0", "1.2.0"]);
        assert!(text(&output).contains("dropped 1.1.0"), "{}", text(&output));
    }

    #[test]
    fn the_same_hash_changes_nothing() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        sandbox.fill(&["1.0.0", "1.1.0"]);
        let before = sandbox.bytes();
        let (zip, _) = sandbox.zip("again.zip", "1.1.0");
        let output = sandbox.run(&["add", "v1.1.0", zip.to_str().unwrap()]);
        assert!(output.status.success(), "{}", text(&output));
        assert!(text(&output).contains("already"), "{}", text(&output));
        assert_eq!(sandbox.bytes(), before);
    }

    #[test]
    fn another_hash_fails_and_changes_nothing() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        sandbox.fill(&["1.0.0", "1.1.0"]);
        let before = sandbox.bytes();
        let (zip, _) = sandbox.zip("other.zip", "not the same bytes");
        let output = sandbox.run(&["add", "v1.1.0", zip.to_str().unwrap()]);
        assert_refused(&output);
        assert_eq!(sandbox.bytes(), before);
    }

    #[test]
    fn a_release_older_than_the_oldest_is_refused_when_the_list_is_full() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        sandbox.fill(&["1.1.0", "1.2.0", "1.3.0", "1.4.0", "1.5.0"]);
        let before = sandbox.bytes();
        let (zip, _) = sandbox.zip("old.zip", "old");
        let output = sandbox.run(&["add", "v1.0.0", zip.to_str().unwrap()]);
        assert_refused(&output);
        assert_eq!(sandbox.bytes(), before);
    }

    #[test]
    fn a_version_that_is_not_three_numbers_is_refused() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        let (zip, _) = sandbox.zip("short.zip", "short");
        for version in ["v1.4", "1.4.0", "v1.4.0-rc1", "v1.4.0.1"] {
            let output = sandbox.run(&["add", version, zip.to_str().unwrap()]);
            assert_refused(&output);
        }
        assert!(!sandbox.list().exists(), "a refused add writes no list");
    }

    /// The app reads each part as a `u32` and names the zip from those numbers, so `01.8.0` would sign a zip name the
    /// app never asks for, and a part above `u32::MAX` would never read. Only the form the app writes back is a version.
    #[test]
    fn a_version_the_app_would_not_write_back_is_refused() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        sandbox.fill(&["1.0.0"]);
        let before = sandbox.bytes();
        let (zip, _) = sandbox.zip("odd.zip", "odd");
        for version in ["v01.8.0", "v1.08.0", "v1.8.00", "v00.8.0", "v1.8.4294967296", "v4294967296.0.0", "v1.8.99999999999"] {
            assert_refused(&sandbox.run(&["add", version, zip.to_str().unwrap()]));
            assert_eq!(sandbox.bytes(), before, "{version}: the file is unchanged");
        }
        sandbox.add("0.8.0", "zero");
        sandbox.add("1.8.4294967295", "largest");
        assert_eq!(sandbox.versions(), ["1.8.4294967295", "1.0.0", "0.8.0"]);
    }

    #[test]
    fn a_list_holding_a_version_the_app_would_not_write_back_is_refused() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        let sha256 = "0".repeat(64);
        for version in ["01.8.0", "1.8.4294967296", "1.8.0\n2"] {
            let list = serde_json::json!([{ "version": version, "sha256": sha256 }]).to_string();
            fs::write(sandbox.list(), &list).unwrap();
            let output = sandbox.run(&["check"]);
            assert_refused(&output);
            // Refused as a list, before any download: a check that downloads would fail too, but for another reason.
            assert!(text(&output).contains("is not a list of 1 to") && !text(&output).contains("checking"), "{version:?}: {}", text(&output));
            assert_eq!(fs::read_to_string(sandbox.list()).unwrap(), list, "{version:?}: the file is unchanged");
        }
    }

    #[test]
    fn a_file_that_is_not_a_zip_is_refused() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        sandbox.fill(&["1.0.0"]);
        let before = sandbox.bytes();
        let not_zip = sandbox.dir.path().join("not.zip");
        fs::write(&not_zip, "this is not a zip").unwrap();
        let output = sandbox.run(&["add", "v1.1.0", not_zip.to_str().unwrap()]);
        assert_refused(&output);
        assert_eq!(sandbox.bytes(), before);
    }

    /// A file of several JSON values is no list, even when each is one: `jq -e` alone would judge only the last.
    #[test]
    fn a_file_holding_two_arrays_is_refused() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        sandbox.fill(&["1.0.0", "1.1.0"]);
        let one = String::from_utf8(sandbox.bytes()).unwrap();
        let (zip, _) = sandbox.zip("new.zip", "new");
        for (second, why) in [(one.as_str(), "the same list twice"), ("[]\n", "a list, then an empty one")] {
            let both = format!("{one}{second}");
            fs::write(sandbox.list(), &both).unwrap();
            assert_refused(&sandbox.run(&["remove", "v1.1.0"]));
            assert_refused(&sandbox.run(&["add", "v1.2.0", zip.to_str().unwrap()]));
            assert_refused(&sandbox.run(&["check"]));
            assert_eq!(fs::read_to_string(sandbox.list()).unwrap(), both, "{why}: the file is unchanged");
        }
    }

    #[test]
    fn remove_works_and_refuses_the_last_entry() {
        if !tools_present() {
            return;
        }
        let sandbox = Sandbox::new();
        sandbox.fill(&["1.0.0", "1.1.0", "1.2.0"]);
        let output = sandbox.run(&["remove", "v1.1.0"]);
        assert!(output.status.success(), "{}", text(&output));
        assert_eq!(sandbox.versions(), ["1.2.0", "1.0.0"]);
        assert_refused(&sandbox.run(&["remove", "v1.1.0"]));
        assert!(sandbox.run(&["remove", "v1.2.0"]).status.success());
        let before = sandbox.bytes();
        assert_refused(&sandbox.run(&["remove", "v1.0.0"]));
        assert_eq!(sandbox.bytes(), before);
        assert_eq!(sandbox.versions(), ["1.0.0"]);
        assert_refused(&sandbox.run(&["remove", "v1.4"]));
    }
}
