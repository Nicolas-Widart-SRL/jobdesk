//! Why the writers of the job search folder wait on the folder update: tasks and replies, Profile's Save, a portal's
//! switch and Remove, Test connection, Restart to update and Change folder. Each says why in its own words while the
//! folder updates, and all say `UNLOCKABLE` while Jobdesk cannot lock it.

/// Why the folder's writers wait on the folder update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderWait {
    /// Jobdesk updates the folder, undoes its update or rolls one back: this Jobdesk, or another that holds it alone.
    Updating,
    /// Jobdesk cannot lock the folder: its lock file, `.jobdesk/.open`, does not open or lock, for a reason other than
    /// another Jobdesk's update, such as a link in its place or its permissions. Nothing updates the folder.
    Unlockable,
}

/// What the notice and every waiting writer say while Jobdesk cannot lock the folder. Plain text, never Markdown.
pub const UNLOCKABLE: &str = "Jobdesk cannot lock your job search folder, so tasks and saves wait. Check .jobdesk/.open in the folder.";
