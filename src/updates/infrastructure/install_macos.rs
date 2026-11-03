//! Restart to update on macOS: the signed archive is unpacked beside the bundle, swapped in for it, then opened.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Component, Path, PathBuf};
use std::process::{self, Command};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

use crate::shared_kernel::process_tree::no_window;
use crate::updates::domain::{ApplyFailure, InstallBlock, RestartPlan};
use crate::updates::infrastructure::signature::{UpdateKey, signed_by};

/// The one folder the archive may hold, as cargo packager writes it.
const BUNDLE: &str = "Jobdesk.app";

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

/// Checks the archive once more, unpacks it beside the bundle and swaps it in. On any failure the old bundle is back
/// where it was, and nothing Jobdesk made beside it is left.
pub fn apply(plan: &RestartPlan, exe: &Path, key: Option<&UpdateKey>) -> Result<(), ApplyFailure> {
    if !key.is_some_and(|key| signed_by(key, &plan.file, &plan.signature).unwrap_or(false)) {
        return Err(ApplyFailure::BadSignature);
    }
    let (Some(bundle), Some(program)) = (bundle_of(exe), exe.file_name()) else { return Err(ApplyFailure::NotInstalled) };
    replace(&plan.file, &bundle, program).map_err(|_| ApplyFailure::Replace)
}

/// Opens whatever copy is at the bundle of `exe` now, as a new instance, with `args`. A program outside a bundle is
/// started directly.
pub fn reopen(exe: &Path, args: &[OsString]) -> io::Result<()> {
    let Some(bundle) = bundle_of(exe) else {
        let mut program = Command::new(exe);
        no_window(&mut program);
        return program.args(args).spawn().map(drop);
    };
    let mut open = Command::new("/usr/bin/open");
    no_window(&mut open);
    let status = open.arg("-n").arg(&bundle).arg("--args").args(args).status()?;
    if status.success() { Ok(()) } else { Err(io::Error::other("open failed")) }
}

fn replace(archive: &Path, bundle: &Path, program: &OsStr) -> io::Result<()> {
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

/// Moves the old bundle aside and the new one into its place. If the new one does not go in, the old one comes back.
fn swap(bundle: &Path, new: &Path, aside: &Path) -> io::Result<()> {
    fs::rename(bundle, aside)?;
    if let Err(error) = fs::rename(new, bundle) {
        fs::rename(aside, bundle)?;
        return Err(error);
    }
    Ok(())
}

/// Unpacks a `.tar.gz` that holds only `Jobdesk.app/` into `into`. Plain files keep their permission bits, without
/// the set-id bits or group and other write. A path that leaves the bundle or passes through a link, a hard link,
/// a device, or a link that is absolute or climbs with `..` refuses the whole archive. So every link points down,
/// and none can lead out of the bundle.
fn unpack(archive: &Path, into: &Path) -> io::Result<()> {
    let refuse = |why: &str| Err(io::Error::other(why.to_owned()));
    let mut archive = Archive::new(GzDecoder::new(BufReader::new(File::open(archive)?)));
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
    fn archive(dir: &Path, entries: &[(&str, EntryType, &str)]) -> PathBuf {
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
        path
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
    fn a_program_outside_a_bundle_is_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(apply(&plan(fixture(MAC)), &root.path().join("jobdesk"), Some(&fixture_key())), Err(ApplyFailure::NotInstalled));
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
        assert!(replace(Path::new("/no/such/archive.tar.gz"), &bundle, program).is_err());
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
