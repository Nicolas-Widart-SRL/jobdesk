//! The ai-job-search release zip, checked the one way every reader of it agrees on: its SHA-256 against the pin first,
//! then every entry's name, kind and size, before a byte of any entry is read.

use std::collections::HashMap;
use std::collections::hash_map::Entry as Slot;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};

use icu_normalizer::DecomposingNormalizerBorrowed;
use zip::read::ZipFile;
use zip::{CompressionMethod, ZipArchive};

use crate::shared_kernel::installed_source::{self, MAX_ZIP, sha256_hex};

/// The most a release may hold once unpacked, and the most entries it may have. A larger one is refused whole.
pub const MAX_UNPACKED: u64 = 100 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 20_000;

/// The file type bits of a Unix mode, and the two types an archive may hold.
const TYPE_BITS: u32 = 0o170_000;
const REGULAR: u32 = 0o100_000;
const DIRECTORY: u32 = 0o040_000;

/// Why a release zip was refused. Each caller says it in its own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZipFailure {
    /// The zip is larger than setup downloads.
    TooLarge,
    /// The zip is not the release the pin names.
    Mismatch,
    /// The archive holds an entry that could lead outside the folder, a link, two names one disk would take for the
    /// same name, or more than the limits allow.
    BadArchive,
}

/// One file of a release, read whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Relative, with the archive's top folder stripped.
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    /// The mode the file gets: see `file_mode`.
    pub mode: u32,
}

/// One entry to read, its path relative to the archive's top folder.
#[derive(Debug)]
pub struct Planned {
    index: usize,
    pub path: PathBuf,
    /// `None` for a folder, the mode to give a file otherwise.
    pub file_mode: Option<u32>,
}

/// A release zip whose every entry passed the checks, ready to read one entry at a time.
pub struct CheckedZip<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
    /// The bytes read so far, whatever sizes the archive claimed, and the most it may give.
    read: u64,
    limit: u64,
}

/// Refuses `bytes` larger than setup downloads, or whose SHA-256 is not `sha256`.
pub fn verify(bytes: &[u8], sha256: &str) -> Result<(), ZipFailure> {
    if bytes.len() as u64 > MAX_ZIP {
        return Err(ZipFailure::TooLarge);
    }
    if sha256_hex(bytes) != sha256 { Err(ZipFailure::Mismatch) } else { Ok(()) }
}

/// Every file of the release in `bytes`, once its SHA-256 matched `sha256` and every entry passed the checks, in path
/// order and with the top folder stripped. Folders are left out.
pub fn read_verified(bytes: &[u8], sha256: &str) -> Result<Vec<Entry>, ZipFailure> {
    verify(bytes, sha256)?;
    let (mut zip, plan) = CheckedZip::open(bytes)?;
    let mut entries = Vec::new();
    for planned in &plan {
        let Some(mode) = planned.file_mode else { continue };
        let mut read = Vec::new();
        zip.copy(planned, |chunk| {
            read.extend_from_slice(chunk);
            Ok::<(), ZipFailure>(())
        })?;
        entries.push(Entry { path: planned.path.clone(), bytes: read, mode });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

impl<'a> CheckedZip<'a> {
    /// Opens `bytes` and checks every entry before any is read. It does not check the SHA-256: see `verify`.
    pub fn open(bytes: &'a [u8]) -> Result<(Self, Vec<Planned>), ZipFailure> {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| ZipFailure::BadArchive)?;
        let plan = plan(&mut archive)?;
        Ok((Self { archive, read: 0, limit: MAX_UNPACKED }, plan))
    }

    /// Reads the file `planned` and hands its bytes to `take`, a chunk at a time. The bytes read are counted again,
    /// whatever sizes the archive claimed, and more than `MAX_UNPACKED` in all refuses the archive. An entry that does
    /// not read, whose checksum does not match or that is larger than its header says, is `BadArchive`.
    pub fn copy<E: From<ZipFailure>>(&mut self, planned: &Planned, mut take: impl FnMut(&[u8]) -> Result<(), E>) -> Result<(), E> {
        let mut entry = self.archive.by_index(planned.index).map_err(|_| ZipFailure::BadArchive)?;
        let mut buffer = vec![0; 64 * 1024];
        loop {
            let read = match entry.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(ZipFailure::BadArchive.into()),
            };
            self.read += read as u64;
            if self.read > self.limit {
                return Err(ZipFailure::BadArchive.into());
            }
            take(&buffer[..read])?;
        }
    }
}

/// Every entry of the archive, checked before anything is read. One entry that could lead outside the folder, a link
/// or any other special file, an encrypted entry, two names one disk would take for the same name, an entry in a top
/// `.jobdesk` folder, where the kept base goes, or more than the limits allow refuses the whole archive.
fn plan<R: Read + io::Seek>(archive: &mut ZipArchive<R>) -> Result<Vec<Planned>, ZipFailure> {
    if archive.len() > MAX_ENTRIES {
        return Err(ZipFailure::BadArchive);
    }
    // The zip reader keeps one entry of each exact name, so two with the same name are found here, with entries
    // whose headers disagree or whose bytes overlap.
    archive.validate_header_consistency().map_err(|_| ZipFailure::BadArchive)?;
    let mut top: Option<String> = None;
    let mut total: u64 = 0;
    let mut plan = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index).map_err(|_| ZipFailure::BadArchive)?;
        if entry.encrypted() || !matches!(entry.compression(), CompressionMethod::Stored | CompressionMethod::Deflated) {
            return Err(ZipFailure::BadArchive);
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
            return Err(ZipFailure::BadArchive);
        }
        let name = entry.name().map_err(|_| ZipFailure::BadArchive)?;
        let (first, rest) = split_entry(&name, is_dir).ok_or(ZipFailure::BadArchive)?;
        if top.get_or_insert_with(|| first.to_owned()) != first {
            return Err(ZipFailure::BadArchive);
        }
        // The kept base takes `.jobdesk`, spelled any way the disk takes for the same name.
        if rest.as_deref().and_then(|path| path.iter().next()).is_some_and(|part| compared(Path::new(part)) == installed_source::DIR) {
            return Err(ZipFailure::BadArchive);
        }
        // The size the archive claims. The bytes read are counted again in `CheckedZip::copy`.
        total = total.saturating_add(ZipFile::size(&entry));
        if total > MAX_UNPACKED {
            return Err(ZipFailure::BadArchive);
        }
        match (rest, is_dir) {
            // The top folder itself.
            (None, true) => {}
            // A file beside the top folder.
            (None, false) => return Err(ZipFailure::BadArchive),
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
fn check_names(plan: &[Planned]) -> Result<(), ZipFailure> {
    // Each name as the disk compares it, with its spelling and whether it is a folder.
    let mut seen: HashMap<String, (&Path, bool)> = HashMap::with_capacity(plan.len());
    for planned in plan {
        if seen.insert(compared(&planned.path), (&planned.path, planned.file_mode.is_none())).is_some() {
            return Err(ZipFailure::BadArchive);
        }
    }
    // The folders each entry needs, which an archive may leave out.
    for planned in plan {
        for folder in planned.path.ancestors().skip(1).filter(|folder| !folder.as_os_str().is_empty()) {
            match seen.entry(compared(folder)) {
                Slot::Vacant(slot) => {
                    slot.insert((folder, true));
                }
                Slot::Occupied(slot) if *slot.get() != (folder, true) => return Err(ZipFailure::BadArchive),
                Slot::Occupied(_) => {}
            }
        }
    }
    Ok(())
}

/// `path` as a disk that ignores case and Unicode composition compares it.
pub fn compared(path: &Path) -> String {
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

/// Fictional release zips for tests, and the ones every reader of a release must refuse.
#[cfg(test)]
pub mod fixtures {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use crate::shared_kernel::release_zip::{MAX_ENTRIES, MAX_UNPACKED};

    pub const TOP: &str = "ai-job-search-1.7.5/";

    #[derive(Clone, Copy)]
    pub enum Entry<'a> {
        File(&'a str, &'a [u8]),
        Mode(&'a str, &'a [u8], u32),
        Dir(&'a str),
        Link(&'a str, &'a str),
    }

    /// A fictional release zip, its entries named as written.
    pub fn zip_of(entries: &[Entry]) -> Vec<u8> {
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

    /// A fictional release: a readme, a command in a folder the archive names, and a script in one it leaves out,
    /// whose mode asks for set-id bits and shared write.
    pub fn release() -> Vec<u8> {
        let bytes = zip_of(&[
            Entry::Dir(TOP),
            Entry::File("ai-job-search-1.7.5/README.md", b"# ai-job-search"),
            Entry::Dir("ai-job-search-1.7.5/.claude/commands/"),
            Entry::File("ai-job-search-1.7.5/.claude/commands/apply.md", b"apply"),
            Entry::Mode("ai-job-search-1.7.5/scripts/run.sh", b"#!/bin/sh", 0o777),
        ]);
        with_mode(bytes, "ai-job-search-1.7.5/scripts/run.sh", 0o106_777)
    }

    /// Releases with one entry that refuses the whole archive, each with what it shows.
    pub fn refused_entries() -> Vec<(&'static str, Vec<u8>)> {
        let good = Entry::File("ai-job-search-1.7.5/README.md", b"readme");
        vec![
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
        ]
    }

    /// Releases past the limits, each with what it shows.
    pub fn over_limits() -> Vec<(&'static str, Vec<u8>)> {
        // Large but compressing to almost nothing, so the test zip stays small.
        let big = vec![0; (MAX_UNPACKED / 2 + 1) as usize];
        let too_large = zip_of(&[Entry::File("ai-job-search-1.7.5/a", &big), Entry::File("ai-job-search-1.7.5/b", &big)]);
        let names: Vec<String> = (0..=MAX_ENTRIES).map(|n| format!("ai-job-search-1.7.5/{n}")).collect();
        let too_many = zip_of(&names.iter().map(|name| Entry::Mode(name, b"", 0o644)).collect::<Vec<_>>());
        vec![("too large", too_large), ("too many", too_many), ("sizes under-reported", under_reported())]
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{Entry, TOP, over_limits, refused_entries, release, zip_of};
    use super::*;

    /// Every file of `bytes`, read with its own hash.
    fn read(bytes: &[u8]) -> Result<Vec<super::Entry>, ZipFailure> {
        read_verified(bytes, &sha256_hex(bytes))
    }

    #[test]
    fn a_release_reads_in_path_order_without_its_top_folder_or_its_folders() {
        let files = read(&release()).unwrap();
        let paths: Vec<&Path> = files.iter().map(|file| file.path.as_path()).collect();
        assert_eq!(paths, [Path::new(".claude/commands/apply.md"), Path::new("README.md"), Path::new("scripts/run.sh")]);
        assert_eq!(files[0].bytes, b"apply");
        assert_eq!(files[1].bytes, b"# ai-job-search");
        assert_eq!(files[2].bytes, b"#!/bin/sh");
        assert_eq!(files[2].mode, 0o755, "no set-id bits and no shared write");
    }

    #[test]
    fn a_mismatch_is_refused_before_the_archive_is_read() {
        let wrong = sha256_hex(b"another release");
        assert_eq!(read_verified(&release(), &wrong), Err(ZipFailure::Mismatch));
        // Neither a zip nor a safe one: the hash is checked first, so neither is looked at.
        assert_eq!(read_verified(b"not a zip", &wrong), Err(ZipFailure::Mismatch));
        let (_, link) = refused_entries().into_iter().find(|(what, _)| *what == "a link").unwrap();
        assert_eq!(read_verified(&link, &wrong), Err(ZipFailure::Mismatch));
    }

    #[test]
    fn a_zip_larger_than_setup_downloads_is_refused() {
        let bytes = vec![0; MAX_ZIP as usize + 1];
        assert_eq!(read_verified(&bytes, &sha256_hex(b"")), Err(ZipFailure::TooLarge));
    }

    #[test]
    fn each_refused_entry_kind_refuses_the_whole_archive() {
        for (what, bytes) in refused_entries().into_iter().chain(over_limits()) {
            assert_eq!(read(&bytes), Err(ZipFailure::BadArchive), "{what}");
        }
        assert_eq!(read(b"not a zip"), Err(ZipFailure::BadArchive));
    }

    #[test]
    fn the_refused_fixtures_keep_every_case() {
        // Every suite that loops over these lists loses a case silently when one is dropped, so the names are pinned.
        let names = |cases: Vec<(&'static str, Vec<u8>)>| cases.into_iter().map(|(what, _)| what).collect::<Vec<_>>();
        assert_eq!(
            names(refused_entries()),
            [
                "an absolute path",
                "a parent part",
                "a parent top",
                "a dot part",
                "a backslash",
                "a drive",
                "an empty part",
                "a link",
                "a file outside the top folder",
                "a file beside the top folder",
                "a fifo",
                "a link by its mode",
                "a folder that says it is a file",
                "an encrypted entry",
                "a file in the base's folder",
                "the base's folder in capitals",
                "the base's folder alone",
                "a file named as the base's folder",
                "a Windows device name",
                "a trailing dot",
                "a duplicate name",
                "names that differ only in case",
                "names that differ only in how an accent is composed",
                "a file a folder also uses",
                "a folder spelled two ways",
            ]
        );
        assert_eq!(names(over_limits()), ["too large", "too many", "sizes under-reported"]);
    }

    #[test]
    fn the_bytes_read_are_counted_whatever_the_headers_claim() {
        // Each file's bytes past its header's size, and headers that disagree, are refused before this count; so is
        // a total of the headers' sizes past the limit. A smaller limit shows the count of bytes read refuses too.
        let bytes = zip_of(&[Entry::Dir(TOP), Entry::File("ai-job-search-1.7.5/a", &[0; 600]), Entry::File("ai-job-search-1.7.5/b", &[0; 600])]);
        let read_all = |limit| {
            let (mut zip, plan) = CheckedZip::open(&bytes).unwrap();
            zip.limit = limit;
            plan.iter().try_for_each(|planned| zip.copy(planned, |_| Ok::<(), ZipFailure>(())))
        };
        assert_eq!(read_all(1_000), Err(ZipFailure::BadArchive));
        assert_eq!(read_all(1_200), Ok(()));
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
            assert_eq!(check_names(&plan), Err(ZipFailure::BadArchive), "{:?}", plan.map(|planned| planned.path));
        }
        let allowed = [planned("docs/a.md", false), planned("docs", true), planned("docs/b.md", false), planned("Docs.md", false)];
        assert_eq!(check_names(&allowed), Ok(()));
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
