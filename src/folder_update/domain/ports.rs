//! What a folder update needs from the disk and the network, so the application layer decides when an update runs
//! without touching either, and its tests use fakes.
//!
//! Every method that writes the folder, and `recover`, must run only while no task and no Claude session is open. The
//! application layer refuses to call them otherwise, and its caller keeps no run from starting while one works.

use std::path::Path;

use crate::folder_update::domain::{Report, Reverted, Update, UpdateError};
use crate::shared_kernel::FolderWait;
use crate::shared_kernel::download::DownloadFailure;
use crate::shared_kernel::installed_source::InstalledSource;

/// The release a job search folder was set up from or last updated to, as its `.jobdesk/` holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base {
    /// The folder has no `.jobdesk`, or one that holds neither the kept zip nor its record: it was set up before
    /// Jobdesk kept one, or it is a folder the user already had.
    Missing,
    /// The kept zip or its record is there but does not read: one of them is missing, a link, too large, or the zip's
    /// SHA-256 is not the one its record holds.
    Bad,
    /// The record and the zip, whose SHA-256 matches it.
    Found { source: InstalledSource, zip: Vec<u8> },
}

/// What a folder holds, as far as the gate asks.
pub trait FolderProbe: Send + Sync {
    /// Whether `folder` is a folder.
    fn exists(&self, folder: &Path) -> bool;
    /// Whether `folder` holds a `.git` entry of any kind: a folder, a file, or a link. One that cannot be looked at
    /// counts as there, so a folder that may be under git is never updated.
    fn has_git(&self, folder: &Path) -> bool;
}

/// What a look whether a folder can be held alone found: see `FolderUpdater::alone_possible`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AloneLook {
    /// No other Jobdesk had the folder open.
    pub alone: bool,
    /// Why this Jobdesk is kept out once it took its hold back, if it is.
    pub kept_out: Option<FolderWait>,
}

/// Reads the base of a folder update.
pub trait BaseReader: Send + Sync {
    fn read(&self, folder: &Path) -> Base;
    /// The release the user undid the last update to, as Undo noted it in the folder's `.jobdesk`: its version and the
    /// SHA-256 of its zip. `None` when nothing is noted, or the note does not read. The next update of the folder
    /// replaces the note with its own backup.
    fn skipped(&self, folder: &Path) -> Option<InstalledSource>;
}

/// Downloads the pinned release zip.
pub trait ZipFetcher: Send + Sync {
    /// Downloads `url` over https, as setup does, into a part file named after `name` in this Jobdesk's own folder of
    /// the data folder, and gives its bytes. More than the largest release setup downloads is `TooLarge`. The part file
    /// is deleted whatever the end, but for a Jobdesk that stops midway, whose part file stays until setup next opens. It
    /// blocks: call it on a worker. `progress` hears the bytes received.
    fn fetch(&self, url: &str, name: &str, progress: &mut dyn FnMut(u64)) -> Result<Vec<u8>, DownloadFailure>;
}

/// Updates, recovers and undoes a folder on the disk. `recover`, `update` and `undo` write the folder: none may run while
/// a task or a Claude session is open. Each holds the folder alone, so each is `Busy` while another Jobdesk has it open.
pub trait FolderUpdater: Send + Sync {
    /// `folder` opened in this Jobdesk: it holds the folder open, alongside any other Jobdesk that has it open, until
    /// another folder opens or Jobdesk quits, and lets go of the folder it held before. While it holds a folder, another
    /// Jobdesk's `recover`, `update` and `undo` of it are `Busy`, and its own let go of the hold while they run. A
    /// folder whose `.jobdesk` is not there is held by nothing, and nothing is made in it.
    ///
    /// Returns why it is kept out, if it is, so this one cannot hold the folder open now: `Updating` while another
    /// Jobdesk holds it alone, to update, undo or roll it back, and `Unlockable` while the lock does not open or lock for
    /// any other reason. Only a folder with no `.jobdesk` folder, where no update ever runs, is never kept out. While it
    /// is kept out, nothing may write the folder until `hold_again` takes the hold.
    fn open(&self, folder: &Path) -> Option<FolderWait>;
    /// Tries again to hold `folder` open, after `open` or the end of an update, Undo or rollback of this Jobdesk's own
    /// found it kept out. Returns why it is still kept out, if it is. `None` when `folder` is not the one open, or when
    /// this Jobdesk holds it already or could never hold it, as for a folder with no `.jobdesk`. It never waits: while
    /// this Jobdesk's own update, Undo or rollback runs, it is `None`.
    fn hold_again(&self, folder: &Path) -> Option<FolderWait>;
    /// Whether this Jobdesk could hold `folder` alone now: no other Jobdesk has it open. It lets go of its own hold for a
    /// moment and takes it back, as `recover` does, and says whether it is kept out once it has: another Jobdesk may
    /// take the folder alone in that moment. It never waits: while this Jobdesk's own update, Undo or rollback runs, it
    /// finds the folder neither free nor kept out.
    fn alone_possible(&self, folder: &Path) -> AloneLook;
    /// Rolls back an update of `folder` that stopped midway. `None` when none did.
    fn recover(&self, folder: &Path) -> Result<Option<Reverted>, UpdateError>;
    /// Updates `folder` from `update.base` to `update.new`, after the new zip matches `update.new.sha256`. Once it holds
    /// the folder alone it reads the base and Undo's note again, and changes nothing when they moved since the gate read
    /// them: `Superseded`, `Undone` or `Busy`.
    fn update(&self, folder: &Path, update: &Update) -> Result<Report, UpdateError>;
    /// Whether the last update of `folder` can be undone. It only reads.
    fn can_undo(&self, folder: &Path) -> bool;
    /// Whether an Undo of the last update of `folder` started and did not finish: it noted the release and stopped
    /// before it had put every file back, so the folder holds some of each. It only reads.
    fn undo_unfinished(&self, folder: &Path) -> bool;
    /// Undoes the last update of `folder`.
    fn undo(&self, folder: &Path) -> Result<Reverted, UpdateError>;
}
