//! The ports of a folder update over the disk, and over the one https client that setup downloads the release with.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};

use crate::folder_update::domain::{AloneLook, Base, BaseReader, FolderProbe, FolderUpdater, Report, Reverted, Update, UpdateError, ZipFetcher};
use crate::folder_update::infrastructure::disk::is_real_dir;
use crate::folder_update::infrastructure::hold::{Open, hold_alone, hold_open};
use crate::folder_update::infrastructure::updater;
use crate::shared_kernel::FolderWait;
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
    /// Why the last try could not hold the folder, if it could not: another Jobdesk held it alone, or its lock file did
    /// not open or lock.
    kept_out: Option<FolderWait>,
}

impl Opened {
    /// Tries to hold the open folder, and says why this Jobdesk is kept out, if it is.
    fn hold(&mut self) -> Option<FolderWait> {
        let folder = self.folder.as_ref()?;
        (self.hold, self.kept_out) = match hold_open(folder) {
            Open::Held(file) => (Some(file), None),
            Open::KeptOut(wait) => (None, Some(wait)),
            Open::Nothing => (None, None),
        };
        self.kept_out
    }
}

/// Takes this Jobdesk's hold on its open folder back when it drops, as `DiskFolder::alone` ends: after the job, and also
/// when the job panics, since the drop runs as the panic unwinds. When the hold cannot be taken back, `Opened::hold`
/// leaves this Jobdesk kept out, so every writer waits and the hold is tried again.
struct TakeBack<'a> {
    open: MutexGuard<'a, Opened>,
    held: bool,
}

impl Drop for TakeBack<'_> {
    fn drop(&mut self) {
        if self.held {
            self.open.hold();
        }
    }
}

impl DiskFolder {
    /// The open folder and its hold, unless a worker of this Jobdesk's own holds them now, which this never waits on. A
    /// lock poisoned by a panic is used as it is: `TakeBack` took the hold back as the panic unwound.
    fn try_open(&self) -> Option<MutexGuard<'_, Opened>> {
        match self.open.try_lock() {
            Ok(open) => Some(open),
            Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    /// `alone_possible`, with `gap` run while this Jobdesk's hold is let go of, as another Jobdesk could act then: a
    /// test's seam. What taking the hold back found is read back, so the caller knows when it is kept out.
    fn alone_possible_with(&self, folder: &Path, gap: impl FnOnce()) -> AloneLook {
        let Some(mut open) = self.try_open() else { return AloneLook { alone: false, kept_out: None } };
        let held = open.folder.as_deref() == Some(folder);
        if held {
            open.hold = None;
        }
        // Takes the hold back even if what follows panics.
        let mut take_back = TakeBack { open, held };
        // The lock alone is let go of at once, before the hold is taken back.
        let alone = hold_alone(&folder.join(installed_source::DIR)).is_ok();
        gap();
        let kept_out = if held { take_back.open.hold() } else { None };
        take_back.held = false;
        AloneLook { alone, kept_out }
    }

    /// Runs `job` on `folder` with this Jobdesk's own hold on it let go, so the job can hold the folder alone, then
    /// takes the hold again. The hold is let go of only while `job` runs, and taken again even when the job failed or
    /// panicked, or when it could not be taken at the open because another Jobdesk held the folder alone then.
    fn alone<T>(&self, folder: &Path, job: impl FnOnce() -> T) -> T {
        // A panic in an earlier job poisoned the lock: what it guards is whole, as `TakeBack` took the hold back.
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let held = open.folder.as_deref() == Some(folder);
        if held {
            open.hold = None;
        }
        let _take_back = TakeBack { open, held };
        job()
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
    fn open(&self, folder: &Path) -> Option<FolderWait> {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        // The folder held before is let go of first: it may be this one.
        *open = Opened { folder: Some(folder.to_owned()), ..Opened::default() };
        open.hold()
    }

    fn hold_again(&self, folder: &Path) -> Option<FolderWait> {
        let mut open = self.try_open()?;
        if open.folder.as_deref() != Some(folder) || open.kept_out.is_none() {
            return None;
        }
        open.hold()
    }

    fn alone_possible(&self, folder: &Path) -> AloneLook {
        self.alone_possible_with(folder, || {})
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
        // Setup's cleanup in another Jobdesk may have deleted this one's folder since the last download.
        self.reclaim().map_err(|_| DownloadFailure::Disk)?;
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
        assert_eq!(disk.open(folder.path()), None, "never kept out");
        assert_eq!(disk.hold_again(folder.path()), None);
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
        assert_eq!(disk.open(folder.path()), Some(FolderWait::Updating), "kept out at the open, while the other updates it");
        assert_eq!(disk.hold_again(folder.path()), Some(FolderWait::Updating), "and while the other holds it");
        assert_eq!(disk.hold_again(&folder.path().join("another")), None, "of the open folder only");
        drop(other);
        assert_eq!(disk.hold_again(folder.path()), None, "the hold is taken");
        let other = fs::File::options().read(true).write(true).open(&lock).unwrap();
        assert!(other.try_lock().is_err(), "this Jobdesk holds it open now");
        assert_eq!(disk.hold_again(folder.path()), None);
    }

    /// A lock file that cannot be opened now, here for its permissions, keeps this Jobdesk out and retrying, as
    /// another Jobdesk's hold does: it never leaves the folder open with no hold.
    #[cfg(unix)]
    #[test]
    fn a_lock_file_that_does_not_open_keeps_this_jobdesk_out_until_it_does() {
        use std::os::unix::fs::PermissionsExt;
        let folder = set_up();
        let lock = folder.path().join(installed_source::DIR).join(".open");
        fs::write(&lock, b"").unwrap();
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o000)).unwrap();
        let disk = DiskFolder::default();
        assert_eq!(disk.open(folder.path()), Some(FolderWait::Unlockable), "kept out, though nothing updates it");
        assert_eq!(disk.hold_again(folder.path()), Some(FolderWait::Unlockable), "and still while it does not open");
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(disk.hold_again(folder.path()), None, "held once it opens");
        let other = fs::File::options().read(true).write(true).open(&lock).unwrap();
        assert!(other.try_lock().is_err(), "this Jobdesk holds the folder open");
    }

    /// The look whether a folder can be held alone lets go of this Jobdesk's hold for a moment. Another Jobdesk that
    /// takes the folder alone then, as a second handle does in the gap here, keeps this one out, and the look says so.
    #[test]
    fn a_look_that_cannot_take_the_hold_back_says_it_is_kept_out() {
        let folder = set_up();
        let lock = folder.path().join(installed_source::DIR).join(".open");
        let disk = DiskFolder::default();
        assert_eq!(disk.open(folder.path()), None);
        assert_eq!(disk.alone_possible(folder.path()), AloneLook { alone: true, kept_out: None }, "held again after a plain look");

        let other = fs::File::options().read(true).write(true).open(&lock).unwrap();
        let look = disk.alone_possible_with(folder.path(), || other.lock().unwrap());
        assert_eq!(look, AloneLook { alone: true, kept_out: Some(FolderWait::Updating) });
        assert_eq!(disk.hold_again(folder.path()), Some(FolderWait::Updating), "and stays kept out while the other holds it");
        other.unlock().unwrap();
        assert_eq!(disk.hold_again(folder.path()), None, "held once the other lets go");
    }

    /// A job that panics, as the zip crate or the planner might on bytes a run's tools wrote, still leaves this Jobdesk
    /// holding the folder open, or kept out and trying again when another Jobdesk took it meanwhile.
    #[test]
    fn a_job_that_panics_leaves_the_folder_held_or_kept_out() {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        let folder = set_up();
        let lock = folder.path().join(installed_source::DIR).join(".open");
        let disk = DiskFolder::default();
        assert_eq!(disk.open(folder.path()), None);
        let panicked = catch_unwind(AssertUnwindSafe(|| disk.alone(folder.path(), || panic!("a fictional panic in the job"))));
        assert!(panicked.is_err());
        let other = fs::File::options().read(true).write(true).open(&lock).unwrap();
        assert!(other.try_lock().is_err(), "this Jobdesk holds the folder open again");
        assert_eq!(disk.hold_again(folder.path()), None, "and is not kept out");

        // Another Jobdesk takes the folder alone while the job runs, and the job panics.
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            disk.alone(folder.path(), || {
                other.lock().unwrap();
                panic!("a fictional panic in the job")
            })
        }));
        assert!(panicked.is_err());
        assert_eq!(disk.hold_again(folder.path()), Some(FolderWait::Updating), "kept out, and trying again");
        other.unlock().unwrap();
        assert_eq!(disk.hold_again(folder.path()), None, "the hold is taken once the other lets go");
        assert!(other.try_lock().is_err());
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

    /// Setup's cleanup in another Jobdesk can delete this one's download folder in the moment before it is locked.
    /// The next download makes and locks it again, so one deletion never fails every later download.
    #[test]
    fn a_download_folder_deleted_since_the_last_download_is_made_again() {
        let data = tempfile::tempdir().unwrap();
        let fetcher = HttpsFetcher::new(Arc::new(Serves(b"zip".to_vec())), data.path().join("downloads"));
        ZipFetcher::fetch(&fetcher, URL, "update-ai-job-search-v1.8.0.zip", &mut |_| {}).unwrap();
        let own = data.path().join("downloads").join(std::process::id().to_string());
        fs::remove_dir_all(&own).unwrap();
        let zip = ZipFetcher::fetch(&fetcher, URL, "update-ai-job-search-v1.8.0.zip", &mut |_| {});
        assert_eq!(zip.as_deref(), Ok(&b"zip"[..]));
        let other = fs::File::open(own.join(".lock")).unwrap();
        assert!(other.try_lock().is_err(), "the folder is this Jobdesk's again, locked");
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
