use std::path::{Path, PathBuf};

use crate::onboarding::domain::Contents;
use crate::shared_kernel::installed_source::InstalledSource;
/// The largest release zip setup downloads, which is also the largest kept one that reads.
pub use crate::shared_kernel::installed_source::MAX_ZIP;

/// The most a release may hold once unpacked, and the most entries it may have. A larger one is refused whole.
pub const MAX_UNPACKED: u64 = 100 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 20_000;

/// Where the download of ai-job-search has got to, one line each on the download screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The folder is made, and empty.
    Made,
    /// So many bytes have arrived. GitHub does not say the size first, so there is no total.
    Downloading { received: u64 },
    /// The zip is compared with the SHA-256 the signed feed pins, before anything is unpacked.
    Checking,
    /// The zip is unpacked beside the folder, then put in its place in one step.
    Unpacking,
}

/// Why setting up the folder, or installing a helper, stopped. Each leaves the folder empty and the helper's place as
/// it was, and no server text is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// No internet, a timeout, or a download or a read of the feed cut short.
    Offline,
    /// GitHub answered the feed or the download with this status code: it refused, found nothing, failed, or asks
    /// Jobdesk to wait because it was asked too often.
    Refused(u16),
    /// Jobdesk's release feed vouches for no ai-job-search release now, or what it sent does not read.
    NoPin,
    /// The download grew past `MAX_ZIP`, or past the size a helper's file is pinned at.
    TooLarge,
    /// The download is not the release the feed pins, or not the helper file Jobdesk pins. It is deleted.
    Mismatch,
    /// The archive holds an entry that could lead outside the folder, a link, two names one disk would take for the
    /// same name, or more than the limits allow. Nothing of it is unpacked.
    BadArchive,
    /// The folder holds files now, so nothing was put in it. For a helper, a folder that holds anything already takes
    /// its place, and it is left alone.
    NotEmpty,
    /// A file could not be written or read on this computer.
    Disk,
    /// The helper is in its place, but its programs do not run.
    DoesNotRun,
    /// The user pressed Cancel.
    Cancelled,
}

/// Downloads the release zip.
pub trait SourceFetcher: Send + Sync {
    /// Downloads `url` into a new part file named after `name`, in this Jobdesk's own folder of `onboarding/` in the
    /// data folder, and gives its path. It blocks, and stops soon after `cancelled` turns true. `progress` hears the
    /// bytes received at most every 100 ms. Whatever ends it short deletes the part file.
    fn fetch(&self, url: &str, name: &str, cancelled: &dyn Fn() -> bool, progress: &mut dyn FnMut(u64)) -> Result<PathBuf, Failure>;
    /// Removes the part files a Jobdesk that no longer runs left. Links are removed, never followed.
    fn clear_leftovers(&self);
}

/// Makes the chosen folder and unpacks the release into it.
pub trait Unpacker: Send + Sync {
    /// What `folder` holds now.
    fn look(&self, folder: &Path) -> Contents;
    /// Makes `folder` when it is missing. It fails with `NotEmpty` unless the folder is then empty.
    fn make(&self, folder: &Path) -> Result<(), Failure>;
    /// Checks the whole of `zip` against the SHA-256 of `source`, the release the feed pins, before anything is
    /// unpacked, then unpacks it into the empty `folder` with its top folder stripped. The zip and the record of `source`
    /// arrive with it in the folder's `.jobdesk`, the base of a later update. `phase` hears Checking and Unpacking.
    /// `zip` is deleted whatever the end.
    fn unpack(&self, zip: &Path, source: &InstalledSource, folder: &Path, cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(Phase)) -> Result<(), Failure>;
    /// The version of the release kept in `folder`'s `.jobdesk`, such as `1.7.5`, when its record and zip read whole and
    /// match each other.
    fn installed(&self, folder: &Path) -> Option<String>;
    /// Removes the hidden staging folders beside `folder` that a Jobdesk which no longer runs left. Links are removed,
    /// never followed.
    fn clear_leftovers(&self, folder: &Path);
}
