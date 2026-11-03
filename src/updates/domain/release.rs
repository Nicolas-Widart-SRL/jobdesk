use std::path::PathBuf;

use chrono::NaiveDate;

use crate::updates::domain::Version;

/// Where Jobdesk is published. Every address Jobdesk opens or downloads is built from this and a version,
/// never read from the feed or from the release notes.
pub const REPOSITORY: &str = "https://github.com/Nicolas-Widart-SRL/jobdesk";
/// The feed: the file the latest release carries beside its builds.
pub const FEED_URL: &str = "https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest/download/latest.json";

/// The builds Jobdesk publishes, and so the computers it can update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacArm,
    WindowsX64,
}

impl Platform {
    /// `None` on a computer Jobdesk publishes no build for.
    pub fn current() -> Option<Platform> {
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Platform::MacArm)
        } else if cfg!(all(windows, target_arch = "x86_64")) {
            Some(Platform::WindowsX64)
        } else {
            None
        }
    }

    /// The key of this build in the feed.
    pub fn feed_key(self) -> &'static str {
        match self {
            Platform::MacArm => "darwin-aarch64",
            Platform::WindowsX64 => "windows-x86_64",
        }
    }

    pub fn asset_name(self, version: &Version) -> String {
        match self {
            Platform::MacArm => format!("Jobdesk-{}-macos-arm64.app.tar.gz", version.tag()),
            Platform::WindowsX64 => format!("Jobdesk-{}-windows-x64-setup.exe", version.tag()),
        }
    }
}

/// The page of a release on GitHub.
pub fn release_page(version: &Version) -> String {
    format!("{REPOSITORY}/releases/tag/{}", version.tag())
}

/// The build of a release for a platform.
pub fn asset_url(version: &Version, platform: Platform) -> String {
    format!("{REPOSITORY}/releases/download/{}/{}", version.tag(), platform.asset_name(version))
}

/// What the feed says about the latest release, for this computer's build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub released: Option<NaiveDate>,
    /// Markdown from the network: untrusted, so it is only ever drawn as text.
    pub notes: String,
    /// The size of the build in bytes, which the download may not exceed.
    pub size: u64,
    /// The signature of the build. The feed holds it and the size, and no address.
    pub signature: String,
}

/// Why a check or a download did not finish. Jobdesk says each in its own words, and no server text reaches the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    TimedOut,
    Unreachable,
    /// GitHub answered with this status code.
    Refused(u16),
    BadFeed,
    /// The release has no build for this computer.
    NoBuild,
    /// The download grew past the size the feed gave.
    TooLarge,
    Interrupted,
    Disk,
}

/// How a download ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadEnd {
    /// The file is downloaded and its signature checked.
    Done(PathBuf),
    /// It stopped at this share of the size.
    Failed {
        percent: u8,
        failure: Failure,
    },
    /// The signature did not match. The file is deleted and never applied.
    BadSignature,
    Cancelled,
}

/// Why this copy cannot replace itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallBlock {
    /// The folder it is installed in is not writable.
    ReadOnly,
    /// It is a development build or was not put in place by an installer.
    NotInstalled,
    /// The build carries no update key, so a download cannot be checked.
    NoKey,
}

/// What Restart to update applies: the file of this version, and the signature of this release.
/// The signature is carried so the apply step can check the file once more against the release it was downloaded for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartPlan {
    pub version: Version,
    pub file: PathBuf,
    pub signature: String,
}

/// The number of one download. Each download started gets a new one, and every event of it carries it back,
/// so what a cancelled download still reports late is never taken for the next one's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DownloadId(pub u64);

/// What the service reports back, in the order it happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceEvent {
    Checked(Result<Release, Failure>),
    Progress { id: DownloadId, done: u64, total: u64 },
    Ended { id: DownloadId, end: DownloadEnd },
}

/// The network and the disk side of an update. Each call returns at once, and the answers come through `poll`.
pub trait UpdateService: Send {
    /// Reads the feed.
    fn check(&mut self);
    /// Downloads the build of `release` from the address built from its version, and checks its signature.
    /// `id` is the number of this download: every `Progress` and `Ended` it reports must carry it back unchanged.
    /// Events of an older download, which a cancel left running, carry the older number and are ignored.
    fn download(&mut self, release: &Release, id: DownloadId);
    /// Stops the download and deletes what it wrote, a finished file too.
    fn cancel(&mut self);
    /// The next thing that happened, if anything did.
    fn poll(&mut self) -> Option<ServiceEvent>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v020() -> Version {
        Version { major: 0, minor: 2, patch: 0 }
    }

    #[test]
    fn the_feed_and_release_addresses_are_exact() {
        assert_eq!(FEED_URL, "https://github.com/Nicolas-Widart-SRL/jobdesk/releases/latest/download/latest.json");
        assert_eq!(FEED_URL, format!("{REPOSITORY}/releases/latest/download/latest.json"));
        assert_eq!(release_page(&v020()), "https://github.com/Nicolas-Widart-SRL/jobdesk/releases/tag/v0.2.0");
    }

    #[test]
    fn the_mac_build_has_its_name_and_address() {
        assert_eq!(Platform::MacArm.feed_key(), "darwin-aarch64");
        assert_eq!(Platform::MacArm.asset_name(&v020()), "Jobdesk-v0.2.0-macos-arm64.app.tar.gz");
        assert_eq!(
            asset_url(&v020(), Platform::MacArm),
            "https://github.com/Nicolas-Widart-SRL/jobdesk/releases/download/v0.2.0/Jobdesk-v0.2.0-macos-arm64.app.tar.gz"
        );
    }

    #[test]
    fn the_windows_build_has_its_name_and_address() {
        assert_eq!(Platform::WindowsX64.feed_key(), "windows-x86_64");
        assert_eq!(Platform::WindowsX64.asset_name(&v020()), "Jobdesk-v0.2.0-windows-x64-setup.exe");
        assert_eq!(
            asset_url(&v020(), Platform::WindowsX64),
            "https://github.com/Nicolas-Widart-SRL/jobdesk/releases/download/v0.2.0/Jobdesk-v0.2.0-windows-x64-setup.exe"
        );
    }
}
