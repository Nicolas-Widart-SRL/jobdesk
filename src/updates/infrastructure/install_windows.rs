//! Restart to update on Windows: the signed setup runs in its passive mode, then opens Jobdesk again.
//! Built on every computer, so its tests run everywhere, and called only on Windows.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use crate::shared_kernel::process_tree::no_window;
use crate::updates::domain::{ApplyFailure, InstallBlock, Platform, RestartPlan};
use crate::updates::infrastructure::download::{MAX_DOWNLOAD, new_private_file, read_build};
use crate::updates::infrastructure::signature::{UpdateKey, signs};

/// The file the Jobdesk installer puts beside the program, and so the sign that an installer put it there.
const UNINSTALLER: &str = "uninstall.exe";
/// The installer reopens Jobdesk with no arguments, so the folder that was open goes in this variable instead.
const WORKSPACE_VARIABLE: &str = "JOBDESK_WORKSPACE";

/// Why the copy at `exe` cannot replace itself: only one an installer put in place can.
pub fn block(exe: &Path) -> Option<InstallBlock> {
    let installed = exe.parent().is_some_and(|folder| folder.join(UNINSTALLER).is_file());
    (!installed).then_some(InstallBlock::NotInstalled)
}

/// Checks the setup once more, then starts it and returns, so this copy ends and the setup can replace it.
/// What starts is a copy that was checked through the handle that holds it, so it cannot change between the check
/// and the start. `Err` says why it did not start, and this copy should open again.
pub fn apply(plan: &RestartPlan, exe: &Path, args: &[OsString], key: Option<&UpdateKey>) -> Result<(), ApplyFailure> {
    if block(exe).is_some() {
        return Err(ApplyFailure::NotInstalled);
    }
    let (setup, held) = prepare(plan, key)?;
    let started = installer(&setup, workspace(args).as_deref()).spawn().map(drop).map_err(|_| ApplyFailure::Start);
    drop(held);
    started
}

/// Copies the setup into a folder made just now beside it, opens the copy so that, on Windows, nothing may write or
/// delete it while the handle is open, and checks the bytes read through that handle. Gives the copy and the handle.
fn prepare(plan: &RestartPlan, key: Option<&UpdateKey>) -> Result<(PathBuf, File), ApplyFailure> {
    let asset = Platform::WindowsX64.asset_name(&plan.version);
    let bytes = read_build(&plan.file).map_err(|_| ApplyFailure::Missing)?;
    let folder = plan.file.with_file_name(format!("apply-{}", process::id()));
    let setup = folder.join(&asset);
    fresh_folder(&folder).and_then(|()| new_private_file(&setup)).and_then(|mut copy| copy.write_all(&bytes)).map_err(|_| ApplyFailure::Start)?;
    let mut held = hold(&setup).map_err(|_| ApplyFailure::Start)?;
    let mut checked = Vec::new();
    (&mut held).take(MAX_DOWNLOAD + 1).read_to_end(&mut checked).map_err(|_| ApplyFailure::Start)?;
    if !key.is_some_and(|key| signs(key, &checked, &plan.signature, &asset)) {
        return Err(ApplyFailure::BadSignature);
    }
    Ok((setup, held))
}

/// An empty folder made now, only the user's, in place of whatever had its name.
fn fresh_folder(folder: &Path) -> io::Result<()> {
    let _ = fs::remove_dir_all(folder);
    let mut builder = fs::DirBuilder::new();
    // Not recursive: a folder that is there already is an error, so the copy always goes in one made now.
    builder.recursive(false);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(folder)
}

/// Opens `path` to read. On Windows others may only read it too: no write and no delete while this is open.
fn hold(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    std::os::windows::fs::OpenOptionsExt::share_mode(&mut options, 0x0000_0001); // FILE_SHARE_READ
    options.open(path)
}

/// `<setup> /P /R /NS`: passive, so it shows only its progress, then reopens Jobdesk, and adds no shortcut again.
fn installer(setup: &Path, workspace: Option<&Path>) -> Command {
    let mut command = Command::new(setup);
    // The setup is a window program, so this hides nothing of it. It only keeps a console from opening.
    no_window(&mut command);
    command.args(["/P", "/R", "/NS"]);
    match workspace {
        Some(folder) => command.env(WORKSPACE_VARIABLE, folder),
        None => command.env_remove(WORKSPACE_VARIABLE),
    };
    command
}

/// The folder the relaunch arguments open: the value of their last `--workspace`.
fn workspace(args: &[OsString]) -> Option<PathBuf> {
    args.windows(2).rev().find(|pair| pair[0] == "--workspace").map(|pair| PathBuf::from(&pair[1]))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;

    use super::*;
    use crate::updates::domain::Version;
    use crate::updates::infrastructure::signature::tests::{fixture, fixture_key, fixture_signature};

    const WINDOWS: &str = "Jobdesk-v0.2.0-windows-x64-setup.exe";

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    /// The signed setup as a download leaves it, in `folder`.
    fn downloaded(folder: &Path) -> PathBuf {
        let file = folder.join(format!("1-{WINDOWS}"));
        fs::copy(fixture(WINDOWS), &file).unwrap();
        file
    }

    fn plan(file: PathBuf) -> RestartPlan {
        RestartPlan { version: Version { major: 0, minor: 2, patch: 0 }, file, signature: fixture_signature(WINDOWS) }
    }

    #[test]
    fn only_a_copy_beside_its_uninstaller_can_update() {
        let folder = tempfile::tempdir().unwrap();
        let exe = folder.path().join("jobdesk.exe");
        assert_eq!(block(&exe), Some(InstallBlock::NotInstalled));
        fs::write(folder.path().join(UNINSTALLER), "").unwrap();
        assert_eq!(block(&exe), None);
    }

    #[test]
    fn the_setup_runs_passive_and_carries_the_open_folder() {
        let command = installer(Path::new("C:/Data/updates/1-setup.exe"), Some(Path::new("C:/Jobs")));
        assert_eq!(command.get_program(), "C:/Data/updates/1-setup.exe");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["/P", "/R", "/NS"]);
        assert_eq!(command.get_envs().collect::<Vec<_>>(), [(OsStr::new(WORKSPACE_VARIABLE), Some(OsStr::new("C:/Jobs")))]);
        let none = installer(Path::new("setup.exe"), None);
        assert_eq!(none.get_envs().collect::<Vec<_>>(), [(OsStr::new(WORKSPACE_VARIABLE), None)], "a folder from before is not passed on");
    }

    #[test]
    fn the_open_folder_is_the_last_workspace_argument() {
        assert_eq!(workspace(&os(&["--verbose", "--workspace", "C:/Jobs"])), Some(PathBuf::from("C:/Jobs")));
        assert_eq!(workspace(&os(&["--workspace", "C:/Old", "--workspace", "C:/New"])), Some(PathBuf::from("C:/New")));
        assert_eq!(workspace(&os(&["--verbose"])), None);
        assert_eq!(workspace(&os(&["--workspace"])), None);
    }

    #[test]
    fn a_setup_that_no_longer_matches_its_signature_never_starts() {
        let folder = tempfile::tempdir().unwrap();
        fs::write(folder.path().join(UNINSTALLER), "").unwrap();
        let exe = folder.path().join("jobdesk.exe");
        let setup = folder.path().join(WINDOWS);
        let mut bytes = fs::read(fixture(WINDOWS)).unwrap();
        bytes[0] ^= 1;
        fs::write(&setup, bytes).unwrap();
        assert_eq!(apply(&plan(setup), &exe, &[], Some(&fixture_key())), Err(ApplyFailure::BadSignature));
        assert_eq!(apply(&plan(downloaded(folder.path())), &exe, &[], None), Err(ApplyFailure::BadSignature), "no key, no apply");
    }

    #[test]
    fn a_setup_signed_for_another_version_never_starts() {
        let folder = tempfile::tempdir().unwrap();
        let older = RestartPlan { signature: fixture_signature("Jobdesk-v0.1.9-windows-x64-setup.exe"), ..plan(downloaded(folder.path())) };
        assert_eq!(prepare(&older, Some(&fixture_key())).err(), Some(ApplyFailure::BadSignature));
    }

    #[test]
    fn a_setup_that_is_gone_is_missing_not_a_bad_signature() {
        let folder = tempfile::tempdir().unwrap();
        fs::write(folder.path().join(UNINSTALLER), "").unwrap();
        let gone = plan(folder.path().join(WINDOWS));
        assert_eq!(apply(&gone, &folder.path().join("jobdesk.exe"), &[], Some(&fixture_key())), Err(ApplyFailure::Missing));
    }

    #[test]
    fn what_starts_is_the_copy_that_was_checked() {
        let folder = tempfile::tempdir().unwrap();
        let original = downloaded(folder.path());
        let (setup, held) = prepare(&plan(original.clone()), Some(&fixture_key())).unwrap();
        assert_ne!(setup, original);
        assert_eq!(setup.parent().unwrap().parent().unwrap(), folder.path(), "the copy sits in a folder of its own beside the download");
        fs::write(&original, "changed after the check").unwrap();
        assert_eq!(fs::read(&setup).unwrap(), fs::read(fixture(WINDOWS)).unwrap());
        #[cfg(windows)]
        assert!(OpenOptions::new().write(true).open(&setup).is_err(), "nothing may write the held copy");
        drop(held);
    }

    #[test]
    fn a_copy_no_installer_put_in_place_is_not_replaced() {
        let folder = tempfile::tempdir().unwrap();
        let exe = folder.path().join("jobdesk.exe");
        assert_eq!(apply(&plan(downloaded(folder.path())), &exe, &[], Some(&fixture_key())), Err(ApplyFailure::NotInstalled));
    }

    // Unix only, so no Windows runner ever tries to start the stand-in.
    #[cfg(unix)]
    #[test]
    fn a_setup_that_cannot_start_says_so() {
        let folder = tempfile::tempdir().unwrap();
        fs::write(folder.path().join(UNINSTALLER), "").unwrap();
        // The stand-in setup is signed but is no program, so it cannot start.
        assert_eq!(apply(&plan(downloaded(folder.path())), &folder.path().join("jobdesk.exe"), &[], Some(&fixture_key())), Err(ApplyFailure::Start));
    }
}
