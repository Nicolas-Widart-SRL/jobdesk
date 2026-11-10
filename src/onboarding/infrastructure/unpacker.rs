use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zip::read::ZipFile;
use zip::{CompressionMethod, ZipArchive};

use crate::onboarding::domain::{Contents, Failure, MAX_ENTRIES, MAX_UNPACKED, MAX_ZIP, Phase, Unpacker};
use crate::shared_kernel::own_folder::{claim, in_use};

/// The file type bits of a Unix mode, and the two types an archive may hold.
const TYPE_BITS: u32 = 0o170_000;
const REGULAR: u32 = 0o100_000;
const DIRECTORY: u32 = 0o040_000;

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

    fn checked_unpack(&self, zip: &Path, sha256: &str, folder: &Path, cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(Phase)) -> Result<(), Failure> {
        phase(Phase::Checking);
        // Read once, so the bytes that are checked are the bytes that are unpacked.
        let bytes = read_whole(zip)?;
        if sha256_hex(&bytes) != sha256 {
            return Err(Failure::Mismatch);
        }
        phase(Phase::Unpacking);
        let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice())).map_err(|_| Failure::BadArchive)?;
        let plan = plan(&mut archive)?;
        let staging = staging_of(folder, &claim(&self.dir).map_err(|_| Failure::Disk)?)?;
        remove(&staging);
        fs::create_dir(&staging).map_err(|_| Failure::Disk)?;
        let placed = write_all(&mut archive, &plan, &staging, cancelled)
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
        match fs::read_dir(folder) {
            Ok(mut entries) => {
                if entries.next().is_none() {
                    Contents::Empty
                } else {
                    Contents::Files
                }
            }
            Err(_) => Contents::Files,
        }
    }

    fn make(&self, folder: &Path) -> Result<(), Failure> {
        if self.look(folder) == Contents::Missing {
            fs::create_dir_all(folder).map_err(|_| Failure::Disk)?;
        }
        if self.look(folder) == Contents::Empty { Ok(()) } else { Err(Failure::NotEmpty) }
    }

    fn unpack(&self, zip: &Path, sha256: &str, folder: &Path, cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(Phase)) -> Result<(), Failure> {
        let unpacked = self.checked_unpack(zip, sha256, folder, cancelled, phase);
        let _ = fs::remove_file(zip);
        unpacked
    }

    fn clear_leftovers(&self, folder: &Path) {
        let (Some(parent), Some(prefix)) = (folder.parent(), staging_prefix(folder)) else { return };
        let Ok(entries) = fs::read_dir(parent) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|name| name.strip_prefix(prefix.as_str())) else { continue };
            if pid.is_empty() || !pid.bytes().all(|byte| byte.is_ascii_digit()) || in_use(&self.dir.join(pid)) {
                continue;
            }
            remove(&entry.path());
        }
    }
}

/// The start of the name of every staging folder of `folder`: `.<name>.jobdesk-`.
fn staging_prefix(folder: &Path) -> Option<String> {
    Some(format!(".{}.jobdesk-", folder.file_name()?.to_str()?))
}

/// This Jobdesk's staging folder for `folder`: hidden, beside it, so the rename that puts it in place stays on one
/// disk. `own` is this Jobdesk's folder of `onboarding/`, whose name is its pid.
fn staging_of(folder: &Path, own: &Path) -> Result<PathBuf, Failure> {
    let (Some(parent), Some(prefix), Some(pid)) = (folder.parent(), staging_prefix(folder), own.file_name()) else { return Err(Failure::Disk) };
    let mut name = OsString::from(prefix);
    name.push(pid);
    Ok(parent.join(name))
}

/// Removes `path` and what it holds. A link is removed itself, never followed.
fn remove(path: &Path) {
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

/// 64 lowercase hex digits, as `sha256sum` writes them and the pin holds them.
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Every entry of the archive, checked before anything is written. One entry that could lead outside the folder, a
/// link or any other special file, an encrypted entry, or more than the limits allow refuses the whole archive.
fn plan<R: Read + io::Seek>(archive: &mut ZipArchive<R>) -> Result<Vec<Planned>, Failure> {
    if archive.len() > MAX_ENTRIES {
        return Err(Failure::BadArchive);
    }
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
    Ok(plan)
}

/// The top folder of an entry's name and the path under it, or `None` for a name that could lead anywhere but under
/// it: an absolute path, `..` or `.`, an empty part, a backslash, a colon or a NUL.
fn split_entry(name: &str, is_dir: bool) -> Option<(&str, Option<PathBuf>)> {
    let name = if is_dir { name.strip_suffix('/').unwrap_or(name) } else { name };
    let mut parts = name.split('/');
    let first = parts.next()?;
    let mut path = PathBuf::new();
    for part in std::iter::once(first).chain(parts.clone()) {
        let plain = !part.is_empty() && part != "." && part != ".." && !part.contains(['\\', ':', '\0']);
        if !plain {
            return None;
        }
    }
    for part in parts {
        path.push(part);
    }
    Some((first, (path.components().next().is_some()).then_some(path)))
}

/// The mode a file gets: its own read and run bits, never set-id, sticky or group and other write, and always
/// readable and writable by the user.
fn file_mode(mode: Option<u32>) -> u32 {
    mode.map_or(0o644, |mode| mode & 0o755) | 0o600
}

/// Writes every planned entry into `staging`, which is new and holds no link, so no write can leave it. The bytes
/// written are counted again, whatever sizes the archive claimed.
fn write_all<R: Read + io::Seek>(archive: &mut ZipArchive<R>, plan: &[Planned], staging: &Path, cancelled: &dyn Fn() -> bool) -> Result<(), Failure> {
    let mut written: u64 = 0;
    let mut buffer = vec![0; 64 * 1024];
    for planned in plan {
        if cancelled() {
            return Err(Failure::Cancelled);
        }
        let target = staging.join(&planned.path);
        let Some(mode) = planned.file_mode else {
            fs::create_dir_all(&target).map_err(|_| Failure::Disk)?;
            continue;
        };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|_| Failure::Disk)?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, mode);
        #[cfg(not(unix))]
        let _ = mode;
        let mut file = options.open(&target).map_err(|_| Failure::Disk)?;
        let mut entry = archive.by_index(planned.index).map_err(|_| Failure::BadArchive)?;
        loop {
            let read = match entry.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                // A corrupt entry, or one whose checksum does not match.
                Err(_) => return Err(Failure::BadArchive),
            };
            written += read as u64;
            if written > MAX_UNPACKED {
                return Err(Failure::BadArchive);
            }
            file.write_all(&buffer[..read]).map_err(|_| Failure::Disk)?;
        }
    }
    Ok(())
}

/// Puts `staging` in the place of `folder` in one rename, which never replaces a folder that holds anything.
fn put_in_place(staging: &Path, folder: &Path) -> Result<(), Failure> {
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
            let end = self.unpacker.unpack(&zip, sha256, &self.folder, &|| false, &mut |phase| phases.push(phase));
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

    #[test]
    fn a_mismatch_leaves_the_folder_empty_and_deletes_the_zip() {
        let setup = setup();
        let bytes = release();
        let wrong = sha256_hex(b"another release");
        let (end, phases) = setup.unpack(&bytes, &wrong);
        assert_eq!(end, Err(Failure::Mismatch));
        assert_eq!(phases, [Phase::Checking], "nothing is unpacked once the check fails");
        assert!(setup.is_empty());
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
        ];
        for (what, bytes) in refused {
            let setup = setup();
            assert_eq!(setup.unpack(&bytes, &sha256_hex(&bytes)).0, Err(Failure::BadArchive), "{what}");
            assert!(setup.is_empty(), "{what}: nothing is written");
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
        for (what, bytes) in [("too large", too_large), ("too many", too_many)] {
            let setup = setup();
            assert_eq!(setup.unpack(&bytes, &sha256_hex(&bytes)).0, Err(Failure::BadArchive), "{what}");
            assert!(setup.is_empty(), "{what}");
        }
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
    fn a_folder_that_filled_during_the_unpack_keeps_what_it_holds() {
        let setup = setup();
        let bytes = release();
        let zip = setup.zip(&bytes);
        setup.unpacker.make(&setup.folder).unwrap();
        let end = setup.unpacker.unpack(&zip, &sha256_hex(&bytes), &setup.folder, &|| false, &mut |phase| {
            if phase == Phase::Unpacking {
                fs::write(setup.folder.join("cv.pdf"), "mine").unwrap();
            }
        });
        assert_eq!(end, Err(Failure::NotEmpty));
        assert_eq!(fs::read_to_string(setup.folder.join("cv.pdf")).unwrap(), "mine");
        assert_eq!(setup.around(), ["Job search"], "the staging folder goes");
    }

    #[test]
    fn cancel_mid_unpack_leaves_the_folder_empty() {
        let setup = setup();
        let bytes = release();
        let zip = setup.zip(&bytes);
        setup.unpacker.make(&setup.folder).unwrap();
        let end = setup.unpacker.unpack(&zip, &sha256_hex(&bytes), &setup.folder, &|| true, &mut |_| {});
        assert_eq!(end, Err(Failure::Cancelled));
        assert!(setup.is_empty());
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
        let own = staging_of(&setup.folder, &claim(&setup.data.join("onboarding")).unwrap()).unwrap();
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
        for bad in ["", "/top/a", "top/../a", "top/a/..", "..", "top/a\\b", "top/C:", "top/a\0b", "top//a", "top/a/"] {
            assert_eq!(split_entry(bad, false), None, "{bad:?}");
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
