use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::portals::domain::{CommunityList, CountryCode, FolderName, GitSha, ListedPortal, parse_codeowners};
use crate::shared_kernel::replace_file::rename_over;

/// The most of the saved file that is read. A longer one reads as no saved list.
pub const CACHE_LIMIT: u64 = 1024 * 1024;
/// The shape of the file. A file of another version reads as no saved list.
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    /// RFC 3339.
    saved_at: String,
    portals: Vec<PortalRecord>,
    codeowners: Option<OwnersRecord>,
}

#[derive(Serialize, Deserialize)]
struct PortalRecord {
    country: String,
    folder: String,
    name: String,
    sentence: Option<String>,
    sha: String,
}

#[derive(Serialize, Deserialize)]
struct OwnersRecord {
    sha: String,
    /// The lines Jobdesk read, as `CODEOWNERS` text.
    lines: String,
}

/// The last community list read in full, in `portals/community.json` of the app's data folder. It is not a secret:
/// everything in it is public on GitHub. It is read again as untrusted, as if it came from GitHub.
pub struct CommunityCache {
    path: PathBuf,
}

impl CommunityCache {
    pub fn in_data(data: &Path) -> Self {
        Self { path: data.join("portals").join("community.json") }
    }

    /// The saved list, or `None` when there is none, or the file is too large or fails any check.
    pub fn load(&self) -> Option<CommunityList> {
        let mut bytes = Vec::new();
        File::open(&self.path).ok()?.take(CACHE_LIMIT + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > CACHE_LIMIT {
            return None;
        }
        list_from(serde_json::from_slice(&bytes).ok()?)
    }

    /// Goes through a copy of its own, so a crash leaves the previous list whole, and two windows saving at once each
    /// put a whole list in place. A copy left by a save that failed is removed.
    pub fn save(&self, list: &CommunityList) -> io::Result<()> {
        let bytes = serde_json::to_vec(&record_from(list)).map_err(io::Error::other)?;
        let folder = self.path.parent().ok_or(io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(folder)?;
        let mut copy = tempfile::Builder::new().prefix("community.json.").suffix(".tmp").tempfile_in(folder)?;
        copy.write_all(&bytes)?;
        // On disk before the rename, or a power loss could keep the rename and lose the list.
        copy.as_file().sync_all()?;
        // Kept while it is still at its own path: on Windows keeping clears the temporary attribute there, and fails once
        // the copy has moved. From here on, this save removes its copy itself.
        let copy = copy.into_temp_path().keep().map_err(|error| error.error)?;
        rename_over(&copy, &self.path).inspect_err(|_| {
            let _ = fs::remove_file(&copy);
        })
    }
}

fn record_from(list: &CommunityList) -> Record {
    Record {
        version: VERSION,
        saved_at: list.saved_at.to_rfc3339(),
        portals: list
            .portals
            .iter()
            .map(|portal| PortalRecord {
                country: portal.country.as_str().to_owned(),
                folder: portal.folder.as_str().to_owned(),
                name: portal.name.clone(),
                sentence: portal.sentence.clone(),
                sha: portal.sha.as_str().to_owned(),
            })
            .collect(),
        codeowners: list.codeowners_sha.as_ref().map(|sha| OwnersRecord { sha: sha.as_str().to_owned(), lines: list.codeowners.to_text() }),
    }
}

/// Every field checked again, as when it came from GitHub. One that fails means no saved list.
fn list_from(record: Record) -> Option<CommunityList> {
    if record.version != VERSION {
        return None;
    }
    let saved_at = DateTime::parse_from_rfc3339(&record.saved_at).ok()?.with_timezone(&Utc);
    let portals = record
        .portals
        .into_iter()
        .map(|portal| {
            let (country, folder, sha) = (CountryCode::parse(&portal.country)?, FolderName::parse(&portal.folder)?, GitSha::parse(&portal.sha)?);
            ListedPortal::from_parts(country, folder, sha, &portal.name, portal.sentence.as_deref())
        })
        .collect::<Option<Vec<_>>>()?;
    let (codeowners, codeowners_sha) = match record.codeowners {
        None => Default::default(),
        Some(owners) => (parse_codeowners(&owners.lines), Some(GitSha::parse(&owners.sha)?)),
    };
    Some(CommunityList { saved_at, portals, codeowners, codeowners_sha })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn list() -> CommunityList {
        let part = |country: &str, folder: &str, sha: char, name: &str, sentence: Option<&str>| {
            let sha = GitSha::parse(&sha.to_string().repeat(40)).unwrap();
            ListedPortal::from_parts(CountryCode::parse(country).unwrap(), FolderName::parse(folder).unwrap(), sha, name, sentence).unwrap()
        };
        CommunityList {
            saved_at: Utc.with_ymd_and_hms(2026, 11, 10, 8, 48, 0).unwrap(),
            portals: vec![part("be", "northfield-search", '1', "Northfield", Some("Belgian jobs.")), part("global", "alder-search", '2', "Alder", None)],
            codeowners: parse_codeowners("* @fallback-desk\n/portals/be/northfield-search/ @lena-vos # unmaintained since 2025-07\n"),
            codeowners_sha: GitSha::parse(&"3".repeat(40)),
        }
    }

    fn cache() -> (tempfile::TempDir, CommunityCache) {
        let data = tempfile::tempdir().unwrap();
        let cache = CommunityCache::in_data(data.path());
        (data, cache)
    }

    fn write(cache: &CommunityCache, text: &str) {
        fs::create_dir_all(cache.path.parent().unwrap()).unwrap();
        fs::write(&cache.path, text).unwrap();
    }

    #[test]
    fn a_saved_list_reads_back_the_same() {
        let (_data, cache) = cache();
        assert_eq!(cache.load(), None, "no file, no saved list");
        cache.save(&list()).unwrap();
        assert_eq!(cache.load(), Some(list()));
        assert!(!cache.path.with_extension("json.tmp").exists());
        let bare = CommunityList { codeowners: Default::default(), codeowners_sha: None, ..list() };
        cache.save(&bare).unwrap();
        assert_eq!(cache.load(), Some(bare));
    }

    #[test]
    fn a_corrupt_or_tampered_file_is_no_saved_list() {
        let (_data, cache) = cache();
        cache.save(&list()).unwrap();
        let good = fs::read_to_string(&cache.path).unwrap();
        for bad in [
            "not json".to_owned(),
            good.replace(r#""version":1"#, r#""version":2"#),
            good.replace("2026-11-10", "tomorrow"),
            good.replace("northfield-search", "../../etc"),
            good.replace(r#""be""#, r#""BE""#),
            good.replace(&"1".repeat(40), "1111"),
            good.replace(&"3".repeat(40), "zz"),
            good.replace(r#""Alder""#, "\"\u{200B}\""),
        ] {
            write(&cache, &bad);
            assert_eq!(cache.load(), None, "{bad}");
        }
    }

    #[test]
    fn saved_words_are_cleaned_again() {
        let (_data, cache) = cache();
        cache.save(&list()).unwrap();
        let good = fs::read_to_string(&cache.path).unwrap();
        write(&cache, &good.replace("\"Northfield\"", "\"North\\u200bfield\\nSearch\""));
        assert_eq!(cache.load().unwrap().portals[0].name, "Northfield Search");
    }

    /// Two windows may save at once: each save writes its own copy, so every save lands whole and none fails.
    #[test]
    fn saves_at_once_never_mix() {
        let (data, _cache) = cache();
        let lists: Vec<CommunityList> = (0..8).map(|n| CommunityList { saved_at: list().saved_at + chrono::TimeDelta::seconds(n), ..list() }).collect();
        std::thread::scope(|scope| {
            for saved in &lists {
                let folder = data.path();
                scope.spawn(move || {
                    for _ in 0..20 {
                        CommunityCache::in_data(folder).save(saved).unwrap();
                    }
                });
            }
        });
        let loaded = CommunityCache::in_data(data.path()).load().unwrap();
        assert!(lists.contains(&loaded));
        let left: Vec<_> = fs::read_dir(data.path().join("portals")).unwrap().flatten().map(|entry| entry.file_name()).collect();
        assert_eq!(left, ["community.json"], "no copy is left behind");
    }

    #[test]
    fn an_oversized_file_is_no_saved_list() {
        let (_data, cache) = cache();
        cache.save(&list()).unwrap();
        let good = fs::read_to_string(&cache.path).unwrap();
        let padded = good.replacen('{', &format!("{{{}", " ".repeat(CACHE_LIMIT as usize)), 1);
        write(&cache, &padded);
        assert_eq!(cache.load(), None);
        write(&cache, &good);
        assert!(cache.load().is_some());
    }
}
