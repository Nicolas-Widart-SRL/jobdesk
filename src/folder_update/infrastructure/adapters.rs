//! The ports of a folder update over the disk, and over the one https client that setup downloads the release with.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError, TryLockError};

use crate::folder_update::domain::{Base, BaseReader, FolderProbe, FolderUpdater, Report, Reverted, Update, UpdateError, ZipFetcher};
use crate::folder_update::infrastructure::disk::is_real_dir;
use crate::folder_update::infrastructure::hold::{Open, hold_open};
use crate::folder_update::infrastructure::updater;
use crate::shared_kernel::download::{DownloadFailure, HttpsFetcher};
use crate::shared_kernel::installed_source::{self, InstalledSource, MAX_ZIP};

/// A job search folder on the disk, and the one this Jobdesk has open.
#[derive(Default)]
pub struct DiskFolder {
    open: Mutex<Opened>,
}

/// The folder `open` named last, and this Jobdesk's hold on it.
#[derive(Default)]
struct Opened {
    folder: Option<PathBuf>,
    /// The hold, while this Jobdesk has one.
    hold: Option<File>,
    /// Another Jobdesk held the folder alone at the last try, so this one could not hold it.
    kept_out: bool,
}

impl Opened {
    /// Tries to hold the open folder, and says whether another Jobdesk keeps this one out.
    fn hold(&mut self) -> bool {
        let Some(folder) = &self.folder else { return false };
        (self.hold, self.kept_out) = match hold_open(folder) {
            Open::Held(file) => (Some(file), false),
            Open::Elsewhere => (None, true),
            Open::Nothing => (None, false),
        };
        self.kept_out
    }
}

impl DiskFolder {
    /// Runs `job` on `folder` with this Jobdesk's own hold on it let go, so the job can hold the folder alone, then
    /// takes the hold again. The hold is let go of only while `job` runs, and taken again even when the job failed, or
    /// when it could not be taken at the open because another Jobdesk held the folder alone then.
    fn alone<T>(&self, folder: &Path, job: impl FnOnce() -> T) -> T {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let held = open.folder.as_deref() == Some(folder);
        if held {
            open.hold = None;
        }
        let done = job();
        if held {
            open.hold();
        }
        done
    }
}

/// Whether nothing at all, not even a link, is at `path`. Anything that cannot be looked at counts as there.
fn missing(path: &Path) -> bool {
    matches!(fs::symlink_metadata(path), Err(error) if error.kind() == io::ErrorKind::NotFound)
}

impl FolderProbe for DiskFolder {
    fn exists(&self, folder: &Path) -> bool {
        fs::metadata(folder).is_ok_and(|meta| meta.is_dir())
    }

    fn has_git(&self, folder: &Path) -> bool {
        !missing(&folder.join(".git"))
    }
}

impl BaseReader for DiskFolder {
    fn read(&self, folder: &Path) -> Base {
        if let Some((source, zip)) = installed_source::read(folder) {
            return Base::Found { source, zip };
        }
        let dir = folder.join(installed_source::DIR);
        let empty = is_real_dir(&dir) && missing(&dir.join(installed_source::ZIP)) && missing(&dir.join(installed_source::RECORD));
        if missing(&dir) || empty { Base::Missing } else { Base::Bad }
    }

    fn skipped(&self, folder: &Path) -> Option<InstalledSource> {
        updater::skipped(folder)
    }
}

impl FolderUpdater for DiskFolder {
    fn open(&self, folder: &Path) -> bool {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        // The folder held before is let go of first: it may be this one.
        *open = Opened { folder: Some(folder.to_owned()), ..Opened::default() };
        open.hold()
    }

    fn hold_again(&self, folder: &Path) -> bool {
        // A worker of this Jobdesk's own that holds the folder alone holds this too: never wait on it.
        let mut open = match self.open.try_lock() {
            Ok(open) => open,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return false,
        };
        if open.folder.as_deref() != Some(folder) || !open.kept_out {
            return false;
        }
        open.hold()
    }

    fn recover(&self, folder: &Path) -> Result<Option<Reverted>, UpdateError> {
        self.alone(folder, || updater::recover(folder))
    }

    fn update(&self, folder: &Path, update: &Update) -> Result<Report, UpdateError> {
        self.alone(folder, || updater::update(folder, update))
    }

    fn can_undo(&self, folder: &Path) -> bool {
        updater::can_undo(folder)
    }

    fn undo_unfinished(&self, folder: &Path) -> bool {
        updater::undo_unfinished(folder)
    }

    fn undo(&self, folder: &Path) -> Result<Reverted, UpdateError> {
        self.alone(folder, || updater::undo(folder))
    }
}

/// The release zip downloads as setup's does: over https only, into this Jobdesk's own folder, and no larger than the
/// largest release setup takes. Its bytes are read and the part file deleted at once.
impl ZipFetcher for HttpsFetcher {
    fn fetch(&self, url: &str, name: &str, progress: &mut dyn FnMut(u64)) -> Result<Vec<u8>, DownloadFailure> {
        let part = self.fetch_at_most(url, name, MAX_ZIP, &|| false, progress)?;
        let bytes = fs::read(&part).map_err(|_| DownloadFailure::Disk);
        let _ = fs::remove_file(&part);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use super::*;
    use crate::shared_kernel::https::{Body, Fetch, HttpsFailure, Transport};
    use crate::shared_kernel::installed_source::{InstalledSource, sha256_hex, write_into};

    const URL: &str = "https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/v1.8.0";

    fn set_up() -> tempfile::TempDir {
        let folder = tempfile::tempdir().unwrap();
        let source = InstalledSource { version: "1.7.5".into(), sha256: sha256_hex(b"fictional zip") };
        write_into(folder.path(), &source, b"fictional zip").unwrap();
        folder
    }

    #[test]
    fn a_kept_base_reads_whole() {
        let folder = set_up();
        let Base::Found { source, zip } = DiskFolder::default().read(folder.path()) else { panic!("no base") };
        assert_eq!((source.version.as_str(), zip.as_slice()), ("1.7.5", &b"fictional zip"[..]));
    }

    #[test]
    fn a_folder_that_keeps_nothing_has_no_base() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(DiskFolder::default().read(folder.path()), Base::Missing, "no .jobdesk");
        fs::create_dir(folder.path().join(installed_source::DIR)).unwrap();
        assert_eq!(DiskFolder::default().read(folder.path()), Base::Missing, "a .jobdesk with neither file");
    }

    #[test]
    fn a_base_that_does_not_read_is_bad() {
        let changed = set_up();
        fs::write(changed.path().join(installed_source::DIR).join(installed_source::ZIP), b"another zip").unwrap();
        assert_eq!(DiskFolder::default().read(changed.path()), Base::Bad);
        for name in [installed_source::ZIP, installed_source::RECORD] {
            let half = set_up();
            fs::remove_file(half.path().join(installed_source::DIR).join(name)).unwrap();
            assert_eq!(DiskFolder::default().read(half.path()), Base::Bad, "{name} alone is gone");
        }
        let file = tempfile::tempdir().unwrap();
        fs::write(file.path().join(installed_source::DIR), b"").unwrap();
        assert_eq!(DiskFolder::default().read(file.path()), Base::Bad, ".jobdesk is a file");
    }

    #[test]
    fn a_git_entry_of_any_kind_is_seen() {
        let folder = tempfile::tempdir().unwrap();
        assert!(DiskFolder::default().exists(folder.path()));
        assert!(!DiskFolder::default().has_git(folder.path()));
        fs::write(folder.path().join(".git"), b"gitdir: ../elsewhere").unwrap();
        assert!(DiskFolder::default().has_git(folder.path()), "a worktree's .git file");
        fs::remove_file(folder.path().join(".git")).unwrap();
        fs::create_dir(folder.path().join(".git")).unwrap();
        assert!(DiskFolder::default().has_git(folder.path()), "a .git folder");
        #[cfg(unix)]
        {
            fs::remove_dir(folder.path().join(".git")).unwrap();
            std::os::unix::fs::symlink(folder.path().join("nowhere"), folder.path().join(".git")).unwrap();
            assert!(DiskFolder::default().has_git(folder.path()), "a broken link");
        }
        assert!(!DiskFolder::default().exists(&folder.path().join("gone")));
    }

    /// An update from the base `set_up` keeps, whose zips do not read, so one that holds the folder alone fails on the
    /// base, past the lock. `sha256` is the base's.
    fn broken(sha256: &str) -> Update<'_> {
        use crate::folder_update::domain::Release;
        Update { base: Release { zip: b"fictional zip", sha256 }, base_version: "1.7.5", new: Release { zip: b"", sha256: "" }, version: "1.8.0" }
    }

    /// Two Jobdesks with one folder open, each its own `DiskFolder` and so its own handle of the lock file.
    #[test]
    fn a_folder_another_jobdesk_has_open_is_busy_and_its_own_hold_never_is() {
        let folder = set_up();
        let sha256 = sha256_hex(b"fictional zip");
        let broken = || broken(&sha256);
        let (first, second) = (DiskFolder::default(), DiskFolder::default());
        first.open(folder.path());
        assert_eq!(first.recover(folder.path()), Ok(None), "its own hold never keeps it out");
        second.open(folder.path());
        assert_eq!(first.recover(folder.path()), Err(UpdateError::Busy));
        assert_eq!(first.update(folder.path(), &broken()), Err(UpdateError::Busy));
        assert_eq!(first.undo(folder.path()), Err(UpdateError::Busy));
        assert_eq!(second.recover(folder.path()), Err(UpdateError::Busy), "the first holds it open again after each");

        let elsewhere = set_up();
        second.open(elsewhere.path());
        assert_eq!(first.recover(folder.path()), Ok(None), "the second let go of it");
        assert!(matches!(first.update(folder.path(), &broken()), Err(UpdateError::BadBase(_))));
        assert_eq!(first.undo(folder.path()), Err(UpdateError::NothingToUndo));
        assert_eq!(second.undo(folder.path()), Err(UpdateError::Busy), "while the first has it open");
    }

    #[test]
    fn opening_a_folder_with_no_jobdesk_makes_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let disk = DiskFolder::default();
        assert!(!disk.open(folder.path()), "never kept out");
        assert!(!disk.hold_again(folder.path()));
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 0);
    }

    /// A Jobdesk that opens a folder while another holds it alone, as a second handle does here, is kept out until it
    /// takes the hold, and holds it from then on.
    #[test]
    fn a_folder_another_jobdesk_holds_alone_keeps_this_one_out_until_it_holds_it() {
        let folder = set_up();
        let lock = folder.path().join(installed_source::DIR).join(".open");
        let other = fs::File::options().read(true).write(true).create(true).truncate(false).open(&lock).unwrap();
        other.lock().unwrap();
        let disk = DiskFolder::default();
        assert!(disk.open(folder.path()), "kept out at the open");
        assert!(disk.hold_again(folder.path()), "and while the other holds it");
        assert!(!disk.hold_again(&folder.path().join("another")), "of the open folder only");
        drop(other);
        assert!(!disk.hold_again(folder.path()), "the hold is taken");
        let other = fs::File::options().read(true).write(true).open(&lock).unwrap();
        assert!(other.try_lock().is_err(), "this Jobdesk holds it open now");
        assert!(!disk.hold_again(folder.path()));
    }

    struct Serves(Vec<u8>);

    impl Transport for Serves {
        fn open(&self, url: &str, fetch: Fetch) -> Result<Body, HttpsFailure> {
            assert_eq!((url, fetch), (URL, Fetch::Download));
            Ok(Box::new(Cursor::new(self.0.clone())))
        }
    }

    #[test]
    fn the_zip_is_read_and_its_part_file_deleted() {
        let data = tempfile::tempdir().unwrap();
        let fetcher = HttpsFetcher::new(Arc::new(Serves(b"zip".to_vec())), data.path().join("downloads"));
        let mut heard = 0;
        let zip = ZipFetcher::fetch(&fetcher, URL, "update-ai-job-search-v1.8.0.zip", &mut |received| heard = received).unwrap();
        assert_eq!((zip.as_slice(), heard), (&b"zip"[..], 3));
        let own = data.path().join("downloads").join(std::process::id().to_string());
        let left: Vec<_> = fs::read_dir(own).unwrap().flatten().map(|entry| entry.file_name()).filter(|name| name != ".lock").collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn a_download_failure_comes_back_as_it_is() {
        struct Down;
        impl Transport for Down {
            fn open(&self, _url: &str, _fetch: Fetch) -> Result<Body, HttpsFailure> {
                Err(HttpsFailure::Refused(404))
            }
        }
        let data = tempfile::tempdir().unwrap();
        let fetcher = HttpsFetcher::new(Arc::new(Down), data.path().join("downloads"));
        assert_eq!(ZipFetcher::fetch(&fetcher, URL, "update-ai-job-search-v1.8.0.zip", &mut |_| {}), Err(DownloadFailure::Refused(404)));
    }
}
