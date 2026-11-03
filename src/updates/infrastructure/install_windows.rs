//! Restart to update on Windows: the signed setup runs in its passive mode, then opens Jobdesk again.
//! Built on every computer, so its tests run everywhere, and called only on Windows.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::shared_kernel::process_tree::no_window;
use crate::updates::domain::{ApplyFailure, InstallBlock, RestartPlan};
use crate::updates::infrastructure::signature::{UpdateKey, signed_by};

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
/// `Err` says why it did not start, and this copy should open again.
pub fn apply(plan: &RestartPlan, exe: &Path, args: &[OsString], key: Option<&UpdateKey>) -> Result<(), ApplyFailure> {
    if !key.is_some_and(|key| signed_by(key, &plan.file, &plan.signature).unwrap_or(false)) {
        return Err(ApplyFailure::BadSignature);
    }
    if block(exe).is_some() {
        return Err(ApplyFailure::NotInstalled);
    }
    installer(&plan.file, workspace(args).as_deref()).spawn().map(drop).map_err(|_| ApplyFailure::Start)
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
        assert_eq!(apply(&plan(fixture(WINDOWS)), &exe, &[], None), Err(ApplyFailure::BadSignature), "no key, no apply");
    }

    #[test]
    fn a_copy_no_installer_put_in_place_is_not_replaced() {
        let folder = tempfile::tempdir().unwrap();
        let exe = folder.path().join("jobdesk.exe");
        assert_eq!(apply(&plan(fixture(WINDOWS)), &exe, &[], Some(&fixture_key())), Err(ApplyFailure::NotInstalled));
    }

    // Unix only, so no Windows runner ever tries to start the stand-in.
    #[cfg(unix)]
    #[test]
    fn a_setup_that_cannot_start_says_so() {
        let folder = tempfile::tempdir().unwrap();
        fs::write(folder.path().join(UNINSTALLER), "").unwrap();
        // The stand-in setup is signed but is no program, so it cannot start.
        let setup = folder.path().join(WINDOWS);
        fs::copy(fixture(WINDOWS), &setup).unwrap();
        assert_eq!(apply(&plan(setup), &folder.path().join("jobdesk.exe"), &[], Some(&fixture_key())), Err(ApplyFailure::Start));
    }
}
