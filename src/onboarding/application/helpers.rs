use std::sync::Arc;

use crate::onboarding::domain::{Failure, Helper, HelperInstaller, HelperProbe, InstallPhase, Outcome};

/// The Computer step: checks Python, Bun and LaTeX, and installs a missing one in the home folder. Each call blocks, so
/// the window calls them on a worker.
#[derive(Clone)]
pub struct HelperSetup {
    probe: Arc<dyn HelperProbe>,
    installer: Arc<dyn HelperInstaller>,
}

impl HelperSetup {
    /// Removes the half installs and downloads a Jobdesk that no longer runs left, so that helper starts again. A
    /// finished helper stays.
    pub fn new(probe: Arc<dyn HelperProbe>, installer: Arc<dyn HelperInstaller>) -> Self {
        installer.clear_leftovers();
        Self { probe, installer }
    }

    /// Whether the helper's programs run now.
    pub fn is_here(&self, helper: Helper) -> bool {
        self.probe.is_here(helper)
    }

    /// Installs `helper` unless it is already here, then checks that its programs run. `phase` hears how far the
    /// install has got.
    pub fn install(&self, helper: Helper, cancelled: &dyn Fn() -> bool, phase: &mut dyn FnMut(InstallPhase)) -> Outcome {
        if self.probe.is_here(helper) {
            return Outcome::AlreadyHere;
        }
        match self.installer.install(helper, &|| self.probe.is_here(helper), cancelled, phase) {
            Ok(()) if self.probe.is_here(helper) => Outcome::Installed,
            Ok(()) => Outcome::CouldNotInstall(Failure::DoesNotRun),
            Err(failure) => Outcome::CouldNotInstall(failure),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    use super::*;

    /// The helpers whose programs run.
    struct Here(Mutex<BTreeSet<Helper>>);

    impl HelperProbe for Here {
        fn is_here(&self, helper: Helper) -> bool {
            self.0.lock().unwrap().contains(&helper)
        }
    }

    /// Puts the helper in `Here` when `end` is `Ok(true)`, installs one that does not run for `Ok(false)`, and fails
    /// with an `Err`. It counts its installs and its cleanups.
    struct Installer {
        here: Arc<Here>,
        end: Result<bool, Failure>,
        installs: Mutex<u32>,
        cleared: Mutex<u32>,
    }

    impl HelperInstaller for Installer {
        fn install(&self, helper: Helper, _: &dyn Fn() -> bool, _: &dyn Fn() -> bool, phase: &mut dyn FnMut(InstallPhase)) -> Result<(), Failure> {
            *self.installs.lock().unwrap() += 1;
            phase(InstallPhase::Checking);
            let runs = self.end?;
            if runs {
                self.here.0.lock().unwrap().insert(helper);
            }
            Ok(())
        }

        fn clear_leftovers(&self) {
            *self.cleared.lock().unwrap() += 1;
        }
    }

    fn setup(here: &[Helper], end: Result<bool, Failure>) -> (HelperSetup, Arc<Installer>) {
        let probe = Arc::new(Here(Mutex::new(here.iter().copied().collect())));
        let installer = Arc::new(Installer { here: Arc::clone(&probe), end, installs: Mutex::new(0), cleared: Mutex::new(0) });
        (HelperSetup::new(probe, Arc::clone(&installer) as Arc<dyn HelperInstaller>), installer)
    }

    #[test]
    fn opening_clears_what_a_stopped_install_left() {
        let (_, installer) = setup(&[], Ok(true));
        assert_eq!(*installer.cleared.lock().unwrap(), 1);
    }

    #[test]
    fn a_helper_already_here_is_left_alone() {
        let (helpers, installer) = setup(&[Helper::Python], Ok(true));
        assert!(helpers.is_here(Helper::Python));
        assert_eq!(helpers.install(Helper::Python, &|| false, &mut |_| {}), Outcome::AlreadyHere);
        assert_eq!(*installer.installs.lock().unwrap(), 0);
    }

    #[test]
    fn a_missing_helper_is_installed_then_checked_again() {
        let (helpers, _) = setup(&[], Ok(true));
        let mut heard = Vec::new();
        assert!(!helpers.is_here(Helper::Bun));
        assert_eq!(helpers.install(Helper::Bun, &|| false, &mut |phase| heard.push(phase)), Outcome::Installed);
        assert_eq!(heard, [InstallPhase::Checking]);
        assert!(helpers.is_here(Helper::Bun));
    }

    #[test]
    fn a_failed_install_or_one_that_does_not_run_could_not_install() {
        let (helpers, _) = setup(&[], Err(Failure::Mismatch));
        assert_eq!(helpers.install(Helper::Latex, &|| false, &mut |_| {}), Outcome::CouldNotInstall(Failure::Mismatch));
        let (helpers, _) = setup(&[], Ok(false));
        assert_eq!(helpers.install(Helper::Latex, &|| false, &mut |_| {}), Outcome::CouldNotInstall(Failure::DoesNotRun));
    }
}
