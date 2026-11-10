//! The helpers ai-job-search's commands call: Python for the tracker tools, Bun for the job site searches, and LaTeX for
//! the CV and cover letter PDFs. First-run setup installs a missing one in Jobdesk's own folder of the home folder, from
//! files pinned here by version, SHA-256 and size. Every address is built from a fixed host and the pinned version,
//! never from anything a server sent.

use crate::onboarding::domain::Failure;

/// GitHub's release downloads, for Python, Bun and TinyTeX.
const GITHUB: &str = "https://github.com";
/// The TeX Live historic archive, where each year's last package set stays frozen as `tlnet-final`.
const TEX_HISTORIC: &str = "https://ftp.math.utah.edu/pub/tex/historic/systems/texlive";

/// The python-build-standalone release and the CPython it builds.
const PYTHON_BUILD: &str = "20261021";
const PYTHON: &str = "3.14.9";
/// The oldest Python ai-job-search's tools run on, as major and minor.
const PYTHON_MINIMUM: (u32, u32) = (3, 10);
const BUN: &str = "1.4.4";
/// The TinyTeX release, the last one built from TeX Live 2025, so the packages below match it.
const TINYTEX: &str = "2026.03";
const TEX_LIVE: &str = "2025";

/// The folder in the home folder that holds every helper setup installs. Jobdesk owns it, and setup writes, replaces
/// and removes nothing in the home folder outside it. In it, setup touches only the helpers' places and their staging
/// folders.
pub const OWN_FOLDER: &str = ".jobdesk";

/// A helper, in the order setup checks and installs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Helper {
    Python,
    Bun,
    Latex,
}

pub const HELPERS: [Helper; 3] = [Helper::Python, Helper::Bun, Helper::Latex];

/// The processor a build is for. TinyTeX's programs run on both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Arm64,
    X64,
}

impl Arch {
    /// The processor this copy of Jobdesk was built for, when setup can install for it.
    pub const fn of_this_build() -> Option<Self> {
        if cfg!(target_arch = "aarch64") {
            Some(Self::Arm64)
        } else if cfg!(target_arch = "x86_64") {
            Some(Self::X64)
        } else {
            None
        }
    }
}

/// How a downloaded file becomes part of a helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// A `.tar.gz` whose one top folder becomes the helper's folder.
    Tree { top: &'static str },
    /// A `.zip` that holds the helper's one program at `entry`, which goes in the helper's `bin` folder by its own name.
    Program { entry: &'static str },
    /// A TeX Live package, a `.tar.xz` whose files go under TinyTeX's `texmf-dist`, but for its `tlpkg` record.
    TexPackage,
}

/// One file setup downloads, checked against its pinned SHA-256 and size before anything of it is unpacked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub name: &'static str,
    /// The release, or for a TeX Live package its revision.
    pub version: &'static str,
    pub file: String,
    pub url: String,
    /// 64 lowercase hex digits.
    pub sha256: &'static str,
    pub size: u64,
    pub layout: Layout,
}

/// A TeX Live 2025 package the CV and cover letter templates need that TinyTeX-1 lacks, dependencies included:
/// name, the file a document loads it by, revision, SHA-256 and size.
type TexPin = (&'static str, &'static str, &'static str, &'static str, u64);

/// `moderncv` and what it needs for the CV, `textpos`, `titlesec`, `xltxtra`, `cite` and `fancyhdr` for the cover
/// letter, and the rest of the list ai-job-search's own setup guide names for a minimal TeX.
const TEX_PACKAGES: [TexPin; 20] = [
    ("academicons", "academicons.sty", "77677", "9cbaab0f350adf7b0a621bd71b332aa40c0437e0c4e036b014c8ac6bffc02dde", 260_392),
    ("arydshln", "arydshln.sty", "77677", "3a89219c00fd9a3777afc4e44e85dd4922d3362b4382dc00acdc645aab762584", 9_336),
    ("cite", "cite.sty", "77677", "9c0ab1c3c8dbd07ea09ae8a4d200f5443366330b55df8ea1ada707494a6b5df0", 20_112),
    ("colortbl", "colortbl.sty", "77677", "3fd7aef7f6daf98b465db0f76af927d9c784658a4ec3a7cc74c26ad6c341617e", 3_748),
    ("fancyhdr", "fancyhdr.sty", "78116", "9dab3331ad37957d02875715bec27aed5fc19d3778a73600f630ec28169b950a", 8_412),
    ("fontawesome5", "fontawesome5.sty", "77677", "1af05661507967ed1eb952187fa1bcd59f1d7e23d9aa8e4362bfde27570c1143", 866_332),
    ("fontawesome6", "fontawesome6.sty", "77677", "3955b84440434fd6c460870a7d811f06e9f0a5f04a9cfbc968194dcc3608a951", 1_880_124),
    ("fp", "fp.sty", "77677", "382b3166fcc643ca93881a133ad7b3208d4d120b487e02427ecda156777cf601", 19_240),
    ("import", "import.sty", "77677", "b40cf8e120d18f8955b6cfdcb16225b504e84a5f4a235ac645abc2953160ed03", 2_720),
    ("metalogo", "metalogo.sty", "77677", "1fbcbaf80f992a8afe0385dc283d49d073bd91c52972b06dc15b1373a3413a51", 2_224),
    ("microtype", "microtype.sty", "78231", "57b380b27006b34c5cc8fe20a0ceb0e3da848f397c5607b40f2c270055197ebe", 59_200),
    ("moderncv", "moderncv.cls", "77552", "f897a29679d0412d1e0a137bce4257241182211390e03afe1944da2b40f04a0c", 39_544),
    ("multirow", "multirow.sty", "77677", "c1afe8c9d424ae96d31f73b80a8a9c229a76dae90e2a159d944ddd48008c6662", 2_844),
    ("needspace", "needspace.sty", "77677", "49858007eddb001bd1741b01660019fa03da5f75ef8346f9547f450127ca12be", 956),
    ("pgf", "pgf.sty", "78116", "845efd98fcf700c803a2ed68f9c6779080ed4d00a1d74c53877e8480447e7e74", 718_196),
    ("realscripts", "realscripts.sty", "77677", "d00173260719dad26a84fcb6981018f4be6a5d566b0107062110e657c9afd440", 1_892),
    ("setspace", "setspace.sty", "77677", "53bffec9151d0c223fdfec937f0b03b0432388a1bc4a3a704fa1e60ccd0e3eb3", 7_372),
    ("textpos", "textpos.sty", "77677", "0948cc29ff8bcea56667362073c96c8ae381c4d719ede44690885fbb24439e36", 4_240),
    ("titlesec", "titlesec.sty", "77677", "5af4e3f6c2f677a65de1b81c9c3707d0117de56d4d596fbcb4734f6ceb3908ac", 19_920),
    ("xltxtra", "xltxtra.sty", "77677", "218c816c48c9925b6e3965ed8a3b4410c5ece99537f20739c0d49f47ce0e39ad", 2_532),
];

/// The program beside a TeX engine that tells whether that TeX finds a file.
const TEX_FILE_FINDER: &str = "kpsewhich";

impl Helper {
    /// The programs that must run for the helper to count as here.
    pub const fn programs(self) -> &'static [&'static str] {
        match self {
            Self::Python => &["python3"],
            Self::Bun => &["bun"],
            Self::Latex => &["lualatex", "xelatex"],
        }
    }

    /// Whether `line`, the first line one of the helper's programs printed for `--version`, names one setup can use:
    /// any, but for Python one at least `PYTHON_MINIMUM`.
    pub fn accepts(self, line: &str) -> bool {
        self != Self::Python || python_new_enough(line)
    }

    /// The files the helper must find for ai-job-search's commands to work, with the program, beside each of its
    /// programs, that looks them up: for LaTeX, `kpsewhich` and the file each pinned TeX Live package is loaded by, so a
    /// smaller TeX, such as BasicTeX, does not count as here. `None` for a helper that needs no file.
    pub fn needed_files(self) -> Option<(&'static str, Vec<&'static str>)> {
        (self == Self::Latex).then(|| (TEX_FILE_FINDER, TEX_PACKAGES.iter().map(|&(_, file, ..)| file).collect()))
    }

    /// The helper's folder in `OWN_FOLDER`.
    pub const fn place(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Bun => "bun",
            Self::Latex => "TinyTeX",
        }
    }

    /// The folder of the helper's programs, under its place. `HELPER_BINS` in the shared kernel lists the same folders.
    pub const fn bin(self) -> &'static [&'static str] {
        match self {
            Self::Python | Self::Bun => &["bin"],
            Self::Latex => &["bin", "universal-darwin"],
        }
    }

    /// The files setup downloads for the helper on `arch`, in the order it unpacks them.
    pub fn artifacts(self, arch: Arch) -> Vec<Artifact> {
        match self {
            Self::Python => vec![python(arch)],
            Self::Bun => vec![bun(arch)],
            Self::Latex => std::iter::once(tinytex()).chain(TEX_PACKAGES.iter().map(tex_package)).collect(),
        }
    }

    /// What setup downloads for the helper, in bytes.
    pub fn download_size(self, arch: Arch) -> u64 {
        self.artifacts(arch).iter().map(|artifact| artifact.size).sum()
    }

    /// About what the helper takes on disk once installed, in bytes, as measured on the pinned files.
    pub const fn disk_size(self, arch: Arch) -> u64 {
        match (self, arch) {
            (Self::Python, Arch::Arm64) => 69_210_793,
            (Self::Python, Arch::X64) => 70_477_082,
            (Self::Bun, Arch::Arm64) => 88_660_864,
            (Self::Bun, Arch::X64) => 98_168_288,
            (Self::Latex, _) => 238_211_333,
        }
    }

    /// About how long the install takes, in seconds, unpacking included: on a connection of about 3 MB/s, and on a
    /// slow one of about 0.5 MB/s. Measured at about 10 MB/s, where it took 4, 4 and 25 seconds.
    pub const fn seconds(self) -> (u32, u32) {
        match self {
            Self::Python => (10, 60),
            Self::Bun => (15, 90),
            Self::Latex => (45, 210),
        }
    }
}

fn python(arch: Arch) -> Artifact {
    let (triple, sha256, size) = match arch {
        Arch::Arm64 => ("aarch64-apple-darwin", "226d696642a8a53d03b4d5b255b1c319b1575a6d44bd723a953c9f507755e36a", 26_834_117),
        Arch::X64 => ("x86_64-apple-darwin", "600cf336dd007e7c72318214fd0708a56c555b516964175df4a748e4ff0487a2", 26_931_969),
    };
    let file = format!("cpython-{PYTHON}+{PYTHON_BUILD}-{triple}-install_only.tar.gz");
    let url = format!("{GITHUB}/astral-sh/python-build-standalone/releases/download/{PYTHON_BUILD}/{file}");
    Artifact { name: "python", version: PYTHON, file, url, sha256, size, layout: Layout::Tree { top: "python" } }
}

/// Intel Macs get Bun's baseline build, which runs on processors without AVX2 and under Rosetta.
fn bun(arch: Arch) -> Artifact {
    let (build, entry, sha256, size) = match arch {
        Arch::Arm64 => ("bun-darwin-aarch64", "bun-darwin-aarch64/bun", "c240d7f0ccb90c263a849fcdb09b7ee62630d921348ed6aafda86b9d577bc510", 40_323_673),
        Arch::X64 => ("bun-darwin-x64-baseline", "bun-darwin-x64-baseline/bun", "d4a155adfbfd09155eb3ba05c9e3a544f39297d7a68616d3c7759f67ab5c47dc", 43_442_607),
    };
    let file = format!("{build}.zip");
    let url = format!("{GITHUB}/oven-sh/bun/releases/download/bun-v{BUN}/{file}");
    Artifact { name: "bun", version: BUN, file, url, sha256, size, layout: Layout::Program { entry } }
}

/// TinyTeX-1, whose macOS programs run on both processors.
fn tinytex() -> Artifact {
    let file = format!("TinyTeX-1-v{TINYTEX}.tgz");
    let url = format!("{GITHUB}/rstudio/tinytex-releases/releases/download/v{TINYTEX}/{file}");
    let sha256 = "ca4a50677dec9113eddcb5fb555fbde38c280084ee26ff2b3081a007326652fd";
    Artifact { name: "tinytex", version: TINYTEX, file, url, sha256, size: 92_059_502, layout: Layout::Tree { top: "TinyTeX" } }
}

fn tex_package(&(name, _, revision, sha256, size): &TexPin) -> Artifact {
    let file = format!("{name}.tar.xz");
    let url = format!("{TEX_HISTORIC}/{TEX_LIVE}/tlnet-final/archive/{file}");
    Artifact { name, version: revision, file, url, sha256, size, layout: Layout::TexPackage }
}

/// Whether `line`, what `python3 --version` printed, names a Python at least `PYTHON_MINIMUM`.
fn python_new_enough(line: &str) -> bool {
    let Some(version) = line.trim().strip_prefix("Python ") else { return false };
    let mut parts = version.split('.').map(|part| part.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<u32>());
    match (parts.next(), parts.next()) {
        (Some(Ok(major)), Some(Ok(minor))) => (major, minor) >= PYTHON_MINIMUM,
        _ => false,
    }
}

/// Where an install has got to. The bytes count over all of a helper's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPhase {
    Downloading {
        received: u64,
        total: u64,
    },
    /// Every file is compared with its pinned SHA-256 and size, before anything is unpacked.
    Checking,
    /// The files are unpacked beside the helper's place, then put there in one step.
    Unpacking,
}

/// How a helper ended, one tag on its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    AlreadyHere,
    Installed,
    CouldNotInstall(Failure),
}

/// Finds whether a helper works for ai-job-search.
pub trait HelperProbe: Send + Sync {
    /// Whether every program of `helper` runs, Python is new enough, and the TeX of each LaTeX program finds every
    /// file the templates need. A check that cannot tell, such as one that takes too long, counts as not here. It never
    /// runs a stub that would open a dialog.
    fn is_here(&self, helper: Helper) -> bool;
}

/// Installs a helper in `OWN_FOLDER` of the home folder.
pub trait HelperInstaller: Send + Sync {
    /// Downloads every file of `helper`, checks each against its pin, unpacks them beside the helper's place and puts
    /// them there in one step. When the place is taken, `is_here` is asked again: another Jobdesk may have installed
    /// the helper since setup checked, and then that install stays and this one is dropped. Otherwise the place holds
    /// an earlier install of Jobdesk's own that does not work, which is renamed aside, then removed once the new one
    /// is in. It blocks, and stops soon after `cancelled` turns true. Whatever ends it short leaves nothing new in the
    /// home folder.
    fn install(&self, helper: Helper, is_here: &dyn Fn() -> bool, cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(InstallPhase)) -> Result<(), Failure>;
    /// Removes the half installs and downloads a Jobdesk that no longer runs left. Links are removed, never followed.
    fn clear_leftovers(&self);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::host_path::HELPER_BINS;

    #[test]
    fn every_address_is_built_from_a_fixed_host_and_the_version() {
        for arch in [Arch::Arm64, Arch::X64] {
            for helper in HELPERS {
                for artifact in helper.artifacts(arch) {
                    assert!(artifact.url.starts_with(GITHUB) || artifact.url.starts_with(TEX_HISTORIC), "{}", artifact.url);
                    assert!(artifact.url.starts_with("https://"), "{}", artifact.url);
                    assert!(artifact.url.ends_with(&format!("/{}", artifact.file)), "{}", artifact.url);
                    assert!(artifact.url.contains(artifact.version) || artifact.layout == Layout::TexPackage, "{}", artifact.url);
                    assert_eq!(artifact.sha256.len(), 64);
                    assert!(artifact.sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
                    assert!(artifact.size > 0);
                }
            }
        }
        assert_eq!(tex_package(&TEX_PACKAGES[11]).url, "https://ftp.math.utah.edu/pub/tex/historic/systems/texlive/2025/tlnet-final/archive/moderncv.tar.xz");
        assert_eq!(
            python(Arch::Arm64).url,
            "https://github.com/astral-sh/python-build-standalone/releases/download/20261021/cpython-3.14.9+20261021-aarch64-apple-darwin-install_only.tar.gz"
        );
        assert_eq!(bun(Arch::X64).url, "https://github.com/oven-sh/bun/releases/download/bun-v1.4.4/bun-darwin-x64-baseline.zip");
        assert_eq!(tinytex().url, "https://github.com/rstudio/tinytex-releases/releases/download/v2026.03/TinyTeX-1-v2026.03.tgz");
    }

    #[test]
    fn each_helper_has_its_files_and_latex_gets_tinytex_first() {
        let latex = Helper::Latex.artifacts(Arch::Arm64);
        assert_eq!(latex.len(), 21);
        assert_eq!(latex[0].layout, Layout::Tree { top: "TinyTeX" });
        assert!(latex[1..].iter().all(|artifact| artifact.layout == Layout::TexPackage));
        assert_eq!(Helper::Latex.artifacts(Arch::X64), latex, "TinyTeX's programs run on both processors");
        assert_ne!(Helper::Python.artifacts(Arch::Arm64), Helper::Python.artifacts(Arch::X64));
        assert_eq!(Helper::Latex.download_size(Arch::Arm64), 92_059_502 + 3_929_336);
    }

    #[test]
    fn latex_needs_a_file_of_each_pinned_package() {
        let (finder, files) = Helper::Latex.needed_files().unwrap();
        assert_eq!(finder, "kpsewhich");
        assert_eq!(files.len(), TEX_PACKAGES.len());
        assert!(files.contains(&"moderncv.cls") && files.contains(&"fontawesome5.sty"));
        for (&(name, file, ..), needed) in TEX_PACKAGES.iter().zip(&files) {
            assert_eq!(*needed, file);
            assert!(file.starts_with(name) && (file.ends_with(".sty") || file.ends_with(".cls")), "{file}");
        }
        assert_eq!(Helper::Python.needed_files(), None);
        assert_eq!(Helper::Bun.needed_files(), None);
    }

    #[test]
    fn every_helper_goes_in_jobdesks_own_folder_whose_program_folders_the_path_lists() {
        for (helper, bins) in HELPERS.into_iter().zip(HELPER_BINS) {
            let folder: Vec<&str> = [OWN_FOLDER, helper.place()].into_iter().chain(helper.bin().iter().copied()).collect();
            assert_eq!(folder, bins, "{helper:?}");
        }
    }

    #[test]
    fn python_must_be_new_enough() {
        assert!(python_new_enough("Python 3.14.9"));
        assert!(python_new_enough("Python 3.10.0rc1"));
        assert!(python_new_enough("Python 4.0.0"));
        assert!(!python_new_enough("Python 3.9.6"), "the Command Line Tools' Python is too old");
        assert!(!python_new_enough("Python 2.7.18"));
        assert!(!python_new_enough("python3: command not found"));
        assert!(!python_new_enough(""));
    }
}
