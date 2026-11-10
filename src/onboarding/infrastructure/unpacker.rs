use std::collections::HashMap;
use std::collections::hash_map::Entry as Slot;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};

use icu_normalizer::DecomposingNormalizerBorrowed;
use zip::read::ZipFile;
use zip::{CompressionMethod, ZipArchive};

use crate::onboarding::domain::{Contents, Failure, MAX_ENTRIES, MAX_UNPACKED, MAX_ZIP, Phase, Unpacker};
use crate::shared_kernel::installed_source::{self, InstalledSource, sha256_hex};
use crate::shared_kernel::own_folder::{claim, in_use};

/// The file type bits of a Unix mode, and the two types an archive may hold.
const TYPE_BITS: u32 = 0o170_000;
const REGULAR: u32 = 0o100_000;
const DIRECTORY: u32 = 0o040_000;

/// Files Finder writes into a folder the user only opened: its view settings, and on macOS the mark of a folder whose
/// name is shown translated. A folder that holds only these is empty, and they go just before the release takes its
/// place.
const FINDER_FILES: &[&str] = if cfg!(target_os = "macos") { &[".DS_Store", ".localized"] } else { &[".DS_Store"] };

/// How many times the release tries to take the folder's place while Finder keeps writing its files there.
const PLACE_TRIES: u32 = 3;

/// Unpacks the release zip into the chosen folder: checked whole first, then every entry checked, then written into a
/// hidden staging folder beside the chosen one, which takes the folder's place in one rename.
pub struct ZipUnpacker {
    /// `<data>/onboarding`, whose `<pid>` folders say which Jobdesk still runs.
    dir: PathBuf,
    /// Whether a folder is already an ai-job-search folder, as `workspace` recognises one.
    is_ai_job_search: fn(&Path) -> bool,
}

/// One entry to write, its path relative to the archive's top folder.
struct Planned {
    index: usize,
    path: PathBuf,
    /// `None` for a folder, the mode to give a file otherwise.
    file_mode: Option<u32>,
}

impl ZipUnpacker {
    pub fn new(dir: PathBuf, is_ai_job_search: fn(&Path) -> bool) -> Self {
        Self { dir, is_ai_job_search }
    }

    fn checked_unpack(
        &self,
        zip: &Path,
        source: &InstalledSource,
        folder: &Path,
        cancelled: &dyn Fn() -> bool,
        phase: &mut dyn FnMut(Phase),
    ) -> Result<(), Failure> {
        phase(Phase::Checking);
        // Read once, so the bytes that are checked are the bytes that are unpacked and kept.
        let bytes = read_whole(zip)?;
        if sha256_hex(&bytes) != source.sha256 {
            return Err(Failure::Mismatch);
        }
        phase(Phase::Unpacking);
        let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice())).map_err(|_| Failure::BadArchive)?;
        let plan = plan(&mut archive)?;
        let staging = staging_of(folder, &claim(&self.dir).map_err(|_| Failure::Disk)?)?;
        remove(&staging);
        fs::create_dir(&staging).map_err(|_| Failure::Disk)?;
        let placed = write_all(&mut archive, &plan, &staging, MAX_UNPACKED, cancelled)
            // The base of a later update goes in with the release, in the one rename.
            .and_then(|()| installed_source::write_into(&staging, source, &bytes).map_err(|_| Failure::Disk))
            .and_then(|()| if cancelled() { Err(Failure::Cancelled) } else { put_in_place(&staging, folder) });
        if placed.is_err() {
            remove(&staging);
        }
        placed
    }
}

impl Unpacker for ZipUnpacker {
    fn look(&self, folder: &Path) -> Contents {
        let Ok(meta) = fs::symlink_metadata(folder) else { return Contents::Missing };
        if !meta.is_dir() {
            return Contents::Files;
        }
        if (self.is_ai_job_search)(folder) {
            return Contents::AiJobSearch;
        }
        if holds_files(folder) { Contents::Files } else { Contents::Empty }
    }

    fn make(&self, folder: &Path) -> Result<(), Failure> {
        if self.look(folder) == Contents::Missing {
            fs::create_dir_all(folder).map_err(|_| Failure::Disk)?;
        }
        if self.look(folder) == Contents::Empty { Ok(()) } else { Err(Failure::NotEmpty) }
    }

    fn unpack(&self, zip: &Path, source: &InstalledSource, folder: &Path, cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(Phase)) -> Result<(), Failure> {
        let unpacked = self.checked_unpack(zip, source, folder, cancelled, phase);
        let _ = fs::remove_file(zip);
        unpacked
    }

    fn installed(&self, folder: &Path) -> Option<String> {
        installed_source::read(folder).map(|(source, _)| source.version)
    }

    fn clear_leftovers(&self, folder: &Path) {
        clear_stagings(folder, &self.dir);
    }
}

/// Whether `entry` is one of `FINDER_FILES`, as a plain file.
fn is_finder_file(entry: &fs::DirEntry) -> bool {
    FINDER_FILES.iter().any(|name| entry.file_name() == *name) && entry.file_type().is_ok_and(|kind| kind.is_file())
}

/// Whether `folder` holds anything but Finder's files. A folder that cannot be read holds something.
fn holds_files(folder: &Path) -> bool {
    let Ok(mut entries) = fs::read_dir(folder) else { return true };
    entries.any(|entry| !entry.is_ok_and(|entry| is_finder_file(&entry)))
}

/// Deletes Finder's files from `folder`, and nothing else.
fn remove_finder_files(folder: &Path) {
    let Ok(entries) = fs::read_dir(folder) else { return };
    for entry in entries.flatten().filter(is_finder_file) {
        let _ = fs::remove_file(entry.path());
    }
}

/// The start of the name of every staging folder of `folder`: `.<name>.jobdesk-`.
fn staging_prefix(folder: &Path) -> Option<String> {
    Some(format!(".{}.jobdesk-", folder.file_name()?.to_str()?))
}

/// The end of the name of what was in `folder` before, once a Jobdesk renamed it aside: `.<name>.jobdesk-<pid>-old`.
const ASIDE: &str = "-old";

/// This Jobdesk's staging folder for `folder`: hidden, beside it, so the rename that puts it in place stays on one
/// disk. `own` is this Jobdesk's folder of `onboarding/`, whose name is its pid.
pub fn staging_of(folder: &Path, own: &Path) -> Result<PathBuf, Failure> {
    let (Some(parent), Some(prefix), Some(pid)) = (folder.parent(), staging_prefix(folder), own.file_name()) else { return Err(Failure::Disk) };
    let mut name = OsString::from(prefix);
    name.push(pid);
    Ok(parent.join(name))
}

/// Where this Jobdesk renames what was in `folder` before, until its own install is in: beside its staging folder,
/// and cleared the same way. Only the helper installs rename aside, and they run on macOS only.
#[cfg(target_os = "macos")]
pub fn aside_of(folder: &Path, own: &Path) -> Result<PathBuf, Failure> {
    let mut name = staging_of(folder, own)?.into_os_string();
    name.push(ASIDE);
    Ok(PathBuf::from(name))
}

/// Removes the staging folders, or files, beside `folder` that a Jobdesk which no longer runs left, and what it
/// renamed aside. `dir` is `<data>/onboarding`, whose `<pid>` folders say which Jobdesk still runs. Links are removed,
/// never followed.
pub fn clear_stagings(folder: &Path, dir: &Path) {
    let (Some(parent), Some(prefix)) = (folder.parent(), staging_prefix(folder)) else { return };
    let Ok(entries) = fs::read_dir(parent) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let rest = name.to_str().and_then(|name| name.strip_prefix(prefix.as_str()));
        let Some(pid) = rest.map(|rest| rest.strip_suffix(ASIDE).unwrap_or(rest)) else { continue };
        if pid.is_empty() || !pid.bytes().all(|byte| byte.is_ascii_digit()) || in_use(&dir.join(pid)) {
            continue;
        }
        remove(&entry.path());
    }
}

/// Removes `path` and what it holds. A link is removed itself, never followed.
pub fn remove(path: &Path) {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => {
            let _ = fs::remove_dir_all(path);
        }
        Ok(_) => {
            let _ = fs::remove_file(path);
        }
        Err(_) => {}
    }
}

/// The whole download, or `TooLarge` past `MAX_ZIP`.
fn read_whole(zip: &Path) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    File::open(zip).and_then(|file| file.take(MAX_ZIP + 1).read_to_end(&mut bytes)).map_err(|_| Failure::Disk)?;
    if bytes.len() as u64 > MAX_ZIP {
        return Err(Failure::TooLarge);
    }
    Ok(bytes)
}

/// Every entry of the archive, checked before anything is written. One entry that could lead outside the folder, a
/// link or any other special file, an encrypted entry, two names one disk would take for the same name, an entry in a
/// top `.jobdesk` folder, where the kept base goes, or more than the limits allow refuses the whole archive.
fn plan<R: Read + io::Seek>(archive: &mut ZipArchive<R>) -> Result<Vec<Planned>, Failure> {
    if archive.len() > MAX_ENTRIES {
        return Err(Failure::BadArchive);
    }
    // The zip reader keeps one entry of each exact name, so two with the same name are found here, with entries
    // whose headers disagree or whose bytes overlap.
    archive.validate_header_consistency().map_err(|_| Failure::BadArchive)?;
    let mut top: Option<String> = None;
    let mut total: u64 = 0;
    let mut plan = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index).map_err(|_| Failure::BadArchive)?;
        if entry.encrypted() || !matches!(entry.compression(), CompressionMethod::Stored | CompressionMethod::Deflated) {
            return Err(Failure::BadArchive);
        }
        let kind = entry.unix_mode().map_or(0, |mode| mode & TYPE_BITS);
        let is_dir = entry.is_dir();
        let allowed_kind = match kind {
            0 => true,
            REGULAR => !is_dir,
            DIRECTORY => is_dir,
            _ => false,
        };
        if !allowed_kind {
            return Err(Failure::BadArchive);
        }
        let name = entry.name().map_err(|_| Failure::BadArchive)?;
        let (first, rest) = split_entry(&name, is_dir).ok_or(Failure::BadArchive)?;
        if top.get_or_insert_with(|| first.to_owned()) != first {
            return Err(Failure::BadArchive);
        }
        // The kept base takes `.jobdesk`, spelled any way the disk takes for the same name.
        if rest.as_deref().and_then(|path| path.iter().next()).is_some_and(|part| compared(Path::new(part)) == installed_source::DIR) {
            return Err(Failure::BadArchive);
        }
        // The size the archive claims. The bytes written are counted again in `write_all`.
        total = total.saturating_add(ZipFile::size(&entry));
        if total > MAX_UNPACKED {
            return Err(Failure::BadArchive);
        }
        match (rest, is_dir) {
            // The top folder itself.
            (None, true) => {}
            // A file beside the top folder.
            (None, false) => return Err(Failure::BadArchive),
            (Some(path), true) => plan.push(Planned { index, path, file_mode: None }),
            (Some(path), false) => plan.push(Planned { index, path, file_mode: Some(file_mode(entry.unix_mode())) }),
        }
    }
    check_names(&plan)?;
    Ok(plan)
}

/// Refuses two entries that would land on one name, as macOS and Windows compare names by default: ignoring case and
/// how accented letters are composed. A file whose name a folder also uses, and a folder spelled two ways, are
/// refused too, so nothing is ever written over something unpacked before it.
fn check_names(plan: &[Planned]) -> Result<(), Failure> {
    // Each name as the disk compares it, with its spelling and whether it is a folder.
    let mut seen: HashMap<String, (&Path, bool)> = HashMap::with_capacity(plan.len());
    for planned in plan {
        if seen.insert(compared(&planned.path), (&planned.path, planned.file_mode.is_none())).is_some() {
            return Err(Failure::BadArchive);
        }
    }
    // The folders each entry needs, which an archive may leave out.
    for planned in plan {
        for folder in planned.path.ancestors().skip(1).filter(|folder| !folder.as_os_str().is_empty()) {
            match seen.entry(compared(folder)) {
                Slot::Vacant(slot) => {
                    slot.insert((folder, true));
                }
                Slot::Occupied(slot) if *slot.get() != (folder, true) => return Err(Failure::BadArchive),
                Slot::Occupied(_) => {}
            }
        }
    }
    Ok(())
}

/// `path` as a disk that ignores case and Unicode composition compares it.
fn compared(path: &Path) -> String {
    DecomposingNormalizerBorrowed::new_nfd().normalize(&path.to_string_lossy().to_lowercase()).into_owned()
}

/// The top folder of an entry's name and the path under it, or `None` for a name that could lead anywhere but under
/// it, or that Windows would not write as named: see `plain_part`.
fn split_entry(name: &str, is_dir: bool) -> Option<(&str, Option<PathBuf>)> {
    let name = if is_dir { name.strip_suffix('/').unwrap_or(name) } else { name };
    let mut parts = name.split('/');
    let first = parts.next()?;
    if !std::iter::once(first).chain(parts.clone()).all(plain_part) {
        return None;
    }
    let path: PathBuf = parts.collect();
    Some((first, (path.components().next().is_some()).then_some(path)))
}

/// Whether one part of a name is a plain name on every disk: not empty, `.` or `..`; no backslash, colon, control
/// character or other character Windows refuses; no trailing dot or space, which Windows drops; and not a Windows
/// device name.
fn plain_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && !part.chars().any(|c| c.is_control() || matches!(c, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        && !part.ends_with(['.', ' '])
        && !is_device_name(part)
}

/// Whether Windows reads `part` as a device rather than a file, whatever follows its first dot and in any case: `CON`,
/// `nul.txt`, `Aux`, `COM1`, `LPT¹`.
fn is_device_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).trim_end_matches(' ').to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") {
        return true;
    }
    let Some(number) = stem.strip_prefix("COM").or_else(|| stem.strip_prefix("LPT")) else { return false };
    let mut chars = number.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_digit() || "¹²³".contains(c))
}

/// The mode a file gets: its own read and run bits, never set-id, sticky or group and other write, and always
/// readable and writable by the user.
pub fn file_mode(mode: Option<u32>) -> u32 {
    mode.map_or(0o644, |mode| mode & 0o755) | 0o600
}

/// Writes every planned entry into `staging`. Folders are made one level at a time below `staging`, never `staging`
/// itself, so if `staging` goes while this runs, the next write fails rather than starting a new, partial folder. The
/// bytes written are counted again, whatever sizes the archive claimed, and more than `limit` refuses the archive.
fn write_all<R: Read + io::Seek>(
    archive: &mut ZipArchive<R>,
    plan: &[Planned],
    staging: &Path,
    limit: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), Failure> {
    let mut written: u64 = 0;
    let mut buffer = vec![0; 64 * 1024];
    for planned in plan {
        if cancelled() {
            return Err(Failure::Cancelled);
        }
        let Some(mode) = planned.file_mode else {
            make_folders(staging, &planned.path)?;
            continue;
        };
        if let Some(parent) = planned.path.parent() {
            make_folders(staging, parent)?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, mode);
        #[cfg(not(unix))]
        let _ = mode;
        let mut file = options.open(staging.join(&planned.path)).map_err(|_| Failure::Disk)?;
        let mut entry = archive.by_index(planned.index).map_err(|_| Failure::BadArchive)?;
        loop {
            let read = match entry.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                // A corrupt entry, one whose checksum does not match, or one larger than its header says.
                Err(_) => return Err(Failure::BadArchive),
            };
            written += read as u64;
            if written > limit {
                return Err(Failure::BadArchive);
            }
            file.write_all(&buffer[..read]).map_err(|_| Failure::Disk)?;
        }
    }
    Ok(())
}

/// Makes each folder of `relative` below `staging` that is missing. A folder already there must be a folder, not a
/// link or a file.
pub fn make_folders(staging: &Path, relative: &Path) -> Result<(), Failure> {
    let mut at = staging.to_path_buf();
    for part in relative.components() {
        at.push(part);
        match fs::create_dir(&at) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && fs::symlink_metadata(&at).is_ok_and(|meta| meta.is_dir()) => {}
            Err(_) => return Err(Failure::Disk),
        }
    }
    Ok(())
}

/// Puts `staging` in the place of `folder` in one rename, which never replaces a folder that holds anything but
/// Finder's files. The release keeps the permissions the user's folder had.
fn put_in_place(staging: &Path, folder: &Path) -> Result<(), Failure> {
    let own = fs::symlink_metadata(staging).map_err(|_| Failure::Disk)?.permissions();
    if let Ok(meta) = fs::symlink_metadata(folder)
        && meta.is_dir()
    {
        fs::set_permissions(staging, meta.permissions()).map_err(|_| Failure::Disk)?;
    }
    let mut tries = 1;
    let placed = loop {
        remove_finder_files(folder);
        match rename_into(staging, folder) {
            // Finder wrote its files again between the two steps.
            Err(Failure::NotEmpty) if tries < PLACE_TRIES && !holds_files(folder) => tries += 1,
            placed => break placed,
        }
    };
    if placed.is_err() {
        // Its own permissions again, so it can be removed even when the user's folder was read-only.
        let _ = fs::set_permissions(staging, own);
    }
    placed
}

/// Renames `staging` onto the empty `folder`, or a file onto a file. A folder that holds anything, or a place of the
/// other kind, is `NotEmpty`.
pub fn rename_into(staging: &Path, folder: &Path) -> Result<(), Failure> {
    // Windows renames a folder only onto a free name: the empty folder goes first, and only while it is empty.
    #[cfg(windows)]
    if fs::symlink_metadata(folder).is_ok() && fs::remove_dir(folder).is_err() {
        return Err(Failure::NotEmpty);
    }
    fs::rename(staging, folder).map_err(|error| match error.kind() {
        io::ErrorKind::DirectoryNotEmpty | io::ErrorKind::AlreadyExists | io::ErrorKind::NotADirectory | io::ErrorKind::IsADirectory => Failure::NotEmpty,
        _ => Failure::Disk,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    const TOP: &str = "ai-job-search-1.7.5/";

    #[derive(Clone, Copy)]
    enum Entry<'a> {
        File(&'a str, &'a [u8]),
        Mode(&'a str, &'a [u8], u32),
        Dir(&'a str),
        Link(&'a str, &'a str),
    }

    /// A fictional release zip, its entries named as written.
    fn zip_of(entries: &[Entry]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for entry in entries {
            match entry {
                Entry::File(name, bytes) => {
                    writer.start_file(*name, SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)).unwrap();
                    writer.write_all(bytes).unwrap();
                }
                Entry::Mode(name, bytes, mode) => {
                    writer.start_file(*name, stored.unix_permissions(*mode)).unwrap();
                    writer.write_all(bytes).unwrap();
                }
                Entry::Dir(name) => writer.add_directory(*name, stored).unwrap(),
                Entry::Link(name, target) => writer.add_symlink(*name, *target, stored).unwrap(),
            }
        }
        writer.finish().unwrap().into_inner()
    }

    /// `bytes` with the central directory header of the entry `name` changed by `change`, which gets the header's
    /// offset, for what the zip writer will not write.
    fn with_header(mut bytes: Vec<u8>, name: &str, change: impl Fn(&mut Vec<u8>, usize)) -> Vec<u8> {
        let mut at = 0;
        while let Some(found) = bytes[at..].windows(4).position(|window| window == b"PK\x01\x02") {
            let header = at + found;
            let length = u16::from_le_bytes([bytes[header + 28], bytes[header + 29]]) as usize;
            if &bytes[header + 46..header + 46 + length] == name.as_bytes() {
                change(&mut bytes, header);
                return bytes;
            }
            at = header + 4;
        }
        panic!("no entry {name}")
    }

    /// `bytes` with the Unix mode of the entry `name` set to `mode`, type bits included, which the zip writer masks.
    fn with_mode(bytes: Vec<u8>, name: &str, mode: u32) -> Vec<u8> {
        with_header(bytes, name, |bytes, header| {
            bytes[header + 5] = 3; // made on Unix
            bytes[header + 38..header + 42].copy_from_slice(&(mode << 16).to_le_bytes());
        })
    }

    /// A release whose one file says it is encrypted.
    fn encrypted() -> Vec<u8> {
        let bytes = zip_of(&[Entry::Dir(TOP), Entry::Mode("ai-job-search-1.7.5/secret", b"x", 0o644)]);
        with_header(bytes, "ai-job-search-1.7.5/secret", |bytes, header| bytes[header + 8] |= 1)
    }

    /// `bytes` with every name `from` spelled `to`, which must be as long, in both the local and the central headers.
    fn renamed(mut bytes: Vec<u8>, from: &str, to: &str) -> Vec<u8> {
        assert_eq!(from.len(), to.len());
        let mut at = 0;
        while let Some(found) = bytes[at..].windows(from.len()).position(|window| window == from.as_bytes()) {
            let start = at + found;
            bytes[start..start + to.len()].copy_from_slice(to.as_bytes());
            at = start + to.len();
        }
        bytes
    }

    /// Two files whose headers each say they hold one byte, when they hold 600.
    fn under_reported() -> Vec<u8> {
        let bytes = zip_of(&[Entry::File("ai-job-search-1.7.5/a", &[0; 600]), Entry::File("ai-job-search-1.7.5/b", &[0; 600])]);
        ["ai-job-search-1.7.5/a", "ai-job-search-1.7.5/b"]
            .into_iter()
            .fold(bytes, |bytes, name| with_header(bytes, name, |bytes, header| bytes[header + 24..header + 28].copy_from_slice(&1u32.to_le_bytes())))
    }

    fn release() -> Vec<u8> {
        let bytes = zip_of(&[
            Entry::Dir(TOP),
            Entry::File("ai-job-search-1.7.5/README.md", b"# ai-job-search"),
            Entry::Dir("ai-job-search-1.7.5/.claude/commands/"),
            Entry::File("ai-job-search-1.7.5/.claude/commands/apply.md", b"apply"),
            Entry::Mode("ai-job-search-1.7.5/scripts/run.sh", b"#!/bin/sh", 0o777),
        ]);
        with_mode(bytes, "ai-job-search-1.7.5/scripts/run.sh", 0o106_777)
    }

    /// The pinned release of `sha256`.
    fn pinned(sha256: &str) -> InstalledSource {
        InstalledSource { version: "1.7.5".to_owned(), sha256: sha256.to_owned() }
    }

    /// Whether a `.jobdesk` folder is in `folder`.
    fn has_base(folder: &Path) -> bool {
        fs::symlink_metadata(folder.join(installed_source::DIR)).is_ok()
    }

    struct Setup {
        _temp: tempfile::TempDir,
        data: PathBuf,
        folder: PathBuf,
        unpacker: ZipUnpacker,
    }

    fn marker(folder: &Path) -> bool {
        folder.join(".claude/commands/apply.md").exists()
    }

    fn setup() -> Setup {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let folder = temp.path().join("Documents").join("Job search");
        let unpacker = ZipUnpacker::new(data.join("onboarding"), marker);
        Setup { data, folder, unpacker, _temp: temp }
    }

    impl Setup {
        fn zip(&self, bytes: &[u8]) -> PathBuf {
            let part = self.data.join("onboarding").join("1").join("ai-job-search-v1.7.5.zip.part");
            fs::create_dir_all(part.parent().unwrap()).unwrap();
            fs::write(&part, bytes).unwrap();
            part
        }

        fn unpack(&self, bytes: &[u8], sha256: &str) -> (Result<(), Failure>, Vec<Phase>) {
            let zip = self.zip(bytes);
            self.unpacker.make(&self.folder).unwrap();
            let mut phases = Vec::new();
            let end = self.unpacker.unpack(&zip, &pinned(sha256), &self.folder, &|| false, &mut |phase| phases.push(phase));
            assert!(!zip.exists(), "the zip is deleted whatever the end");
            (end, phases)
        }

        /// Everything beside the folder and in it, so a test sees no staging is left.
        fn around(&self) -> Vec<String> {
            let mut names: Vec<String> =
                fs::read_dir(self.folder.parent().unwrap()).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            names
        }

        fn is_empty(&self) -> bool {
            fs::read_dir(&self.folder).unwrap().next().is_none()
        }

        /// This Jobdesk's staging folder for the chosen folder.
        fn staging(&self) -> PathBuf {
            staging_of(&self.folder, &claim(&self.data.join("onboarding")).unwrap()).unwrap()
        }
    }

    #[test]
    fn a_release_that_matches_is_unpacked_without_its_top_folder() {
        let setup = setup();
        let bytes = release();
        let (end, phases) = setup.unpack(&bytes, &sha256_hex(&bytes));
        assert_eq!(end, Ok(()));
        assert_eq!(phases, [Phase::Checking, Phase::Unpacking]);
        assert_eq!(fs::read_to_string(setup.folder.join("README.md")).unwrap(), "# ai-job-search");
        assert_eq!(fs::read_to_string(setup.folder.join(".claude/commands/apply.md")).unwrap(), "apply");
        assert_eq!(setup.around(), ["Job search"], "no staging folder is left");
        assert_eq!(setup.unpacker.look(&setup.folder), Contents::AiJobSearch);
    }

    #[test]
    fn the_release_is_kept_in_the_folder_as_its_update_base() {
        let setup = setup();
        let bytes = release();
        assert_eq!(setup.unpacker.installed(&setup.folder), None, "nothing is kept before the release is in");
        assert_eq!(setup.unpack(&bytes, &sha256_hex(&bytes)).0, Ok(()));
        assert_eq!(fs::read(setup.folder.join(".jobdesk/source.zip")).unwrap(), bytes, "the exact bytes that matched");
        assert_eq!(installed_source::read(&setup.folder), Some((pinned(&sha256_hex(&bytes)), bytes)));
        assert_eq!(setup.unpacker.installed(&setup.folder).as_deref(), Some("1.7.5"));
    }

    #[test]
    fn a_release_written_as_a_stream_is_unpacked() {
        // Sizes and checksums after each file's bytes, as a zip written without seeking has them.
        let mut writer = ZipWriter::new_stream(Vec::new());
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.add_directory(TOP, deflated).unwrap();
        writer.start_file("ai-job-search-1.7.5/README.md", deflated).unwrap();
        writer.write_all(b"# ai-job-search").unwrap();
        writer.start_file("ai-job-search-1.7.5/.claude/commands/apply.md", deflated).unwrap();
        writer.write_all(b"apply").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let setup = setup();
        assert_eq!(setup.unpack(&bytes, &sha256_hex(&bytes)).0, Ok(()));
        assert_eq!(fs::read_to_string(setup.folder.join(".claude/commands/apply.md")).unwrap(), "apply");
    }

    #[cfg(unix)]
    #[test]
    fn set_id_bits_and_group_and_other_write_are_dropped() {
        use std::os::unix::fs::PermissionsExt;
        let setup = setup();
        let bytes = release();
        assert_eq!(setup.unpack(&bytes, &sha256_hex(&bytes)).0, Ok(()));
        let mode = fs::metadata(setup.folder.join("scripts/run.sh")).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode & 0o7022, 0, "{mode:o}");
        assert_eq!(mode & 0o700, 0o700, "the user can still read, write and run it");
    }

    #[cfg(unix)]
    #[test]
    fn the_folder_keeps_the_permissions_the_user_gave_it() {
        use std::os::unix::fs::PermissionsExt;
        let setup = setup();
        fs::create_dir_all(&setup.folder).unwrap();
        fs::set_permissions(&setup.folder, fs::Permissions::from_mode(0o700)).unwrap();
        let bytes = release();
        assert_eq!(setup.unpack(&bytes, &sha256_hex(&bytes)).0, Ok(()));
        assert_eq!(fs::metadata(&setup.folder).unwrap().permissions().mode() & 0o7777, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_folder_that_filled_still_loses_the_staging_folder() {
        use std::os::unix::fs::PermissionsExt;
        let setup = setup();
        let bytes = release();
        let zip = setup.zip(&bytes);
        setup.unpacker.make(&setup.folder).unwrap();
        let end = setup.unpacker.unpack(&zip, &pinned(&sha256_hex(&bytes)), &setup.folder, &|| false, &mut |phase| {
            if phase == Phase::Unpacking {
                fs::write(setup.folder.join("cv.pdf"), "mine").unwrap();
                fs::set_permissions(&setup.folder, fs::Permissions::from_mode(0o500)).unwrap();
            }
        });
        fs::set_permissions(&setup.folder, fs::Permissions::from_mode(0o700)).unwrap();
        // macOS refuses the rename for the permissions before it looks at what the folder holds.
        assert!(matches!(end, Err(Failure::NotEmpty | Failure::Disk)), "{end:?}");
        assert_eq!(fs::read_to_string(setup.folder.join("cv.pdf")).unwrap(), "mine");
        assert_eq!(setup.around(), ["Job search"], "the staging folder goes");
    }

    #[test]
    fn a_mismatch_leaves_the_folder_empty_and_deletes_the_zip() {
        let setup = setup();
        let bytes = release();
        let wrong = sha256_hex(b"another release");
        let (end, phases) = setup.unpack(&bytes, &wrong);
        assert_eq!(end, Err(Failure::Mismatch));
        assert_eq!(phases, [Phase::Checking], "nothing is unpacked once the check fails");
        assert!(setup.is_empty());
        assert!(!has_base(&setup.folder));
        assert_eq!(setup.around(), ["Job search"]);
    }

    #[test]
    fn each_refused_entry_kind_refuses_the_whole_archive() {
        let good = Entry::File("ai-job-search-1.7.5/README.md", b"readme");
        let refused: Vec<(&str, Vec<u8>)> = vec![
            ("an absolute path", zip_of(&[Entry::Dir(TOP), Entry::File("/etc/evil", b"x"), good])),
            ("a parent part", zip_of(&[Entry::Dir(TOP), Entry::File("ai-job-search-1.7.5/../evil", b"x")])),
            ("a parent top", zip_of(&[Entry::File("../evil", b"x")])),
            ("a dot part", zip_of(&[Entry::File("ai-job-search-1.7.5/./evil", b"x")])),
            ("a backslash", zip_of(&[Entry::File("ai-job-search-1.7.5/..\\evil", b"x")])),
            ("a drive", zip_of(&[Entry::File("C:/evil", b"x")])),
            ("an empty part", zip_of(&[Entry::File("ai-job-search-1.7.5//evil", b"x")])),
            ("a link", zip_of(&[Entry::Dir(TOP), Entry::Link("ai-job-search-1.7.5/home", "/Users"), good])),
            ("a file outside the top folder", zip_of(&[Entry::Dir(TOP), good, Entry::File("other-folder/evil", b"x")])),
            ("a file beside the top folder", zip_of(&[Entry::Dir(TOP), Entry::File("evil", b"x")])),
            ("a fifo", with_mode(zip_of(&[Entry::Dir(TOP), Entry::Mode("ai-job-search-1.7.5/pipe", b"", 0o644)]), "ai-job-search-1.7.5/pipe", 0o010_644)),
            (
                "a link by its mode",
                with_mode(zip_of(&[Entry::Dir(TOP), Entry::Mode("ai-job-search-1.7.5/home", b"/Users", 0o644)]), "ai-job-search-1.7.5/home", 0o120_777),
            ),
            (
                "a folder that says it is a file",
                with_mode(zip_of(&[Entry::Dir(TOP), Entry::Dir("ai-job-search-1.7.5/docs/")]), "ai-job-search-1.7.5/docs/", 0o100_644),
            ),
            ("an encrypted entry", encrypted()),
            ("a file in the base's folder", zip_of(&[Entry::Dir(TOP), good, Entry::File("ai-job-search-1.7.5/.jobdesk/x", b"x")])),
            ("the base's folder in capitals", zip_of(&[Entry::Dir(TOP), good, Entry::File("ai-job-search-1.7.5/.JOBDESK/x", b"x")])),
            ("the base's folder alone", zip_of(&[Entry::Dir(TOP), good, Entry::Dir("ai-job-search-1.7.5/.jobdesk/")])),
            ("a file named as the base's folder", zip_of(&[Entry::Dir(TOP), good, Entry::File("ai-job-search-1.7.5/.Jobdesk", b"x")])),
            ("a Windows device name", zip_of(&[Entry::Dir(TOP), good, Entry::File("ai-job-search-1.7.5/docs/aux.txt", b"x")])),
            ("a trailing dot", zip_of(&[Entry::Dir(TOP), good, Entry::File("ai-job-search-1.7.5/notes.", b"x")])),
            ("a duplicate name", renamed(zip_of(&[Entry::Dir(TOP), good, Entry::File("ai-job-search-1.7.5/READMF.md", b"x")]), "READMF", "README")),
            ("names that differ only in case", zip_of(&[Entry::Dir(TOP), good, Entry::File("ai-job-search-1.7.5/readme.md", b"x")])),
            (
                "names that differ only in how an accent is composed",
                zip_of(&[Entry::Dir(TOP), Entry::File("ai-job-search-1.7.5/caf\u{e9}.md", b"x"), Entry::File("ai-job-search-1.7.5/cafe\u{301}.md", b"y")]),
            ),
            (
                "a file a folder also uses",
                zip_of(&[Entry::Dir(TOP), Entry::File("ai-job-search-1.7.5/docs", b"x"), Entry::File("ai-job-search-1.7.5/docs/a.md", b"y")]),
            ),
            (
                "a folder spelled two ways",
                zip_of(&[Entry::Dir(TOP), Entry::File("ai-job-search-1.7.5/Docs/a.md", b"x"), Entry::File("ai-job-search-1.7.5/docs/b.md", b"y")]),
            ),
        ];
        for (what, bytes) in refused {
            let setup = setup();
            assert_eq!(setup.unpack(&bytes, &sha256_hex(&bytes)).0, Err(Failure::BadArchive), "{what}");
            assert!(setup.is_empty(), "{what}: nothing is written");
            assert!(!has_base(&setup.folder), "{what}");
            assert_eq!(setup.around(), ["Job search"], "{what}");
        }
    }

    #[test]
    fn more_than_the_limits_refuses_the_whole_archive() {
        // Large but compressing to almost nothing, so the test zip stays small.
        let big = vec![0; (MAX_UNPACKED / 2 + 1) as usize];
        let too_large = zip_of(&[Entry::File("ai-job-search-1.7.5/a", &big), Entry::File("ai-job-search-1.7.5/b", &big)]);
        let names: Vec<String> = (0..=MAX_ENTRIES).map(|n| format!("ai-job-search-1.7.5/{n}")).collect();
        let too_many = zip_of(&names.iter().map(|name| Entry::Mode(name, b"", 0o644)).collect::<Vec<_>>());
        for (what, bytes) in [("too large", too_large), ("too many", too_many), ("sizes under-reported", under_reported())] {
            let setup = setup();
            assert_eq!(setup.unpack(&bytes, &sha256_hex(&bytes)).0, Err(Failure::BadArchive), "{what}");
            assert!(setup.is_empty(), "{what}");
            assert_eq!(setup.around(), ["Job search"], "{what}");
        }
    }

    #[test]
    fn the_bytes_written_are_counted_whatever_the_headers_claim() {
        // Each file's bytes past its header's size, and headers that disagree, are refused before this count; so is
        // a total of the headers' sizes past the limit. A smaller limit shows the count of bytes written refuses too.
        let bytes = zip_of(&[Entry::Dir(TOP), Entry::File("ai-job-search-1.7.5/a", &[0; 600]), Entry::File("ai-job-search-1.7.5/b", &[0; 600])]);
        let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        let plan = plan(&mut archive).unwrap();
        let staging = tempfile::tempdir().unwrap();
        assert_eq!(write_all(&mut archive, &plan, staging.path(), 1_000, &|| false), Err(Failure::BadArchive));
        let staging = tempfile::tempdir().unwrap();
        assert_eq!(write_all(&mut archive, &plan, staging.path(), 1_200, &|| false), Ok(()));
    }

    #[test]
    fn names_one_disk_takes_for_the_same_are_refused_before_anything_is_written() {
        let planned = |path: &str, is_dir: bool| Planned { index: 0, path: PathBuf::from(path), file_mode: (!is_dir).then_some(0o644) };
        let refused = [
            [planned("a.md", false), planned("a.md", false)],
            [planned("docs", true), planned("docs", true)],
            [planned("A.md", false), planned("a.md", false)],
            [planned("caf\u{e9}.md", false), planned("cafe\u{301}.md", false)],
            [planned("docs", false), planned("docs/a.md", false)],
            [planned("Docs", true), planned("docs/a.md", false)],
        ];
        for plan in refused {
            assert_eq!(check_names(&plan), Err(Failure::BadArchive), "{:?}", plan.map(|planned| planned.path));
        }
        let allowed = [planned("docs/a.md", false), planned("docs", true), planned("docs/b.md", false), planned("Docs.md", false)];
        assert_eq!(check_names(&allowed), Ok(()));
    }

    #[test]
    fn a_non_empty_folder_is_refused_and_an_ai_job_search_folder_is_recognised() {
        let setup = setup();
        assert_eq!(setup.unpacker.look(&setup.folder), Contents::Missing);
        fs::create_dir_all(&setup.folder).unwrap();
        assert_eq!(setup.unpacker.look(&setup.folder), Contents::Empty);
        fs::write(setup.folder.join("cv.pdf"), "mine").unwrap();
        assert_eq!(setup.unpacker.look(&setup.folder), Contents::Files);
        assert_eq!(setup.unpacker.make(&setup.folder), Err(Failure::NotEmpty));
        fs::create_dir_all(setup.folder.join(".claude/commands")).unwrap();
        fs::write(setup.folder.join(".claude/commands/apply.md"), "apply").unwrap();
        assert_eq!(setup.unpacker.look(&setup.folder), Contents::AiJobSearch);
        let file = setup.folder.parent().unwrap().join("notes.txt");
        fs::write(&file, "mine").unwrap();
        assert_eq!(setup.unpacker.look(&file), Contents::Files, "a file is never a folder to use");
    }

    #[test]
    fn a_folder_holding_only_finders_files_is_empty_and_they_go() {
        let setup = setup();
        fs::create_dir_all(&setup.folder).unwrap();
        fs::write(setup.folder.join(".DS_Store"), "view").unwrap();
        assert_eq!(setup.unpacker.look(&setup.folder), Contents::Empty);
        let bytes = release();
        assert_eq!(setup.unpack(&bytes, &sha256_hex(&bytes)).0, Ok(()));
        assert!(!setup.folder.join(".DS_Store").exists());
        assert_eq!(setup.unpacker.look(&setup.folder), Contents::AiJobSearch);
    }

    #[test]
    fn finders_file_beside_another_file_or_as_a_folder_is_not_empty() {
        let setup = setup();
        fs::create_dir_all(setup.folder.join(".DS_Store")).unwrap();
        assert_eq!(setup.unpacker.look(&setup.folder), Contents::Files, "only a plain file is Finder's");
        fs::remove_dir(setup.folder.join(".DS_Store")).unwrap();
        fs::write(setup.folder.join(".DS_Store"), "view").unwrap();
        fs::write(setup.folder.join("cv.pdf"), "mine").unwrap();
        assert_eq!(setup.unpacker.look(&setup.folder), Contents::Files);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_localized_mark_is_finders_on_macos() {
        let setup = setup();
        fs::create_dir_all(&setup.folder).unwrap();
        fs::write(setup.folder.join(".localized"), "").unwrap();
        fs::write(setup.folder.join(".DS_Store"), "view").unwrap();
        assert_eq!(setup.unpacker.look(&setup.folder), Contents::Empty);
    }

    #[test]
    fn finder_writing_into_the_folder_during_the_unpack_does_not_stop_it() {
        let setup = setup();
        let bytes = release();
        let zip = setup.zip(&bytes);
        setup.unpacker.make(&setup.folder).unwrap();
        let end = setup.unpacker.unpack(&zip, &pinned(&sha256_hex(&bytes)), &setup.folder, &|| false, &mut |phase| {
            if phase == Phase::Unpacking {
                fs::write(setup.folder.join(".DS_Store"), "view").unwrap();
            }
        });
        assert_eq!(end, Ok(()));
        assert_eq!(fs::read_to_string(setup.folder.join("README.md")).unwrap(), "# ai-job-search");
        assert!(!setup.folder.join(".DS_Store").exists());
    }

    #[test]
    fn a_folder_that_filled_during_the_unpack_keeps_what_it_holds() {
        let setup = setup();
        let bytes = release();
        let zip = setup.zip(&bytes);
        setup.unpacker.make(&setup.folder).unwrap();
        let end = setup.unpacker.unpack(&zip, &pinned(&sha256_hex(&bytes)), &setup.folder, &|| false, &mut |phase| {
            if phase == Phase::Unpacking {
                fs::write(setup.folder.join("cv.pdf"), "mine").unwrap();
                fs::write(setup.folder.join(".DS_Store"), "view").unwrap();
            }
        });
        assert_eq!(end, Err(Failure::NotEmpty));
        assert_eq!(fs::read_to_string(setup.folder.join("cv.pdf")).unwrap(), "mine");
        assert_eq!(setup.around(), ["Job search"], "the staging folder goes");
    }

    #[test]
    fn a_staging_folder_removed_mid_unpack_fails_and_leaves_the_folder_empty() {
        let setup = setup();
        let bytes = release();
        let zip = setup.zip(&bytes);
        setup.unpacker.make(&setup.folder).unwrap();
        let staging = setup.staging();
        // Asked before each entry: after the first is written, another Jobdesk removes the staging folder.
        let asked = Cell::new(0);
        let cancelled = || {
            asked.set(asked.get() + 1);
            if asked.get() == 2 {
                fs::remove_dir_all(&staging).unwrap();
            }
            false
        };
        let end = setup.unpacker.unpack(&zip, &pinned(&sha256_hex(&bytes)), &setup.folder, &cancelled, &mut |_| {});
        assert_eq!(end, Err(Failure::Disk));
        assert!(asked.get() >= 2, "the staging folder went mid-unpack");
        assert!(setup.is_empty());
        assert_eq!(setup.around(), ["Job search"]);
    }

    #[test]
    fn cancel_mid_unpack_leaves_the_folder_empty() {
        let setup = setup();
        let bytes = release();
        let zip = setup.zip(&bytes);
        setup.unpacker.make(&setup.folder).unwrap();
        let end = setup.unpacker.unpack(&zip, &pinned(&sha256_hex(&bytes)), &setup.folder, &|| true, &mut |_| {});
        assert_eq!(end, Err(Failure::Cancelled));
        assert!(setup.is_empty());
        assert!(!has_base(&setup.folder));
        assert_eq!(setup.around(), ["Job search"]);
    }

    #[test]
    fn cancel_once_the_base_is_written_leaves_the_folder_empty() {
        let setup = setup();
        let bytes = release();
        let zip = setup.zip(&bytes);
        setup.unpacker.make(&setup.folder).unwrap();
        let staging = setup.staging();
        let cancelled = || has_base(&staging);
        let end = setup.unpacker.unpack(&zip, &pinned(&sha256_hex(&bytes)), &setup.folder, &cancelled, &mut |_| {});
        assert_eq!(end, Err(Failure::Cancelled));
        assert!(setup.is_empty());
        assert!(!has_base(&setup.folder));
        assert_eq!(setup.around(), ["Job search"]);
    }

    #[test]
    fn a_crash_midway_is_cleared_on_the_next_open() {
        let setup = setup();
        let parent = setup.folder.parent().unwrap().to_owned();
        fs::create_dir_all(&setup.folder).unwrap();
        let crashed = parent.join(".Job search.jobdesk-1");
        fs::create_dir_all(crashed.join("scripts")).unwrap();
        fs::write(crashed.join("scripts/run.sh"), "half").unwrap();
        let own = setup.staging();
        fs::create_dir(&own).unwrap();
        let theirs = parent.join(".Job search.jobdesk-notes");
        fs::create_dir(&theirs).unwrap();
        setup.unpacker.clear_leftovers(&setup.folder);
        assert!(!crashed.exists(), "a staging folder no Jobdesk holds goes");
        assert!(own.exists(), "this Jobdesk's own staging stays");
        assert!(theirs.exists(), "a name that only looks alike stays");
        assert!(setup.folder.exists());
    }

    #[cfg(unix)]
    #[test]
    fn clearing_a_staging_link_never_follows_it() {
        use std::os::unix::fs::symlink;
        let setup = setup();
        let parent = setup.folder.parent().unwrap().to_owned();
        fs::create_dir_all(&parent).unwrap();
        let elsewhere = parent.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::write(elsewhere.join("keep.txt"), "keep").unwrap();
        let link = parent.join(".Job search.jobdesk-1");
        symlink(&elsewhere, &link).unwrap();
        setup.unpacker.clear_leftovers(&setup.folder);
        assert!(fs::symlink_metadata(&link).is_err(), "the link itself goes");
        assert_eq!(fs::read_to_string(elsewhere.join("keep.txt")).unwrap(), "keep");
    }

    #[test]
    fn a_name_splits_into_its_top_folder_and_the_path_under_it() {
        assert_eq!(split_entry("top/a/b.md", false), Some(("top", Some(PathBuf::from("a/b.md")))));
        assert_eq!(split_entry("top/", true), Some(("top", None)));
        assert_eq!(split_entry("top/a/", true), Some(("top", Some(PathBuf::from("a")))));
        assert_eq!(split_entry("top/console.md", false), Some(("top", Some(PathBuf::from("console.md")))));
        assert_eq!(split_entry("top/main.aux", false), Some(("top", Some(PathBuf::from("main.aux")))));
        assert_eq!(split_entry("top/COM10", false), Some(("top", Some(PathBuf::from("COM10")))));
        let bad = [
            "",
            "/top/a",
            "top/../a",
            "top/a/..",
            "..",
            "top/a\\b",
            "top/C:",
            "top/a\0b",
            "top//a",
            "top/a/",
            "top/a\nb",
            "top/a?",
            "top/a*",
            "top/a|b",
            "top/a<b",
            "top/a\"b",
            "top/a.",
            "top/a ",
            "top/dir./a",
            "CON/a",
        ];
        for bad in bad {
            assert_eq!(split_entry(bad, false), None, "{bad:?}");
        }
    }

    #[test]
    fn windows_device_names_are_known_in_any_case_and_with_any_extension() {
        for device in [
            "CON",
            "con",
            "Prn",
            "AUX",
            "aux.txt",
            "NUL",
            "nul.tar.gz",
            "COM1",
            "com9",
            "COM0",
            "LPT1",
            "COM\u{b9}",
            "lpt\u{b3}",
            "CON .txt",
            "CONIN$",
            "conout$",
        ] {
            assert!(is_device_name(device), "{device:?}");
        }
        for plain in ["CONSOLE", "console.md", "auxiliary", "COM", "COM10", "LPT", "nully", "main.aux", "readme.con"] {
            assert!(!is_device_name(plain), "{plain:?}");
        }
    }

    #[test]
    fn a_file_mode_never_keeps_set_id_or_shared_write() {
        assert_eq!(file_mode(Some(0o6777)), 0o755);
        assert_eq!(file_mode(Some(0o100_666)), 0o644);
        assert_eq!(file_mode(Some(0o400)), 0o600);
        assert_eq!(file_mode(None), 0o644);
    }
}
