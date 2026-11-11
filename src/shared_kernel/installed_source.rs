//! The `ai-job-search` release a job search folder was set up from, kept in the folder itself as the base a later
//! update of the folder starts from: `.jobdesk/source.zip`, the exact bytes that matched the signed SHA-256, and
//! `.jobdesk/source.json`, the version and that SHA-256. It lives in the folder, not the data folder, so it moves and
//! renames with the folder and survives a fresh data folder.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The folder in a job search folder that holds the kept release.
pub const DIR: &str = ".jobdesk";
/// The largest release zip setup downloads, and so the largest kept one that reads.
pub const MAX_ZIP: u64 = 100 * 1024 * 1024;

/// The kept zip and its record, in `DIR`.
pub const ZIP: &str = "source.zip";
pub const RECORD: &str = "source.json";
/// The largest record that reads: a version and 64 hex digits need far less.
const MAX_RECORD: u64 = 4 * 1024;

/// The release a folder was set up from: its version, such as `1.7.5`, and the SHA-256 of its zip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledSource {
    pub version: String,
    /// 64 lowercase hex digits, as `sha256sum` writes them.
    pub sha256: String,
}

/// 64 lowercase hex digits, as `sha256sum` writes them and the pin holds them.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Writes `bytes` and the record of `source` into a new `.jobdesk` folder of `staging`, readable only by the user. The
/// folder must not be there yet, and `staging` must be, so nothing is written once `staging` has gone.
pub fn write_into(staging: &Path, source: &InstalledSource, bytes: &[u8]) -> io::Result<()> {
    let dir = staging.join(DIR);
    let mut builder = fs::DirBuilder::new();
    // Not recursive: a missing `staging` is an error, and so is a `.jobdesk` that is there already.
    builder.recursive(false);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(&dir)?;
    write_new(&dir.join(ZIP), bytes)?;
    write_new(&dir.join(RECORD), &record(source)?)
}

/// The bytes of the record of `source`, as `RECORD` holds them.
pub fn record(source: &InstalledSource) -> io::Result<Vec<u8>> {
    serde_json::to_vec_pretty(source).map_err(io::Error::other)
}

/// The release `folder` was set up from and its zip's bytes. `None` when either file is missing, a link, or larger than
/// it may be, when `.jobdesk` is a link, when the record does not read, when its version is not exactly three numbers
/// or its hash not 64 lowercase hex digits, or when the zip's SHA-256 is not the one the record holds.
///
/// Links are refused when looked at, with a short window before the open, and only the file name is opened without
/// following a link on Unix. The record sits beside the zip, so the hash check catches a damaged copy, not a change by
/// something that can write the folder, such as a run's tools. A caller that needs to trust the base must check it
/// against the signed pin.
pub fn read(folder: &Path) -> Option<(InstalledSource, Vec<u8>)> {
    let dir = folder.join(DIR);
    if !fs::symlink_metadata(&dir).ok()?.is_dir() {
        return None;
    }
    let record = read_record(&dir.join(RECORD))?;
    let bytes = read_file(&dir.join(ZIP), MAX_ZIP)?;
    (sha256_hex(&bytes) == record.sha256).then_some((record, bytes))
}

/// A record as `record` writes it, from the plain file at `path`. `None` when it is missing, a link, larger than a
/// record may be, or does not read, or when its version is not exactly three numbers or its hash not 64 lowercase hex
/// digits. The folders above `path` are the caller's to check.
pub fn read_record(path: &Path) -> Option<InstalledSource> {
    let record: InstalledSource = serde_json::from_slice(&read_file(path, MAX_RECORD)?).ok()?;
    (plain_version(&record.version) && lowercase_sha256(&record.sha256)).then_some(record)
}

/// Exactly three dot-separated whole numbers, such as `1.7.5`: the pin's rule for a version, without its `v` prefix,
/// which setup never writes. Written here because the shared kernel does not reach into the `updates` feature. A record
/// whose version breaks it never reads, so nothing writes one.
pub fn plain_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3 && parts.iter().all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()) && part.parse::<u32>().is_ok())
}

/// 64 lowercase hex digits: the pin's rule for a hash.
fn lowercase_sha256(sha256: &str) -> bool {
    sha256.len() == 64 && sha256.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Writes a new file of `bytes` at `path`, readable only by the user. A file or link already there fails it.
fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

/// The whole of the plain file at `path`, or `None` for a link, anything but a file, or more than `limit` bytes.
fn read_file(path: &Path, limit: u64) -> Option<Vec<u8>> {
    if !fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    let file = open_no_follow(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

/// Opens `path` for reading, refusing a link that took its place since it was looked at.
fn open_no_follow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NOFOLLOW);
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(bytes: &[u8]) -> InstalledSource {
        InstalledSource { version: "1.7.5".to_owned(), sha256: sha256_hex(bytes) }
    }

    fn written(bytes: &[u8]) -> tempfile::TempDir {
        let folder = tempfile::tempdir().unwrap();
        write_into(folder.path(), &source(bytes), bytes).unwrap();
        folder
    }

    #[test]
    fn what_is_written_reads_back() {
        let folder = written(b"fictional release zip");
        assert_eq!(read(folder.path()), Some((source(b"fictional release zip"), b"fictional release zip".to_vec())));
        let record: serde_json::Value = serde_json::from_slice(&fs::read(folder.path().join(DIR).join(RECORD)).unwrap()).unwrap();
        assert_eq!(record, serde_json::json!({"version": "1.7.5", "sha256": sha256_hex(b"fictional release zip")}));
    }

    #[cfg(unix)]
    #[test]
    fn only_the_user_can_read_it() {
        use std::os::unix::fs::PermissionsExt;
        let folder = written(b"zip");
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&folder.path().join(DIR)), 0o700);
        assert_eq!(mode(&folder.path().join(DIR).join(ZIP)), 0o600);
        assert_eq!(mode(&folder.path().join(DIR).join(RECORD)), 0o600);
    }

    #[test]
    fn a_second_write_or_a_missing_staging_folder_fails() {
        let folder = written(b"zip");
        assert!(write_into(folder.path(), &source(b"zip"), b"zip").is_err());
        assert!(write_into(&folder.path().join("gone"), &source(b"zip"), b"zip").is_err());
        assert!(!folder.path().join("gone").exists());
    }

    #[test]
    fn a_changed_zip_reads_as_none() {
        let folder = written(b"fictional release zip");
        fs::write(folder.path().join(DIR).join(ZIP), b"another zip").unwrap();
        assert_eq!(read(folder.path()), None);
    }

    #[test]
    fn a_missing_or_unreadable_record_reads_as_none() {
        let folder = written(b"zip");
        fs::write(folder.path().join(DIR).join(RECORD), b"not json").unwrap();
        assert_eq!(read(folder.path()), None);
        fs::remove_file(folder.path().join(DIR).join(RECORD)).unwrap();
        assert_eq!(read(folder.path()), None);
        assert_eq!(read(tempfile::tempdir().unwrap().path()), None, "a folder with no .jobdesk");
    }

    #[test]
    fn a_record_that_breaks_the_pin_rule_reads_as_none() {
        let bytes = b"fictional release zip";
        let hash = sha256_hex(bytes);
        let with = |version: &str, sha256: &str| {
            let folder = written(bytes);
            let record = serde_json::json!({"version": version, "sha256": sha256});
            fs::write(folder.path().join(DIR).join(RECORD), record.to_string()).unwrap();
            read(folder.path())
        };
        assert!(with("1.7.5", &hash).is_some(), "the rule itself lets a plain record through");
        for version in ["1.7", "1.7.5.0", "v1.7.5", "1.7.5-beta", "1..5", "../1.7.5", "1.7.5\n", "1.\u{202e}7.5", "1.7.99999999999", ""] {
            assert_eq!(with(version, &hash), None, "version {version:?}");
        }
        let upper = hash.to_uppercase();
        for sha256 in [&hash[..63], upper.as_str(), &format!("{hash}0"), &format!("{}g", &hash[..63])] {
            assert_eq!(with("1.7.5", sha256), None, "hash {sha256:?}");
        }
    }

    #[test]
    fn a_zip_larger_than_the_limit_reads_as_none() {
        let folder = written(b"zip");
        let zip = File::options().write(true).open(folder.path().join(DIR).join(ZIP)).unwrap();
        zip.set_len(MAX_ZIP + 1).unwrap();
        assert_eq!(read(folder.path()), None);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_never_followed() {
        use std::os::unix::fs::symlink;
        let real = written(b"zip");
        // `.jobdesk` itself a link to a real base.
        let linked_dir = tempfile::tempdir().unwrap();
        symlink(real.path().join(DIR), linked_dir.path().join(DIR)).unwrap();
        assert_eq!(read(linked_dir.path()), None);
        // Each file a link to its real copy.
        for name in [ZIP, RECORD] {
            let folder = tempfile::tempdir().unwrap();
            fs::create_dir(folder.path().join(DIR)).unwrap();
            for other in [ZIP, RECORD] {
                let to = folder.path().join(DIR).join(other);
                if other == name {
                    symlink(real.path().join(DIR).join(other), &to).unwrap();
                } else {
                    fs::copy(real.path().join(DIR).join(other), &to).unwrap();
                }
            }
            assert_eq!(read(folder.path()), None, "{name} as a link");
        }
    }
}
