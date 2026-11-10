//! Installs a helper in the home folder on macOS. Every file of it downloads into this Jobdesk's own folder of
//! `onboarding/`, and is compared with its pinned SHA-256 and size. Only once all of them match is anything unpacked,
//! into a hidden staging folder beside the helper's place, which then takes that place in one rename.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{OpenOptionsExt, symlink};
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};
use zip::{CompressionMethod, ZipArchive};

use crate::onboarding::domain::{Arch, Artifact, Failure, HELPERS, Helper, HelperInstaller, InstallPhase, Layout, SourceFetcher};
use crate::onboarding::infrastructure::fetcher::HttpsFetcher;
use crate::onboarding::infrastructure::unpacker::{clear_stagings, file_mode, make_folders, remove, rename_into, sha256_hex, staging_of};
use crate::shared_kernel::own_folder::claim;

/// The most one archive may unpack to, and the most entries it may hold. TinyTeX, the largest, unpacks to 227 MB in
/// 7,195 entries.
const MAX_UNPACKED: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;

/// The first line of kpathsea's file name database, without which TeX ignores the file.
const LS_R_MAGIC: &str = "% ls-R -- filename database for kpathsea; do not change this line.";

/// The pinned files of a helper.
type Pins = Box<dyn Fn(Helper) -> Vec<Artifact> + Send + Sync>;

/// Installs a helper's pinned files in `home`, downloading them over the one https client.
pub struct HomeInstaller {
    fetcher: HttpsFetcher,
    /// `<data>/onboarding`, whose `<pid>` folders say which Jobdesk still runs.
    dir: PathBuf,
    home: PathBuf,
    pins: Pins,
}

impl HomeInstaller {
    /// `fetcher` downloads into `dir`. The files are the ones pinned for `arch`.
    pub fn new(fetcher: HttpsFetcher, dir: PathBuf, home: PathBuf, arch: Arch) -> Self {
        Self { fetcher, dir, home, pins: Box::new(move |helper| helper.artifacts(arch)) }
    }

    /// The helper's place in the home folder: a folder, or for Bun its program.
    fn place(&self, helper: Helper) -> PathBuf {
        helper.place().iter().fold(self.home.clone(), |path, part| path.join(part))
    }

    /// Downloads every file, then checks each against its pin. Gives their bytes, in order. The part files are gone
    /// whatever the end.
    fn fetch_checked(&self, artifacts: &[Artifact], cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(InstallPhase)) -> Result<Vec<Vec<u8>>, Failure> {
        let mut parts = Vec::with_capacity(artifacts.len());
        let checked = self.fetch_into(artifacts, &mut parts, cancelled, phase);
        for part in &parts {
            let _ = fs::remove_file(part);
        }
        checked
    }

    fn fetch_into(
        &self,
        artifacts: &[Artifact],
        parts: &mut Vec<PathBuf>,
        cancelled: &dyn Fn() -> bool,
        phase: &mut dyn FnMut(InstallPhase),
    ) -> Result<Vec<Vec<u8>>, Failure> {
        let total = artifacts.iter().map(|artifact| artifact.size).sum();
        let mut before = 0;
        for artifact in artifacts {
            let mut heard = |received| phase(InstallPhase::Downloading { received: before + received, total });
            parts.push(self.fetcher.fetch_at_most(&artifact.url, &artifact.file, artifact.size, cancelled, &mut heard)?);
            before += artifact.size;
        }
        if cancelled() {
            return Err(Failure::Cancelled);
        }
        phase(InstallPhase::Checking);
        artifacts.iter().zip(parts.iter()).map(|(artifact, part)| checked(artifact, part)).collect()
    }
}

impl HelperInstaller for HomeInstaller {
    fn install(&self, helper: Helper, cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(InstallPhase)) -> Result<(), Failure> {
        let artifacts = (self.pins)(helper);
        let files = self.fetch_checked(&artifacts, cancelled, phase)?;
        if cancelled() {
            return Err(Failure::Cancelled);
        }
        phase(InstallPhase::Unpacking);
        let place = self.place(helper);
        fs::create_dir_all(place.parent().ok_or(Failure::Disk)?).map_err(|_| Failure::Disk)?;
        let staging = staging_of(&place, &claim(&self.dir).map_err(|_| Failure::Disk)?)?;
        remove(&staging);
        let placed = unpack_all(&artifacts, &files, &staging, cancelled)
            .and_then(|()| if cancelled() { Err(Failure::Cancelled) } else { rename_into(&staging, &place) });
        if placed.is_err() {
            remove(&staging);
        }
        placed
    }

    fn clear_leftovers(&self) {
        self.fetcher.clear_leftovers();
        for helper in HELPERS {
            clear_stagings(&self.place(helper), &self.dir);
        }
    }
}

/// The bytes of the part file, when they are the pinned file: its size, then its SHA-256.
fn checked(artifact: &Artifact, part: &Path) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    File::open(part).and_then(|file| file.take(artifact.size + 1).read_to_end(&mut bytes)).map_err(|_| Failure::Disk)?;
    if bytes.len() as u64 != artifact.size || sha256_hex(&bytes) != artifact.sha256 {
        return Err(Failure::Mismatch);
    }
    Ok(bytes)
}

/// Unpacks every checked file into `staging`, in order, then lists TinyTeX's files again for TeX when packages were
/// added to it.
fn unpack_all(artifacts: &[Artifact], files: &[Vec<u8>], staging: &Path, cancelled: &dyn Fn() -> bool) -> Result<(), Failure> {
    for (artifact, bytes) in artifacts.iter().zip(files) {
        match artifact.layout {
            Layout::Tree { top } => {
                fs::create_dir(staging).map_err(|_| Failure::Disk)?;
                let mut archive = Archive::new(GzDecoder::new(bytes.as_slice()));
                unpack_entries(&mut archive, staging, true, cancelled, &|plain| match plain.strip_prefix(top) {
                    Ok(rest) if rest.as_os_str().is_empty() => Ok(None),
                    Ok(rest) => Ok(Some(rest.to_owned())),
                    Err(_) => Err(Failure::BadArchive),
                })?;
            }
            Layout::Program { entry } => unpack_program(bytes, entry, staging)?,
            Layout::TexPackage => {
                let tar = xz_decompressed(bytes)?;
                let mut archive = Archive::new(tar.as_slice());
                // A TeX Live package is relocatable: its files go under `texmf-dist`, but for its `tlpkg` record.
                unpack_entries(&mut archive, staging, false, cancelled, &|plain| {
                    Ok(Some(if plain.starts_with("tlpkg") { plain } else { Path::new("texmf-dist").join(plain) }))
                })?;
            }
        }
    }
    if artifacts.iter().any(|artifact| artifact.layout == Layout::TexPackage) {
        write_ls_r(&staging.join("texmf-dist")).map_err(|_| Failure::Disk)?;
    }
    Ok(())
}

/// Where an entry goes below the folder it unpacks into, given its plain path: `None` for the top folder itself, which
/// is that folder.
type Spot<'a> = &'a dyn Fn(PathBuf) -> Result<Option<PathBuf>, Failure>;

/// Unpacks the entries of `archive` into `into`, each where `spot` puts it. Only folders, plain files and, when
/// `links` allows, links that stay inside `into` are taken. A path with `..` or a root, a path that passes through a
/// link, an entry written twice, a hard link, a device, more than `MAX_ENTRIES` entries or more than `MAX_UNPACKED`
/// bytes refuses the whole archive. Files keep their read and run bits, without set-id bits or group and other write.
fn unpack_entries<R: Read>(archive: &mut Archive<R>, into: &Path, links: bool, cancelled: &dyn Fn() -> bool, spot: Spot) -> Result<(), Failure> {
    archive.set_preserve_permissions(false);
    archive.set_mask(0o022);
    archive.set_unpack_xattrs(false);
    archive.set_overwrite(false);
    let mut made_links: Vec<PathBuf> = Vec::new();
    let (mut entries, mut total) = (0, 0u64);
    for entry in archive.entries().map_err(|_| Failure::BadArchive)? {
        if cancelled() {
            return Err(Failure::Cancelled);
        }
        let mut entry = entry.map_err(|_| Failure::BadArchive)?;
        let kind = entry.header().entry_type();
        if matches!(kind, EntryType::XGlobalHeader | EntryType::XHeader) {
            continue;
        }
        entries += 1;
        if entries > MAX_ENTRIES {
            return Err(Failure::BadArchive);
        }
        let plain = plain_path(&entry.path().map_err(|_| Failure::BadArchive)?).ok_or(Failure::BadArchive)?;
        let Some(relative) = spot(plain)? else { continue };
        if made_links.iter().any(|link| relative.starts_with(link)) {
            return Err(Failure::BadArchive);
        }
        match kind {
            EntryType::Directory => make_folders(into, &relative)?,
            EntryType::Regular | EntryType::Continuous => {
                total = total.saturating_add(tar::Header::size(entry.header()).map_err(|_| Failure::BadArchive)?);
                if total > MAX_UNPACKED {
                    return Err(Failure::BadArchive);
                }
                make_parents(into, &relative)?;
                entry.unpack(into.join(&relative)).map_err(|error| written(&error))?;
            }
            EntryType::Symlink if links => {
                let target = entry.link_name().map_err(|_| Failure::BadArchive)?.ok_or(Failure::BadArchive)?;
                if !stays_inside(&relative, &target) {
                    return Err(Failure::BadArchive);
                }
                make_parents(into, &relative)?;
                symlink(&target, into.join(&relative)).map_err(|error| written(&error))?;
                made_links.push(relative);
            }
            _ => return Err(Failure::BadArchive),
        }
    }
    if entries == 0 { Err(Failure::BadArchive) } else { Ok(()) }
}

/// A name already taken means the archive holds the entry twice. Anything else is the disk.
fn written(error: &io::Error) -> Failure {
    if error.kind() == io::ErrorKind::AlreadyExists { Failure::BadArchive } else { Failure::Disk }
}

fn make_parents(into: &Path, relative: &Path) -> Result<(), Failure> {
    relative.parent().map_or(Ok(()), |parent| make_folders(into, parent))
}

/// `path` with its `.` parts dropped, when it holds no `..` and no root.
fn plain_path(path: &Path) -> Option<PathBuf> {
    let mut plain = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => plain.push(name),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!plain.as_os_str().is_empty()).then_some(plain)
}

/// Whether a link at `link`, below the folder it unpacks into, points inside that folder: a relative target that
/// climbs with `..` only at its start, no higher than that folder, then names something. A `..` after a name could
/// climb out through another link, so it is refused.
fn stays_inside(link: &Path, target: &Path) -> bool {
    let depth = link.components().count().saturating_sub(1);
    let (mut climbs, mut named) = (0, 0);
    for component in target.components() {
        match component {
            Component::ParentDir if named == 0 => climbs += 1,
            Component::Normal(_) => named += 1,
            Component::CurDir => {}
            _ => return false,
        }
    }
    named > 0 && climbs <= depth
}

/// Writes the one program `entry` of a zip to `to`, with its run bits.
fn unpack_program(bytes: &[u8], entry: &str, to: &Path) -> Result<(), Failure> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| Failure::BadArchive)?;
    let mut file = archive.by_name(entry).map_err(|_| Failure::BadArchive)?;
    let regular = file.unix_mode().is_none_or(|mode| mode & 0o170_000 == 0o100_000);
    if file.is_dir() || !regular || file.encrypted() || !matches!(file.compression(), CompressionMethod::Stored | CompressionMethod::Deflated) {
        return Err(Failure::BadArchive);
    }
    let mut out = OpenOptions::new().write(true).create_new(true).mode(file_mode(file.unix_mode())).open(to).map_err(|_| Failure::Disk)?;
    let mut buffer = vec![0; 64 * 1024];
    let mut written = 0;
    loop {
        let read = match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            // A corrupt entry, or one larger than its header says.
            Err(_) => return Err(Failure::BadArchive),
        };
        written += read as u64;
        if written > MAX_UNPACKED {
            return Err(Failure::BadArchive);
        }
        out.write_all(&buffer[..read]).map_err(|_| Failure::Disk)?;
    }
    out.sync_all().map_err(|_| Failure::Disk)
}

/// The `.tar` inside an `.xz`, refused past `MAX_UNPACKED`.
fn xz_decompressed(bytes: &[u8]) -> Result<Vec<u8>, Failure> {
    let mut tar = Capped(Vec::new());
    lzma_rs::xz_decompress(&mut io::BufReader::new(bytes), &mut tar).map_err(|_| Failure::BadArchive)?;
    Ok(tar.0)
}

/// A buffer that refuses to grow past `MAX_UNPACKED`.
struct Capped(Vec<u8>);

impl Write for Capped {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.0.len() as u64 + buf.len() as u64 > MAX_UNPACKED {
            return Err(io::Error::other("too large"));
        }
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Writes `ls-R` in `texmf`, the file name database TeX looks names up in, as TeX Live's `mktexlsr` does: the magic
/// line, then for each folder a `./<path>:` line and one line per name in it. A link is listed, never followed.
fn write_ls_r(texmf: &Path) -> io::Result<()> {
    let mut out = format!("{LS_R_MAGIC}\n").into_bytes();
    let mut folders = vec![PathBuf::new()];
    while let Some(relative) = folders.pop() {
        let mut names = Vec::new();
        for entry in fs::read_dir(texmf.join(&relative))? {
            let entry = entry?;
            names.push((entry.file_name(), entry.file_type()?.is_dir()));
        }
        names.sort();
        out.extend_from_slice(b"./");
        out.extend_from_slice(relative.as_os_str().as_bytes());
        out.extend_from_slice(b":\n");
        for (name, is_dir) in names {
            out.extend_from_slice(name.as_bytes());
            out.push(b'\n');
            if is_dir {
                folders.push(relative.join(name));
            }
        }
        out.push(b'\n');
    }
    fs::write(texmf.join("ls-R"), out)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::{Builder, Header};
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;
    use crate::shared_kernel::https::{Body, Fetch, HttpsFailure, Transport};

    /// What is served for one address: its bytes, or why it cannot be had.
    type Serve = Result<Vec<u8>, HttpsFailure>;
    /// A helper's pinned files, each with what is served for it.
    type Files = (Helper, Vec<(Artifact, Serve)>);

    /// Serves each address its bytes or its failure, and refuses any other.
    struct Serves(HashMap<String, Serve>);

    impl Transport for Serves {
        fn open(&self, url: &str, fetch: Fetch) -> Result<Body, HttpsFailure> {
            assert_eq!(fetch, Fetch::Download);
            let served = self.0.get(url).cloned().unwrap_or(Err(HttpsFailure::Refused(404)));
            served.map(|bytes| Box::new(Cursor::new(bytes)) as Body)
        }
    }

    /// A pinned file whose pin is the SHA-256 and size of `bytes`, at an address of its own.
    fn pinned(name: &'static str, bytes: &[u8], layout: Layout) -> Artifact {
        let sha256 = Box::leak(sha256_hex(bytes).into_boxed_str());
        Artifact { name, version: "1", file: format!("{name}.bin"), url: format!("https://example.test/{name}"), sha256, size: bytes.len() as u64, layout }
    }

    struct Fixture {
        root: tempfile::TempDir,
        installer: HomeInstaller,
    }

    impl Fixture {
        fn home(&self) -> PathBuf {
            self.root.path().join("home")
        }

        fn dir(&self) -> PathBuf {
            self.root.path().join("data").join("onboarding")
        }

        fn install(&self, helper: Helper) -> Result<(), Failure> {
            self.installer.install(helper, &|| false, &mut |_| {})
        }

        /// Every file and link left under the home folder and in the downloads folder, but the download folder's lock.
        fn left(&self) -> Vec<String> {
            let mut found = Vec::new();
            for top in [self.home(), self.dir()] {
                walk(&top, &mut found);
            }
            found.retain(|path| !path.ends_with("/.lock"));
            found.into_iter().map(|path| path.replacen(&self.root.path().to_string_lossy().into_owned(), "", 1)).collect()
        }
    }

    fn walk(dir: &Path, found: &mut Vec<String>) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            if entry.file_type().unwrap().is_dir() {
                walk(&entry.path(), found);
            } else {
                found.push(entry.path().to_string_lossy().into_owned());
            }
        }
    }

    /// An installer over a temporary home, whose helpers' files are `pins`, each served as it says.
    fn fixture(pins: Vec<Files>) -> Fixture {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("home").join("Library")).unwrap();
        let served = pins.iter().flat_map(|(_, files)| files.iter().map(|(artifact, serve)| (artifact.url.clone(), serve.clone()))).collect();
        let table: HashMap<Helper, Vec<Artifact>> =
            pins.into_iter().map(|(helper, files)| (helper, files.into_iter().map(|(artifact, _)| artifact).collect())).collect();
        let dir = root.path().join("data").join("onboarding");
        let fetcher = HttpsFetcher::new(Arc::new(Serves(served)), dir.clone());
        let pins: Pins = Box::new(move |helper| table.get(&helper).cloned().unwrap_or_default());
        let installer = HomeInstaller { fetcher, dir, home: root.path().join("home"), pins };
        Fixture { root, installer }
    }

    #[derive(Clone, Copy)]
    enum Entry<'a> {
        Dir(&'a str),
        File(&'a str, &'a str, u32),
        Link(&'a str, &'a str),
        Hard(&'a str, &'a str),
    }

    /// A `.tar` of `entries`, their names written raw so a bad one stays as it is.
    fn tar(entries: &[Entry]) -> Vec<u8> {
        let mut builder = Builder::new(Vec::new());
        for entry in entries {
            let mut header = Header::new_old();
            let (name, kind, mode, data, link) = match *entry {
                Entry::Dir(name) => (name, EntryType::Directory, 0o755, "", None),
                Entry::File(name, data, mode) => (name, EntryType::Regular, mode, data, None),
                Entry::Link(name, target) => (name, EntryType::Symlink, 0o777, "", Some(target)),
                Entry::Hard(name, target) => (name, EntryType::Link, 0o644, "", Some(target)),
            };
            header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_entry_type(kind);
            header.set_mode(mode);
            if let Some(target) = link {
                header.as_old_mut().linkname[..target.len()].copy_from_slice(target.as_bytes());
            }
            header.set_size(data.len() as u64);
            header.set_cksum();
            builder.append(&header, data.as_bytes()).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn gz(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn xz(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        lzma_rs::xz_compress(&mut io::BufReader::new(bytes), &mut out).unwrap();
        out
    }

    fn zip(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data, mode) in entries {
            writer.start_file(*name, SimpleFileOptions::default().unix_permissions(*mode)).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    /// A fictional standalone Python: its program and the link to it.
    fn python_tree() -> Vec<u8> {
        gz(&tar(&[
            Entry::Dir("python/"),
            Entry::Dir("python/bin/"),
            Entry::File("python/bin/python3.14", "#!/bin/sh\necho Python 3.14.9\n", 0o4755),
            Entry::Link("python/bin/python3", "python3.14"),
            Entry::File("python/lib/python3.14/os.py", "", 0o664),
        ]))
    }

    /// A fictional TinyTeX: a program linked from its bin folder, its own file database, one class.
    fn tinytex_tree() -> Vec<u8> {
        gz(&tar(&[
            Entry::Dir("TinyTeX/"),
            Entry::File("TinyTeX/bin/universal-darwin/luahbtex", "lua", 0o755),
            Entry::Link("TinyTeX/bin/universal-darwin/lualatex", "luahbtex"),
            Entry::Link("TinyTeX/bin/universal-darwin/mktexlsr", "../../texmf-dist/scripts/texlive/mktexlsr"),
            Entry::File("TinyTeX/texmf-dist/scripts/texlive/mktexlsr", "#!/bin/sh\n", 0o755),
            Entry::File("TinyTeX/texmf-dist/ls-R", "old", 0o644),
            Entry::File("TinyTeX/texmf-dist/tex/latex/base/article.cls", "", 0o644),
        ]))
    }

    fn tex_package(name: &str) -> Vec<u8> {
        let class = format!("tex/latex/{name}/{name}.cls");
        let record = format!("tlpkg/tlpobj/{name}.tlpobj");
        xz(&tar(&[Entry::File(&class, "\\ProvidesClass", 0o644), Entry::File(&record, "relocated 1", 0o644)]))
    }

    fn served(name: &'static str, bytes: Vec<u8>, layout: Layout) -> (Artifact, Serve) {
        (pinned(name, &bytes, layout), Ok(bytes))
    }

    fn python() -> Files {
        (Helper::Python, vec![served("python", python_tree(), Layout::Tree { top: "python" })])
    }

    /// A TinyTeX and two packages, the second pinned as `moderncv` and served `package`.
    fn latex(package: Serve) -> Files {
        let moderncv = tex_package("moderncv");
        let (pin, _) = served("moderncv", moderncv, Layout::TexPackage);
        (
            Helper::Latex,
            vec![served("tinytex", tinytex_tree(), Layout::Tree { top: "TinyTeX" }), served("fp", tex_package("fp"), Layout::TexPackage), (pin, package)],
        )
    }

    #[test]
    fn python_lands_in_its_place_with_its_link_and_run_bits() {
        let fixture = fixture(vec![python()]);
        let mut heard = Vec::new();
        assert_eq!(fixture.installer.install(Helper::Python, &|| false, &mut |phase| heard.push(phase)), Ok(()));
        let bin = fixture.home().join(".jobdesk").join("python").join("bin");
        assert_eq!(fs::read_link(bin.join("python3")).unwrap(), Path::new("python3.14"));
        assert_eq!(fs::metadata(bin.join("python3.14")).unwrap().permissions().mode() & 0o7777, 0o755, "no set-id bit");
        let library = fixture.home().join(".jobdesk/python/lib/python3.14/os.py");
        assert_eq!(fs::metadata(library).unwrap().permissions().mode() & 0o777, 0o644, "no group write");
        assert_eq!(heard.last(), Some(&InstallPhase::Unpacking));
        assert!(heard.contains(&InstallPhase::Checking));
        let size = python_tree().len() as u64;
        assert!(heard.contains(&InstallPhase::Downloading { received: size, total: size }));
        let left = fixture.left();
        assert!(left.iter().all(|path| path.starts_with("/home/.jobdesk/python/")), "no staging folder or download is left: {left:?}");
    }

    #[test]
    fn a_hash_mismatch_installs_nothing() {
        let (pin, _) = served("python", python_tree(), Layout::Tree { top: "python" });
        let mut other = python_tree();
        other.push(0);
        let fixture = fixture(vec![(Helper::Python, vec![(pin.clone(), Ok(other))])]);
        assert_eq!(fixture.install(Helper::Python), Err(Failure::TooLarge), "a longer file stops at its pinned size");
        let mut changed = python_tree();
        *changed.last_mut().unwrap() ^= 1;
        let fixture = self::fixture(vec![(Helper::Python, vec![(pin, Ok(changed))])]);
        assert_eq!(fixture.install(Helper::Python), Err(Failure::Mismatch));
        assert!(fixture.left().is_empty(), "{:?}", fixture.left());
    }

    #[test]
    fn latex_is_tinytex_with_its_packages_and_a_new_file_database() {
        let fixture = fixture(vec![latex(Ok(tex_package("moderncv")))]);
        assert_eq!(fixture.install(Helper::Latex), Ok(()));
        let tinytex = fixture.home().join("Library").join("TinyTeX");
        let bin = tinytex.join("bin").join("universal-darwin");
        assert_eq!(fs::read_to_string(bin.join("mktexlsr")).unwrap(), "#!/bin/sh\n", "a link may climb, inside TinyTeX");
        assert!(tinytex.join("texmf-dist/tex/latex/moderncv/moderncv.cls").is_file());
        assert!(tinytex.join("tlpkg/tlpobj/moderncv.tlpobj").is_file(), "the package record stays outside texmf-dist");
        let database = fs::read_to_string(tinytex.join("texmf-dist").join("ls-R")).unwrap();
        assert!(database.starts_with(&format!("{LS_R_MAGIC}\n./:\n")), "{database}");
        assert!(database.contains("\n./tex/latex/moderncv:\nmoderncv.cls\n"), "{database}");
        assert!(database.contains("\n./tex/latex/base:\narticle.cls\n"), "{database}");
        assert!(database.contains("\n./tex/latex/fp:\nfp.cls\n"), "{database}");
    }

    #[test]
    fn one_bad_package_installs_none_of_latex() {
        let mut changed = tex_package("moderncv");
        *changed.last_mut().unwrap() ^= 1;
        let fixture = fixture(vec![latex(Ok(changed))]);
        assert_eq!(fixture.install(Helper::Latex), Err(Failure::Mismatch));
        assert_eq!(fixture.left(), Vec::<String>::new(), "TinyTeX is not put in place without its packages");
        let fixture = self::fixture(vec![latex(Err(HttpsFailure::Unreachable))]);
        assert_eq!(fixture.install(Helper::Latex), Err(Failure::Offline));
        assert_eq!(fixture.left(), Vec::<String>::new());
    }

    #[test]
    fn bun_is_one_program_that_replaces_one_that_does_not_run() {
        let entry = "bun-darwin-aarch64/bun";
        let zip = zip(&[("bun-darwin-aarch64/", b"", 0o755), (entry, b"bun", 0o755)]);
        let fixture = fixture(vec![(Helper::Bun, vec![served("bun", zip, Layout::Program { entry })])]);
        let bun = fixture.home().join(".bun").join("bin").join("bun");
        fs::create_dir_all(bun.parent().unwrap()).unwrap();
        fs::write(&bun, "broken").unwrap();
        assert_eq!(fixture.install(Helper::Bun), Ok(()));
        assert_eq!(fs::read_to_string(&bun).unwrap(), "bun");
        assert_eq!(fs::metadata(&bun).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(fixture.left(), ["/home/.bun/bin/bun"]);
    }

    #[test]
    fn a_place_holding_a_folder_jobdesk_did_not_make_is_left_alone() {
        let fixture = fixture(vec![latex(Ok(tex_package("moderncv")))]);
        let theirs = fixture.home().join("Library").join("TinyTeX").join("texmf-local").join("mine.sty");
        fs::create_dir_all(theirs.parent().unwrap()).unwrap();
        fs::write(&theirs, "theirs").unwrap();
        assert_eq!(fixture.install(Helper::Latex), Err(Failure::NotEmpty));
        assert_eq!(fs::read_to_string(&theirs).unwrap(), "theirs");
        assert_eq!(fixture.left(), ["/home/Library/TinyTeX/texmf-local/mine.sty"], "the staging folder is gone");
    }

    #[test]
    fn a_cancelled_install_leaves_nothing() {
        let fixture = fixture(vec![python()]);
        let cancelled = AtomicBool::new(false);
        let result = fixture.installer.install(Helper::Python, &|| cancelled.load(Ordering::SeqCst), &mut |phase| {
            if phase == InstallPhase::Unpacking {
                cancelled.store(true, Ordering::SeqCst);
            }
        });
        assert_eq!(result, Err(Failure::Cancelled));
        assert!(fixture.left().is_empty(), "{:?}", fixture.left());
    }

    #[test]
    fn an_archive_that_could_lead_outside_its_place_is_refused_whole() {
        let refused = [
            vec![Entry::Dir("python/"), Entry::File("python/../escape", "x", 0o644)],
            vec![Entry::Dir("python/"), Entry::File("/python/abs", "x", 0o644)],
            vec![Entry::File("other/file", "x", 0o644)],
            vec![Entry::Link("python/bin/up", "../../outside")],
            vec![Entry::Link("python/abs", "/etc/passwd")],
            vec![Entry::Link("python/bin/back", "lib/../../..")],
            vec![Entry::Link("python/bin", "lib"), Entry::File("python/bin/through", "x", 0o644)],
            vec![Entry::File("python/a", "x", 0o644), Entry::File("python/a", "y", 0o644)],
            vec![Entry::File("python/a", "x", 0o644), Entry::Hard("python/b", "python/a")],
            vec![],
        ];
        for entries in refused {
            let fixture = fixture(vec![(Helper::Python, vec![served("python", gz(&tar(&entries)), Layout::Tree { top: "python" })])]);
            assert_eq!(fixture.install(Helper::Python), Err(Failure::BadArchive), "{:?}", entries.len());
            assert!(fixture.left().is_empty(), "{:?}", fixture.left());
        }
    }

    #[test]
    fn a_package_may_hold_no_link() {
        let package = xz(&tar(&[Entry::Link("tex/latex/x.sty", "y.sty")]));
        let fixture =
            fixture(vec![(Helper::Latex, vec![served("tinytex", tinytex_tree(), Layout::Tree { top: "TinyTeX" }), served("x", package, Layout::TexPackage)])]);
        assert_eq!(fixture.install(Helper::Latex), Err(Failure::BadArchive));
        assert!(fixture.left().is_empty(), "{:?}", fixture.left());
    }

    #[test]
    fn links_stay_inside_their_folder() {
        assert!(stays_inside(Path::new("bin/universal-darwin/mktexlsr"), Path::new("../../texmf-dist/scripts/mktexlsr")));
        assert!(stays_inside(Path::new("bin/python3"), Path::new("python3.14")));
        assert!(stays_inside(Path::new("bin/python3"), Path::new("./python3.14")));
        assert!(!stays_inside(Path::new("bin/universal-darwin/x"), Path::new("../../../x")), "climbs out");
        assert!(!stays_inside(Path::new("x"), Path::new("..")), "names nothing");
        assert!(!stays_inside(Path::new("bin/x"), Path::new("..")));
        assert!(!stays_inside(Path::new("bin/x"), Path::new("/usr/bin/x")));
        assert!(!stays_inside(Path::new("bin/x"), Path::new("lib/../x")), "a climb after a name");
    }

    #[test]
    fn the_next_open_removes_half_installs_no_jobdesk_holds() {
        let fixture = fixture(vec![python()]);
        let library = fixture.home().join("Library");
        let dead = library.join(".TinyTeX.jobdesk-1");
        fs::create_dir_all(dead.join("texmf-dist")).unwrap();
        let bun = fixture.home().join(".bun").join("bin");
        fs::create_dir_all(&bun).unwrap();
        fs::write(bun.join(".bun.jobdesk-1"), "half").unwrap();
        fs::write(bun.join("bun"), "finished").unwrap();
        let own = claim(&fixture.dir()).unwrap();
        let running = staging_of(&library.join("TinyTeX"), &own).unwrap();
        fs::create_dir_all(&running).unwrap();
        fixture.installer.clear_leftovers();
        assert!(!dead.exists());
        assert!(!bun.join(".bun.jobdesk-1").exists());
        assert!(bun.join("bun").exists(), "a finished helper stays");
        assert!(running.exists(), "this Jobdesk's own install goes on");
    }
}
