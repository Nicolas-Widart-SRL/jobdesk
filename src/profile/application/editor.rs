use crate::profile::application::view::{ProfileView, profile_view};
use crate::profile::domain::{BlockKey, ProfileStore, SaveCheck, blocks, check, find, insert_at, replace};
use crate::shared_kernel::LoadError;

/// What a save did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// The draft is in the file, and the editor closed.
    Saved,
    /// A run works, so nothing was read or written.
    Busy,
    /// The block changed on disk since the editor opened it. `now` is the block as the file holds it. Nothing was written.
    Changed { now: String },
    /// The file could not be read or written. The draft stays.
    Failed(LoadError),
}

/// The one block the user edits, and the text it had when they opened it.
#[derive(Debug)]
struct Open {
    key: BlockKey,
    opened_with: String,
    draft: String,
    /// A new block goes at the end of the block `key` names, instead of replacing it.
    new: bool,
    /// The block as the file held it when the last save found it changed.
    theirs: Option<String>,
}

/// Edits one block of the profile at a time, and saves it into its file without touching any other byte.
pub struct ProfileEditor {
    store: Box<dyn ProfileStore>,
    open: Option<Open>,
}

impl ProfileEditor {
    pub fn new(store: Box<dyn ProfileStore>) -> Self {
        Self { store, open: None }
    }

    /// The profile as the files hold it now. A file that cannot be read hides its topics.
    pub fn current_view(&self) -> ProfileView {
        profile_view(&|file| self.store.read(file)).0
    }

    /// Opens the block `key` names, as `text`, which is how the user saw it.
    pub fn open(&mut self, key: BlockKey, text: String) {
        self.open = Some(Open { key, draft: text.clone(), opened_with: text, new: false, theirs: None });
    }

    /// Opens a new block that a save puts at the end of the block `after` names, such as a role at the end of its `##`.
    pub fn open_new(&mut self, after: BlockKey, stub: String) {
        self.open = Some(Open { key: after, draft: stub.clone(), opened_with: stub, new: true, theirs: None });
    }

    /// The block being edited. For a new block, the block it goes at the end of.
    pub fn editing(&self) -> Option<&BlockKey> {
        self.open.as_ref().map(|open| &open.key)
    }

    pub fn draft_mut(&mut self) -> Option<&mut String> {
        self.open.as_mut().map(|open| &mut open.draft)
    }

    pub fn is_dirty(&self) -> bool {
        self.open.as_ref().is_some_and(|open| open.draft != open.opened_with)
    }

    /// Drops the draft for the block as the last save found it on disk, which the next save then compares against.
    pub fn use_new_version(&mut self) {
        if let Some(open) = self.open.as_mut()
            && let Some(theirs) = open.theirs.take()
        {
            open.draft = theirs.clone();
            open.opened_with = theirs;
        }
    }

    pub fn cancel(&mut self) {
        self.open = None;
    }

    /// Reads the file again just before writing, and replaces only the block. While `busy`, a run may be writing the
    /// same file, so it returns before reading anything. A block that changed on disk is written over only with
    /// `over_theirs`. A block that is gone goes to the end of its `##`, or of the file. With nothing open it does nothing.
    pub fn save(&mut self, busy: bool, over_theirs: bool) -> SaveOutcome {
        if busy {
            return SaveOutcome::Busy;
        }
        let Some(open) = self.open.as_mut() else { return SaveOutcome::Saved };
        let text = match self.store.read(open.key.file) {
            Ok(text) => text.unwrap_or_default(),
            Err(error) => return SaveOutcome::Failed(error),
        };
        let saved = if open.new {
            insert_at(&text, end_of(&text, &open.key), &open.draft)
        } else {
            match check(&text, &open.key, &open.opened_with) {
                SaveCheck::Same(range) => replace(&text, range, &open.draft),
                SaveCheck::Changed { now } if !over_theirs => {
                    open.theirs = Some(now.clone());
                    return SaveOutcome::Changed { now };
                }
                SaveCheck::Changed { .. } => match find(&text, &open.key) {
                    Some(range) => replace(&text, range, &open.draft),
                    None => insert_at(&text, end_of_parent(&text, &open.key), &open.draft),
                },
                SaveCheck::Gone => insert_at(&text, end_of_parent(&text, &open.key), &open.draft),
            }
        };
        match self.store.write(open.key.file, &saved) {
            Ok(()) => {
                self.open = None;
                SaveOutcome::Saved
            }
            Err(error) => SaveOutcome::Failed(error),
        }
    }
}

/// The end of the block `key` names, or of the file when it is gone.
fn end_of(text: &str, key: &BlockKey) -> usize {
    find(text, key).map_or(text.len(), |range| range.end)
}

/// The end of the first `##` named as the parent of `key`, or of the file when there is none.
fn end_of_parent(text: &str, key: &BlockKey) -> usize {
    let Some(parent) = key.parent.as_deref() else { return text.len() };
    blocks(text).into_iter().find(|block| block.level == 2 && block.heading == parent).map_or(text.len(), |block| block.range.end)
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;

    use super::*;
    use crate::profile::domain::ProfileFile;

    /// Files in memory, counting every read and write.
    #[derive(Default)]
    struct Memory {
        files: RefCell<HashMap<ProfileFile, String>>,
        reads: Cell<usize>,
        writes: Cell<usize>,
        fail_write: Cell<bool>,
    }

    struct Shared(Rc<Memory>);

    impl ProfileStore for Shared {
        fn read(&self, file: ProfileFile) -> Result<Option<String>, LoadError> {
            self.0.reads.set(self.0.reads.get() + 1);
            Ok(self.0.files.borrow().get(&file).cloned())
        }

        fn write(&self, file: ProfileFile, text: &str) -> Result<(), LoadError> {
            if self.0.fail_write.get() {
                return Err(LoadError::new(file.name(), "disk full"));
            }
            self.0.writes.set(self.0.writes.get() + 1);
            self.0.files.borrow_mut().insert(file, text.to_owned());
            Ok(())
        }
    }

    const TEXT: &str = "## Identity\nName: Ada\n\n## Professional Experience\n\n### Lead - Calder (2021 – now)\nShipped.\n\n## Education\nMSc\n";

    fn editor(text: &str) -> (Rc<Memory>, ProfileEditor) {
        let memory = Rc::new(Memory::default());
        memory.files.borrow_mut().insert(ProfileFile::Candidate, text.to_owned());
        (memory.clone(), ProfileEditor::new(Box::new(Shared(memory))))
    }

    fn key(level: u8, heading: &str, parent: Option<&str>) -> BlockKey {
        BlockKey { file: ProfileFile::Candidate, level, heading: heading.to_owned(), parent: parent.map(str::to_owned), nth: 0 }
    }

    fn file(memory: &Memory) -> String {
        memory.files.borrow()[&ProfileFile::Candidate].clone()
    }

    #[test]
    fn busy_returns_before_any_read() {
        let (memory, mut editor) = editor(TEXT);
        editor.open(key(2, "Education", None), "## Education\nMSc".to_owned());
        *editor.draft_mut().unwrap() = "## Education\nPhD".to_owned();
        assert_eq!(editor.save(true, true), SaveOutcome::Busy);
        assert_eq!((memory.reads.get(), memory.writes.get()), (0, 0));
        assert!(editor.is_dirty());
    }

    #[test]
    fn a_failed_write_keeps_the_draft() {
        let (memory, mut editor) = editor(TEXT);
        editor.open(key(2, "Education", None), "## Education\nMSc".to_owned());
        *editor.draft_mut().unwrap() = "## Education\nPhD".to_owned();
        memory.fail_write.set(true);
        assert!(matches!(editor.save(false, false), SaveOutcome::Failed(error) if error.reason == "disk full"));
        assert_eq!(editor.draft_mut().map(|draft| draft.clone()).as_deref(), Some("## Education\nPhD"));
        memory.fail_write.set(false);
        assert_eq!(editor.save(false, false), SaveOutcome::Saved);
        assert!(file(&memory).ends_with("## Education\nPhD\n"));
        assert_eq!(editor.editing(), None);
    }

    #[test]
    fn a_new_block_goes_at_the_end_of_its_section() {
        let (memory, mut editor) = editor(TEXT);
        editor.open_new(key(2, "Professional Experience", None), "### Dev - Acme (2018 – 2021)\nBuilt.".to_owned());
        assert!(!editor.is_dirty());
        assert_eq!(editor.save(false, false), SaveOutcome::Saved);
        assert_eq!(file(&memory), TEXT.replace("Shipped.\n\n", "Shipped.\n\n### Dev - Acme (2018 – 2021)\nBuilt.\n\n"));
    }

    #[test]
    fn use_new_version_takes_the_file_and_the_next_save_compares_with_it() {
        let (memory, mut editor) = editor(TEXT);
        editor.open(key(2, "Education", None), "## Education\nMSc".to_owned());
        *editor.draft_mut().unwrap() = "## Education\nPhD".to_owned();
        memory.files.borrow_mut().insert(ProfileFile::Candidate, TEXT.replace("MSc", "MSc, Porto"));
        assert_eq!(editor.save(false, false), SaveOutcome::Changed { now: "## Education\nMSc, Porto".to_owned() });
        assert_eq!(memory.writes.get(), 0);
        editor.use_new_version();
        assert_eq!(editor.draft_mut().map(|draft| draft.clone()).as_deref(), Some("## Education\nMSc, Porto"));
        assert!(!editor.is_dirty());
        *editor.draft_mut().unwrap() = "## Education\nMSc, Porto, 2011".to_owned();
        assert_eq!(editor.save(false, false), SaveOutcome::Saved);
        assert!(file(&memory).ends_with("## Education\nMSc, Porto, 2011\n"));
    }

    #[test]
    fn cancel_drops_the_draft() {
        let (memory, mut editor) = editor(TEXT);
        editor.open(key(2, "Education", None), "## Education\nMSc".to_owned());
        editor.cancel();
        assert_eq!(editor.editing(), None);
        assert_eq!(editor.save(false, false), SaveOutcome::Saved);
        assert_eq!(file(&memory), TEXT);
    }

    #[test]
    fn the_view_reads_the_store() {
        let (_, editor) = editor(TEXT);
        let view = editor.current_view();
        assert!(view.any_found);
        assert_eq!(view.topics.len(), 11);
    }
}
