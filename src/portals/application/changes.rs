use crate::portals::domain::{ChangeError, PortalStore};

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

    /// Moves the portal in `folder` to the Trash, wherever it came from. While `busy`, nothing is moved.
    pub fn remove(&self, folder: &str, busy: bool) -> Result<(), ChangeError> {
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
        assert_eq!(changes.switch("linkedin-search", false, false), Ok(()), "a portal can be switched off");
        assert_eq!(*calls.borrow(), ["set northfield-search false", "set linkedin-search false"]);
    }

    #[test]
    fn a_removal_waits_while_a_run_works() {
        let (changes, calls) = changes();
        assert_eq!(changes.remove("northfield-search", true), Err(ChangeError::Busy));
        assert!(calls.borrow().is_empty());
        assert_eq!(changes.remove("northfield-search", false), Ok(()));
        assert_eq!(*calls.borrow(), ["trash northfield-search"]);
    }

    /// A portal `ai-job-search` shipped before 2.0.0 is removed like any other.
    #[test]
    fn a_portal_from_any_source_is_removed() {
        let (changes, calls) = changes();
        assert_eq!(changes.remove("linkedin-search", false), Ok(()));
        assert_eq!(*calls.borrow(), ["trash linkedin-search"]);
    }
}
