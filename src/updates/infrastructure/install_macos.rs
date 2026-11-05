//! Restart to update on macOS: the signed archive is unpacked beside the bundle, swapped in for it, then opened.

use std::ffi::{CString, OsStr, OsString};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::process::{self, Command};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

use crate::shared_kernel::data_folder::{DATA_DIR_VARIABLE, pass_on_data_folder};
use crate::shared_kernel::process_tree::no_window;
use crate::updates::domain::{ApplyFailure, InstallBlock, Platform, RestartPlan};
use crate::updates::infrastructure::download::read_build;
use crate::updates::infrastructure::signature::{UpdateKey, signs};

/// The one folder the archive may hold, as cargo packager writes it.
const BUNDLE: &str = "Jobdesk.app";
/// What Jobdesk makes beside the bundle, each name ending in the id of the process that made it.
const LEFTOVERS: [&str; 3] = [".Jobdesk.app.update-", ".Jobdesk.app.old-", ".Jobdesk.app.probe-"];

/// The bundle the program at `exe` runs from: `<bundle>.app/Contents/MacOS/<program>`. `None` for a program that
/// runs outside a bundle, such as a development build.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let fits = macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && bundle.extension()? == "app";
    fits.then(|| bundle.to_owned())
}

/// Why the copy at `exe` cannot replace itself. It must run from a bundle, in a folder where Jobdesk may create one
/// beside it, which a mounted disk image or a copy macOS moved aside to open (App Translocation) are not.
pub fn block(exe: &Path) -> Option<InstallBlock> {
    let Some(bundle) = bundle_of(exe) else { return Some(InstallBlock::NotInstalled) };
    let Some(folder) = bundle.parent() else { return Some(InstallBlock::NotInstalled) };
    let probe = folder.join(format!(".{BUNDLE}.probe-{}", process::id()));
    if fs::create_dir(&probe).is_err() {
        return Some(InstallBlock::ReadOnly);
    }
    let _ = fs::remove_dir(&probe);
    None
}

/// Reads the archive once, checks those bytes once more, unpacks the same bytes beside the bundle and swaps them in.
/// On any failure the old bundle is back where it was, and nothing Jobdesk made beside it is left.
pub fn apply(plan: &RestartPlan, exe: &Path, key: Option<&UpdateKey>) -> Result<(), ApplyFailure> {
    let archive = read_build(&plan.file).map_err(|_| ApplyFailure::Missing)?;
    let asset = Platform::MacArm.asset_name(&plan.version);
    if !key.is_some_and(|key| signs(key, &archive, &plan.signature, &asset)) {
        return Err(ApplyFailure::BadSignature);
    }
    let (Some(bundle), Some(program)) = (bundle_of(exe), exe.file_name()) else { return Err(ApplyFailure::NotInstalled) };
    replace(&archive, &bundle, program).map_err(|_| ApplyFailure::Replace)
}

/// Removes what an earlier Restart to update left beside the bundle of `exe` when its process died partway.
/// Only folders of processes that no longer run go, so a Jobdesk that updates now is left alone.
pub fn clear_leftovers(exe: &Path) {
    let Some(folder) = bundle_of(exe).and_then(|bundle| bundle.parent().map(Path::to_owned)) else { return };
    let Ok(entries) = fs::read_dir(&folder) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = LEFTOVERS.iter().find_map(|prefix| name.to_str()?.strip_prefix(prefix)?.parse::<u32>().ok()) else { continue };
        if pid != process::id() && !runs(pid) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Whether a process with this id runs now.
fn runs(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else { return false };
    // SAFETY: signal 0 sends nothing. It only asks whether the process exists.
    let answer = unsafe { libc::kill(pid, 0) };
    answer == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Opens whatever copy is at the bundle of `exe` now, as a new instance, with `args`, on the `moved` data folder if
/// there is one. A program outside a bundle is started directly.
pub fn reopen(exe: &Path, args: &[OsString], moved: Option<&Path>) -> io::Result<()> {
    let Some(bundle) = bundle_of(exe) else {
        let mut program = Command::new(exe);
        no_window(&mut program);
        pass_on_data_folder(&mut program, moved);
        return program.args(args).spawn().map(drop);
    };
    let status = opener(&bundle, args, moved).status()?;
    if status.success() { Ok(()) } else { Err(io::Error::other("open failed")) }
}

/// `open -n <bundle> --args <args>`. A `moved` data folder goes in `--env` too, so the new copy gets it whether or not
/// Launch Services passes on what `open` inherits.
fn opener(bundle: &Path, args: &[OsString], moved: Option<&Path>) -> Command {
    let mut open = Command::new("/usr/bin/open");
    no_window(&mut open);
    pass_on_data_folder(&mut open, moved);
    open.arg("-n");
    if let Some(folder) = moved {
        let mut variable = OsString::from(format!("{DATA_DIR_VARIABLE}="));
        variable.push(folder);
        open.arg("--env").arg(variable);
    }
    open.arg(bundle).arg("--args").args(args);
    open
}

fn replace(archive: &[u8], bundle: &Path, program: &OsStr) -> io::Result<()> {
    let folder = bundle.parent().ok_or_else(|| io::Error::other("no folder"))?;
    let staging = folder.join(format!(".{BUNDLE}.update-{}", process::id()));
    let aside = folder.join(format!(".{BUNDLE}.old-{}", process::id()));
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_dir_all(&aside);
    let result = fs::create_dir(&staging).and_then(|()| unpack(archive, &staging)).and_then(|()| {
        let new = staging.join(BUNDLE);
        if !new.join("Contents/MacOS").join(program).is_file() {
            return Err(io::Error::other("the new bundle has no program"));
        }
        swap(bundle, &new, &aside)
    });
    let _ = fs::remove_dir_all(&staging);
    // Should the old bundle fail to come back, the copy set aside is all that is left of it, and it stays.
    if fs::symlink_metadata(bundle).is_ok() {
        let _ = fs::remove_dir_all(&aside);
    }
    result
}

/// Puts the new bundle in place of the old one. Where the disk can, the two trade places in one step, so there is no
/// moment with no bundle. Elsewhere the old one moves aside first, and comes back if the new one does not go in.
fn swap(bundle: &Path, new: &Path, aside: &Path) -> io::Result<()> {
    if exchange(bundle, new).is_ok() {
        return Ok(());
    }
    fs::rename(bundle, aside)?;
    if let Err(error) = fs::rename(new, bundle) {
        fs::rename(aside, bundle)?;
        return Err(error);
    }
    Ok(())
}

/// Trades the places of two paths in one step, with `renamex_np` and `RENAME_SWAP`.
fn exchange(a: &Path, b: &Path) -> io::Result<()> {
    let a = CString::new(a.as_os_str().as_bytes())?;
    let b = CString::new(b.as_os_str().as_bytes())?;
    // SAFETY: both are valid NUL-terminated paths that live until the call returns.
    let done = unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_SWAP) };
    if done == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// Unpacks a `.tar.gz` that holds only `Jobdesk.app/` into `into`. Plain files keep their permission bits, without
/// the set-id bits or group and other write. A path that leaves the bundle or passes through a link, a hard link,
/// a device, or a link that is absolute or climbs with `..` refuses the whole archive. So every link points down,
/// and none can lead out of the bundle.
fn unpack(archive: &[u8], into: &Path) -> io::Result<()> {
    let refuse = |why: &str| Err(io::Error::other(why.to_owned()));
    let mut archive = Archive::new(GzDecoder::new(archive));
    archive.set_preserve_permissions(false);
    archive.set_mask(0o022);
    archive.set_unpack_xattrs(false);
    archive.set_overwrite(false);
    let mut folders = Vec::new();
    let mut links: Vec<PathBuf> = Vec::new();
    let mut entries = 0;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if matches!(kind, EntryType::XGlobalHeader | EntryType::XHeader) {
            continue;
        }
        let Some(path) = inside(&entry.path()?) else { return refuse("a path leaves the bundle") };
        if links.iter().any(|link| path.starts_with(link) && path != *link) {
            return refuse("a path passes through a link");
        }
        match kind {
            EntryType::Directory => folders.push(entry),
            EntryType::Regular | EntryType::Continuous => unpacked(entry.unpack_in(into)?)?,
            EntryType::Symlink => {
                let Some(target) = entry.link_name()? else { return refuse("a link with no target") };
                if !points_down(&target) {
                    return refuse("a link that does not point down");
                }
                unpacked(entry.unpack_in(into)?)?;
                links.push(path);
            }
            _ => return refuse("an entry that is not a file, a folder or a link"),
        }
        entries += 1;
    }
    if entries == 0 {
        return refuse("an empty archive");
    }
    // Deepest first, as tar does, so a folder without write permission is made only once all it holds is in.
    folders.sort_by(|a, b| b.path_bytes().cmp(&a.path_bytes()));
    for mut folder in folders {
        unpacked(folder.unpack_in(into)?)?;
    }
    Ok(())
}

fn unpacked(done: bool) -> io::Result<()> {
    if done { Ok(()) } else { Err(io::Error::other("an entry was skipped")) }
}

/// `path` with its `.` parts dropped, when it lies inside `Jobdesk.app` with no `..` and no root.
fn inside(path: &Path) -> Option<PathBuf> {
    let mut plain = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => plain.push(name),
            Component::CurDir => {}
            _ => return None,
        }
    }
    plain.starts_with(BUNDLE).then_some(plain)
}

/// Whether a link target names something in the link's own folder or below it: relative, with no `..`.
fn points_down(target: &Path) -> bool {
    target.components().any(|component| matches!(component, Component::Normal(_)))
        && target.components().all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use std::fs::File;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::{Builder, Header};

    use super::*;
    use crate::updates::domain::Version;
    use crate::updates::infrastructure::signature::tests::{fixture, fixture_key, fixture_signature};

    const MAC: &str = "Jobdesk-v0.2.0-macos-arm64.app.tar.gz";

    /// `<root>/Applications/Jobdesk.app` holding an old program, and the path of that program.
    fn installed(root: &Path) -> PathBuf {
        let macos = root.join("Applications/Jobdesk.app/Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        let exe = macos.join("jobdesk");
        fs::write(&exe, "old").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        exe
    }

    fn plan(file: PathBuf) -> RestartPlan {
        RestartPlan { version: Version { major: 0, minor: 2, patch: 0 }, file, signature: fixture_signature(MAC) }
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir).unwrap().flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    /// A `.tar.gz` of `(path, kind, data or link target)` entries, written raw so a bad path stays as it is.
    fn archive(dir: &Path, entries: &[(&str, EntryType, &str)]) -> Vec<u8> {
        let path = dir.join("archive.tar.gz");
        let mut builder = Builder::new(GzEncoder::new(File::create(&path).unwrap(), Compression::default()));
        for (name, kind, data) in entries {
            let mut header = Header::new_old();
            header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_entry_type(*kind);
            header.set_mode(0o755);
            if *kind == EntryType::Symlink {
                header.set_link_name(data).unwrap();
                header.set_size(0);
                header.set_cksum();
                builder.append(&header, io::empty()).unwrap();
            } else {
                let data = if *kind == EntryType::Directory { "" } else { data };
                header.set_size(data.len() as u64);
                header.set_cksum();
                builder.append(&header, data.as_bytes()).unwrap();
            }
        }
        builder.into_inner().unwrap().finish().unwrap();
        fs::read(path).unwrap()
    }

    fn good_entries() -> Vec<(&'static str, EntryType, &'static str)> {
        vec![
            ("Jobdesk.app/", EntryType::Directory, ""),
            ("Jobdesk.app/Contents/", EntryType::Directory, ""),
            ("Jobdesk.app/Contents/MacOS/", EntryType::Directory, ""),
            ("Jobdesk.app/Contents/MacOS/jobdesk", EntryType::Regular, "new"),
        ]
    }

    #[test]
    fn a_program_runs_from_a_bundle_only_inside_one() {
        assert_eq!(bundle_of(Path::new("/Applications/Jobdesk.app/Contents/MacOS/jobdesk")), Some(PathBuf::from("/Applications/Jobdesk.app")));
        assert_eq!(bundle_of(Path::new("/Users/me/Jobdesk Copy.app/Contents/MacOS/jobdesk")), Some(PathBuf::from("/Users/me/Jobdesk Copy.app")));
        assert_eq!(bundle_of(Path::new("/Users/me/jobdesk/target/release/jobdesk")), None);
        assert_eq!(bundle_of(Path::new("/Applications/Jobdesk/Contents/MacOS/jobdesk")), None);
        assert_eq!(bundle_of(Path::new("jobdesk")), None);
    }

    #[test]
    fn a_development_build_or_a_folder_it_cannot_write_blocks_the_update() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(block(&root.path().join("target/release/jobdesk")), Some(InstallBlock::NotInstalled));
        let exe = installed(root.path());
        assert_eq!(block(&exe), None);
        assert_eq!(names(&root.path().join("Applications")), ["Jobdesk.app"], "the probe is gone");
        let applications = root.path().join("Applications");
        fs::set_permissions(&applications, fs::Permissions::from_mode(0o555)).unwrap();
        let blocked = block(&exe);
        fs::set_permissions(&applications, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(blocked, Some(InstallBlock::ReadOnly));
    }

    #[test]
    fn the_signed_archive_replaces_the_bundle_and_its_program_stays_executable() {
        let root = tempfile::tempdir().unwrap();
        let exe = installed(root.path());
        assert_eq!(apply(&plan(fixture(MAC)), &exe, Some(&fixture_key())), Ok(()));
        assert!(fs::read_to_string(&exe).unwrap().contains("Jobdesk 0.2.0"));
        assert_eq!(fs::metadata(&exe).unwrap().permissions().mode() & 0o777, 0o755);
        assert!(exe.parent().unwrap().parent().unwrap().join("Info.plist").is_file());
        assert_eq!(names(&root.path().join("Applications")), ["Jobdesk.app"], "nothing is left beside it");
    }

    #[test]
    fn an_archive_that_fails_its_signature_is_never_unpacked() {
        let root = tempfile::tempdir().unwrap();
        let exe = installed(root.path());
        let mut bytes = fs::read(fixture(MAC)).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let file = root.path().join(MAC);
        fs::write(&file, bytes).unwrap();
        assert_eq!(apply(&plan(file), &exe, Some(&fixture_key())), Err(ApplyFailure::BadSignature));
        assert_eq!(apply(&plan(fixture(MAC)), &exe, None), Err(ApplyFailure::BadSignature));
        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
        assert_eq!(names(&root.path().join("Applications")), ["Jobdesk.app"]);
    }

    #[test]
    fn an_archive_signed_for_another_version_or_gone_is_never_unpacked() {
        let root = tempfile::tempdir().unwrap();
        let exe = installed(root.path());
        let older = RestartPlan { version: Version { major: 0, minor: 1, patch: 9 }, ..plan(fixture(MAC)) };
        assert_eq!(apply(&older, &exe, Some(&fixture_key())), Err(ApplyFailure::BadSignature), "a 0.2.0 signature is no 0.1.9 one");
        assert_eq!(apply(&plan(root.path().join(MAC)), &exe, Some(&fixture_key())), Err(ApplyFailure::Missing));
        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
        assert_eq!(names(&root.path().join("Applications")), ["Jobdesk.app"]);
    }

    #[test]
    fn a_program_outside_a_bundle_is_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(apply(&plan(fixture(MAC)), &root.path().join("jobdesk"), Some(&fixture_key())), Err(ApplyFailure::NotInstalled));
    }

    /// `open` names a moved data folder in `--env`, so the new copy gets it whatever Launch Services passes on. With no
    /// folder, `--env` is left out and the variable is removed.
    #[test]
    fn the_new_copy_opens_on_a_moved_data_folder() {
        let args = [OsString::from("--workspace"), OsString::from("/jobs/ai-job-search")];
        let bundle = Path::new("/Applications/Jobdesk.app");
        let moved = opener(bundle, &args, Some(Path::new("/scratch/jobdesk-data")));
        assert_eq!(moved.get_program(), "/usr/bin/open");
        let expected = ["-n", "--env", "JOBDESK_DATA_DIR=/scratch/jobdesk-data", "/Applications/Jobdesk.app", "--args", "--workspace", "/jobs/ai-job-search"];
        assert_eq!(moved.get_args().collect::<Vec<_>>(), expected);
        assert_eq!(moved.get_envs().collect::<Vec<_>>(), [(OsStr::new(DATA_DIR_VARIABLE), Some(OsStr::new("/scratch/jobdesk-data")))]);

        let usual = opener(bundle, &args, None);
        assert_eq!(usual.get_args().collect::<Vec<_>>(), ["-n", "/Applications/Jobdesk.app", "--args", "--workspace", "/jobs/ai-job-search"]);
        assert_eq!(usual.get_envs().collect::<Vec<_>>(), [(OsStr::new(DATA_DIR_VARIABLE), None)], "a value this copy ignored is not passed on");
    }

    #[test]
    fn a_bad_archive_leaves_the_old_bundle_and_nothing_beside_it() {
        let root = tempfile::tempdir().unwrap();
        let exe = installed(root.path());
        let bundle = bundle_of(&exe).unwrap();
        let program = OsStr::new("jobdesk");
        let mut two_folders = good_entries();
        two_folders.push(("Other.app/evil", EntryType::Regular, "x"));
        let mut climbs = good_entries();
        climbs.push(("Jobdesk.app/../../evil", EntryType::Regular, "x"));
        let mut absolute = good_entries();
        absolute.push(("/tmp/jobdesk-evil", EntryType::Regular, "x"));
        let mut link_out = good_entries();
        link_out.push(("Jobdesk.app/Contents/out", EntryType::Symlink, "../.."));
        let mut link_up = good_entries();
        link_up.push(("Jobdesk.app/Contents/MacOS/up", EntryType::Symlink, "../Info.plist"));
        let mut through_link = good_entries();
        through_link.push(("Jobdesk.app/Contents/down", EntryType::Symlink, "MacOS"));
        through_link.push(("Jobdesk.app/Contents/down/extra", EntryType::Regular, "x"));
        let mut absolute_link = good_entries();
        absolute_link.push(("Jobdesk.app/Contents/out", EntryType::Symlink, "/etc"));
        let mut hard_link = good_entries();
        hard_link.push(("Jobdesk.app/Contents/hard", EntryType::Link, "Jobdesk.app/Contents/MacOS/jobdesk"));
        let no_program = vec![("Jobdesk.app/", EntryType::Directory, ""), ("Jobdesk.app/Contents/Info.plist", EntryType::Regular, "plist")];
        for (name, entries) in [
            ("two folders", two_folders),
            ("climbs", climbs),
            ("absolute", absolute),
            ("link out", link_out),
            ("link up", link_up),
            ("through a link", through_link),
            ("absolute link", absolute_link),
            ("hard link", hard_link),
            ("no program", no_program),
            ("empty", Vec::new()),
        ] {
            let scratch = tempfile::tempdir().unwrap();
            let file = archive(scratch.path(), &entries);
            assert!(replace(&file, &bundle, program).is_err(), "{name}");
            assert_eq!(fs::read_to_string(&exe).unwrap(), "old", "{name}");
            assert_eq!(names(&root.path().join("Applications")), ["Jobdesk.app"], "{name}");
            assert!(!root.path().join("evil").exists() && !Path::new("/tmp/jobdesk-evil").exists(), "{name}");
        }
        assert!(replace(b"not an archive", &bundle, program).is_err());
        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
    }

    #[test]
    fn a_link_inside_the_bundle_is_kept() {
        let root = tempfile::tempdir().unwrap();
        let exe = installed(root.path());
        let mut entries = good_entries();
        entries.push(("Jobdesk.app/Contents/Current", EntryType::Symlink, "MacOS"));
        entries.push(("Jobdesk.app/Contents/Program", EntryType::Symlink, "./Current/jobdesk"));
        let scratch = tempfile::tempdir().unwrap();
        replace(&archive(scratch.path(), &entries), &bundle_of(&exe).unwrap(), OsStr::new("jobdesk")).unwrap();
        let contents = exe.parent().unwrap().parent().unwrap().to_owned();
        assert_eq!(fs::read_to_string(contents.join("Current/jobdesk")).unwrap(), "new");
        assert_eq!(fs::read_to_string(contents.join("Program")).unwrap(), "new");
    }

    #[test]
    fn a_swap_that_fails_halfway_puts_the_old_bundle_back() {
        let root = tempfile::tempdir().unwrap();
        let exe = installed(root.path());
        let bundle = bundle_of(&exe).unwrap();
        let aside = root.path().join("Applications/.Jobdesk.app.old-1");
        assert!(swap(&bundle, &root.path().join("missing/Jobdesk.app"), &aside).is_err());
        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
        assert!(!aside.exists());
    }

    #[test]
    fn the_two_bundles_trade_places_in_one_step() {
        let root = tempfile::tempdir().unwrap();
        let exe = installed(root.path());
        let bundle = bundle_of(&exe).unwrap();
        let new = root.path().join("staging/Jobdesk.app/Contents/MacOS");
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("jobdesk"), "new").unwrap();
        let aside = root.path().join("Applications/.Jobdesk.app.old-1");
        swap(&bundle, &root.path().join("staging/Jobdesk.app"), &aside).unwrap();
        assert_eq!(fs::read_to_string(&exe).unwrap(), "new");
        assert_eq!(fs::read_to_string(root.path().join("staging/Jobdesk.app/Contents/MacOS/jobdesk")).unwrap(), "old");
        assert!(!aside.exists(), "nothing was moved aside");
    }

    #[test]
    fn leftovers_of_a_process_that_died_go_and_those_of_one_that_runs_stay() {
        let root = tempfile::tempdir().unwrap();
        let exe = installed(root.path());
        let applications = root.path().join("Applications");
        let mut ended = Command::new("/usr/bin/true").spawn().unwrap();
        let dead = ended.id();
        ended.wait().unwrap();
        let running = std::os::unix::process::parent_id();
        for name in [format!(".Jobdesk.app.update-{dead}"), format!(".Jobdesk.app.old-{dead}"), format!(".Jobdesk.app.probe-{dead}")] {
            fs::create_dir_all(applications.join(name).join("Jobdesk.app")).unwrap();
        }
        fs::create_dir(applications.join(format!(".Jobdesk.app.update-{running}"))).unwrap();
        fs::create_dir(applications.join(format!(".Jobdesk.app.old-{}", process::id()))).unwrap();
        fs::create_dir(applications.join("Other.app")).unwrap();
        clear_leftovers(&exe);
        let mut kept =
            vec![format!(".Jobdesk.app.old-{}", process::id()), format!(".Jobdesk.app.update-{running}"), "Jobdesk.app".to_owned(), "Other.app".to_owned()];
        kept.sort();
        assert_eq!(names(&applications), kept);
        clear_leftovers(&root.path().join("target/release/jobdesk"));
    }

    #[test]
    fn leftovers_from_an_earlier_try_are_cleared() {
        let root = tempfile::tempdir().unwrap();
        let exe = installed(root.path());
        let applications = root.path().join("Applications");
        fs::create_dir_all(applications.join(format!(".Jobdesk.app.update-{}/Jobdesk.app", process::id()))).unwrap();
        fs::create_dir_all(applications.join(format!(".Jobdesk.app.old-{}", process::id()))).unwrap();
        assert_eq!(apply(&plan(fixture(MAC)), &exe, Some(&fixture_key())), Ok(()));
        assert_eq!(names(&applications), ["Jobdesk.app"]);
    }

    #[test]
    fn a_path_is_inside_only_under_the_bundle_and_a_link_only_points_down() {
        assert_eq!(inside(Path::new("Jobdesk.app/Contents/Info.plist")), Some(PathBuf::from("Jobdesk.app/Contents/Info.plist")));
        assert_eq!(inside(Path::new("./Jobdesk.app/")), Some(PathBuf::from("Jobdesk.app")));
        assert_eq!(inside(Path::new("Other.app/x")), None);
        assert_eq!(inside(Path::new("Jobdesk.app.evil/x")), None);
        assert_eq!(inside(Path::new("Jobdesk.app/../x")), None);
        assert!(points_down(Path::new("MacOS/jobdesk")));
        assert!(points_down(Path::new("./Current")));
        assert!(!points_down(Path::new("../Contents")));
        assert!(!points_down(Path::new("MacOS/../..")));
        assert!(!points_down(Path::new("/etc")));
        assert!(!points_down(Path::new("")));
        assert!(!points_down(Path::new(".")));
    }
}
