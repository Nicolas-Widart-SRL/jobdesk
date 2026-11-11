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

/// Where `ai-job-search` is published, as `owner/name` on GitHub. The address of its source is built from this and a
/// version tag, never read from the feed.
pub const AI_JOB_SEARCH_REPOSITORY: &str = "ai-job-search-dev/ai-job-search";

/// The `ai-job-search` release this copy of Jobdesk vouches for: its version, and the SHA-256 of its source zip.
/// It comes from the feed only once the update key's signature of both checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePin {
    pub version: Version,
    /// 64 lowercase hex digits, as `sha256sum` writes them.
    pub sha256: String,
}

impl SourcePin {
    /// A pin of `version` and `sha256`, when the version is exactly three numbers and the hash is 64 lowercase hex digits.
    pub fn parse(version: &str, sha256: &str) -> Option<SourcePin> {
        let version = Version::parse(version)?;
        let hex = sha256.len() == 64 && sha256.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
        hex.then(|| SourcePin { version, sha256: sha256.to_owned() })
    }

    /// The text the release's `sign` job signs, as `sha256sum` writes it: the hash, two spaces, the zip's name and a
    /// line end, such as `<sha256>  ai-job-search-v1.4.0.zip\n`.
    pub fn signed_text(&self) -> String {
        format!("{}  {}\n", self.sha256, source_zip_name(&self.version))
    }
}

/// The name of a release's source zip, such as `ai-job-search-v1.4.0.zip`. The pin's signature names it.
pub fn source_zip_name(version: &Version) -> String {
    format!("ai-job-search-{}.zip", version.tag())
}

/// The source zip of a release: GitHub's fixed address for the zip of that tag.
pub fn source_zip_url(version: &Version) -> String {
    format!("https://codeload.github.com/{AI_JOB_SEARCH_REPOSITORY}/zip/refs/tags/{}", version.tag())
}

/// Reads the `ai-job-search` release the feed pins.
pub trait SourcePinReader: Send + Sync {
    /// Reads the feed, so it blocks: call it away from the window's thread. A feed that cannot be reached or read
    /// says why. `Ok(None)` is a feed with no pin this copy can trust: none, one that does not read, or one whose
    /// signature fails. A build with no update key always gets `Ok(None)`, and reads nothing.
    fn read(&self) -> Result<Option<SourcePin>, Failure>;
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
    /// The `ai-job-search` releases the same read of the feed pins, each once the update key's signature of it checks,
    /// newest first. At most `COMPATIBLE_LIMIT`, and none twice. Empty for a build with no update key, and when no
    /// entry is there, reads or passes its signature. Jobdesk's own update never reads it.
    pub pins: Vec<SignedPin>,
}

/// The most `ai-job-search` releases Jobdesk reads from the feed, and so the most it supports at once.
pub const COMPATIBLE_LIMIT: usize = 5;

/// A pin whose signature the update key checked. Only the read of the feed in the updates feature makes one, so outside
/// it a `Release` can carry no pin but the one that read gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedPin(SourcePin);

impl SignedPin {
    /// `pin`, which the update key's signature has just checked.
    pub(in crate::updates) fn checked(pin: SourcePin) -> Self {
        SignedPin(pin)
    }

    pub fn pin(&self) -> &SourcePin {
        &self.0
    }
}

/// What the last check that read the feed found, for a folder update: the latest Jobdesk version, and the
/// `ai-job-search` releases that feed pins, newest first, each one whose signature checks. Only the updates feature
/// makes one, from a check's release, so the only pins it can hold outside tests are ones the update key checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestFeed {
    latest: Version,
    pins: Vec<SignedPin>,
}

impl LatestFeed {
    /// What the check that gave `release` found.
    pub(in crate::updates) fn of(release: &Release) -> Self {
        LatestFeed { latest: release.version, pins: release.pins.clone() }
    }

    /// The latest Jobdesk version the feed names.
    pub fn latest(&self) -> Version {
        self.latest
    }

    /// The newest `ai-job-search` release the feed pins, its signature checked. `None` when it pins nothing this copy
    /// trusts. First-run setup and the folder update use this one.
    pub fn pin(&self) -> Option<&SourcePin> {
        self.pins.first().map(SignedPin::pin)
    }

    /// Whether the feed's signed list holds `version`. `None` when the list is empty, so nothing can be said.
    pub fn supports(&self, version: Version) -> Option<bool> {
        if self.pins.is_empty() {
            return None;
        }
        Some(self.pins.iter().any(|signed| signed.pin().version == version))
    }
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
    /// Jobdesk publishes no build for this computer.
    NoBuild,
    /// The version of this copy cannot be read, so no release can be told newer or older. Nothing is checked or offered.
    UnreadableVersion,
}

/// What Restart to update applies: the file of this version, and the signature of this release.
/// The signature is carried so the apply step can check the file once more against the release it was downloaded for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartPlan {
    pub version: Version,
    pub file: PathBuf,
    pub signature: String,
}

/// Why Restart to update did not apply a release. The copy that ran opens again in its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyFailure {
    /// The downloaded file was gone or could not be read, so there was nothing to check or apply.
    Missing,
    /// The downloaded file no longer matched the signature of its release, so it was not opened.
    BadSignature,
    /// This copy is not where an installer put it, so there is nothing it may replace.
    NotInstalled,
    /// The new copy could not be unpacked or put in place. The old one is back where it was.
    Replace,
    /// The installer or the new copy did not start, or Jobdesk could not tell where it runs from to start again.
    Start,
}

/// The release a Restart to update did not apply, and why. Read once at the next launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyFailed {
    pub version: Version,
    pub failure: ApplyFailure,
}

/// The number of one download. Each download started gets a new one, and every event of it carries it back,
/// so what a cancelled download still reports late is never taken for the next one's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DownloadId(pub u64);

impl DownloadId {
    /// The name this download gives the file it keeps, such as `3-Jobdesk-v0.2.0-windows-x64-setup.exe`.
    /// The asset name stays at the end, so the file keeps its extension.
    pub fn file_name(self, asset: &str) -> String {
        format!("{}-{asset}", self.0)
    }

    /// The name of the file while this download still writes it.
    pub fn part_name(self, asset: &str) -> String {
        format!("{}.part", self.file_name(asset))
    }
}

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
    ///
    /// Each download owns its files. It writes to `DownloadId::part_name` and keeps the finished file under
    /// `DownloadId::file_name`, so two downloads of the same asset never share a path, and the `Done` path is its own.
    fn download(&mut self, release: &Release, id: DownloadId);
    /// Stops the download that runs, or drops the one that is ready, and deletes only the files of that download.
    /// A cancelled worker never creates, renames over or deletes a path of another download, even when it is still
    /// winding down as the next download starts.
    fn cancel(&mut self);
    /// The next thing that happened, if anything did.
    fn poll(&mut self) -> Option<ServiceEvent>;
}

/// A feed answer for tests anywhere in the crate, with a pin no key checked. No build that ships has it.
#[cfg(test)]
impl LatestFeed {
    pub fn for_tests(latest: Version, pin: Option<SourcePin>) -> Self {
        Self::for_tests_with(latest, pin.into_iter().collect())
    }

    /// As `for_tests`, with `pins` as given, newest first.
    pub fn for_tests_with(latest: Version, pins: Vec<SourcePin>) -> Self {
        LatestFeed { latest, pins: pins.into_iter().map(SignedPin).collect() }
    }
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
    fn each_download_names_its_own_files() {
        let asset = Platform::WindowsX64.asset_name(&v020());
        assert_eq!(DownloadId(3).file_name(&asset), "3-Jobdesk-v0.2.0-windows-x64-setup.exe");
        assert_eq!(DownloadId(3).part_name(&asset), "3-Jobdesk-v0.2.0-windows-x64-setup.exe.part");
        assert_ne!(DownloadId(3).file_name(&asset), DownloadId(4).file_name(&asset));
        assert_ne!(DownloadId(3).part_name(&asset), DownloadId(13).part_name(&asset));
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

    #[test]
    fn the_source_zip_has_its_name_and_fixed_address() {
        let v140 = Version { major: 1, minor: 4, patch: 0 };
        assert_eq!(source_zip_name(&v140), "ai-job-search-v1.4.0.zip");
        assert_eq!(source_zip_url(&v140), "https://codeload.github.com/ai-job-search-dev/ai-job-search/zip/refs/tags/v1.4.0");
    }

    #[test]
    fn a_pin_reads_only_a_plain_version_and_a_lowercase_hash() {
        let hash = "6550a4353ffc282750bff417f4c9a5a7b27ddcd7d2644ac301939b9b97ebd341";
        let pin = SourcePin::parse("v1.4.0", hash).unwrap();
        assert_eq!(pin, SourcePin { version: Version { major: 1, minor: 4, patch: 0 }, sha256: hash.to_owned() });
        assert_eq!(pin.signed_text(), format!("{hash}  ai-job-search-v1.4.0.zip\n"));
        for (version, sha256) in [
            ("1.4.0-beta", hash.to_owned()),
            ("1.4", hash.to_owned()),
            ("1.4.0", hash[1..].to_owned()),
            ("1.4.0", format!("{hash}0")),
            ("1.4.0", hash.to_uppercase()),
            ("1.4.0", hash.replacen('6', "g", 1)),
            ("1.4.0", String::new()),
        ] {
            assert_eq!(SourcePin::parse(version, &sha256), None, "{version} {sha256}");
        }
    }
}
