use crate::portals::domain::{ChangeError, Origin, PortalStore, origin};

/// Switches installed portals on or off and moves them to the Trash. Both wait while a run works, because one run works
/// at a time and a run may read or write the portal folders.
pub struct PortalChanges {
    store: Box<dyn PortalStore>,
}

impl PortalChanges {
    pub fn new(store: Box<dyn PortalStore>) -> Self {
        Self { store }
    }

    /// Turns the portal in `folder` on or off. While `busy`, nothing is read or written.
    pub fn switch(&self, folder: &str, on: bool, busy: bool) -> Result<(), ChangeError> {
        if busy {
            return Err(ChangeError::Busy);
        }
        self.store.set_enabled(folder, on)
    }

    /// Moves the portal in `folder` to the Trash. A built-in portal is refused, by the `origin` passed in or by its
    /// folder name, which `origin` matches exactly, as the folder names come from the disk. While `busy`, nothing is
    /// moved.
    pub fn remove(&self, folder: &str, shown: Origin, busy: bool) -> Result<(), ChangeError> {
        if shown == Origin::BuiltIn || origin(folder, &[]).0 == Origin::BuiltIn {
            return Err(ChangeError::BuiltIn);
        }
        if busy {
            return Err(ChangeError::Busy);
        }
        self.store.trash(folder)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::portals::domain::BUILT_IN;

    /// Records each write it is asked for.
    #[derive(Default, Clone)]
    struct FakeStore {
        calls: Rc<RefCell<Vec<String>>>,
    }

    impl PortalStore for FakeStore {
        fn set_enabled(&self, folder: &str, on: bool) -> Result<(), ChangeError> {
            self.calls.borrow_mut().push(format!("set {folder} {on}"));
            Ok(())
        }

        fn trash(&self, folder: &str) -> Result<(), ChangeError> {
            self.calls.borrow_mut().push(format!("trash {folder}"));
            Ok(())
        }
    }

    fn changes() -> (PortalChanges, Rc<RefCell<Vec<String>>>) {
        let store = FakeStore::default();
        let calls = Rc::clone(&store.calls);
        (PortalChanges::new(Box::new(store)), calls)
    }

    #[test]
    fn a_switch_waits_while_a_run_works() {
        let (changes, calls) = changes();
        assert_eq!(changes.switch("northfield-search", false, true), Err(ChangeError::Busy));
        assert!(calls.borrow().is_empty());
        assert_eq!(changes.switch("northfield-search", false, false), Ok(()));
        assert_eq!(changes.switch("linkedin-search", false, false), Ok(()), "a built-in portal can be switched off");
        assert_eq!(*calls.borrow(), ["set northfield-search false", "set linkedin-search false"]);
    }

    #[test]
    fn a_removal_waits_while_a_run_works() {
        let (changes, calls) = changes();
        assert_eq!(changes.remove("northfield-search", Origin::MadeByYou, true), Err(ChangeError::Busy));
        assert!(calls.borrow().is_empty());
        assert_eq!(changes.remove("northfield-search", Origin::Community, false), Ok(()));
        assert_eq!(*calls.borrow(), ["trash northfield-search"]);
    }

    #[test]
    fn a_built_in_portal_is_never_removed() {
        let (changes, calls) = changes();
        assert_eq!(changes.remove("northfield-search", Origin::BuiltIn, false), Err(ChangeError::BuiltIn));
        for (folder, _) in BUILT_IN {
            assert_eq!(changes.remove(folder, Origin::MadeByYou, false), Err(ChangeError::BuiltIn), "{folder}");
        }
        assert_eq!(changes.remove("linkedin-search", Origin::Community, true), Err(ChangeError::BuiltIn));
        assert!(calls.borrow().is_empty());
    }

    /// The screen offers Remove by `origin`, so a folder it calls yours is never refused as built-in, and the other way.
    #[test]
    fn remove_and_origin_agree_on_built_in() {
        for folder in ["linkedin-search", "LinkedIn-search", "JOBNET-SEARCH", "northfield-search"] {
            let (changes, _) = changes();
            let (shown, _) = origin(folder, &[]);
            let refused = changes.remove(folder, shown, false) == Err(ChangeError::BuiltIn);
            assert_eq!(refused, shown == Origin::BuiltIn, "{folder}");
        }
    }
}
