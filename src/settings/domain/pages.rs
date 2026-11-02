/// The four groups of the rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Personal,
    JobSearch,
    ModelsAndProviders,
    System,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Personal, Group::JobSearch, Group::ModelsAndProviders, Group::System];

    pub fn title(self) -> &'static str {
        match self {
            Group::Personal => "Personal",
            Group::JobSearch => "Job search",
            Group::ModelsAndProviders => "Models and providers",
            Group::System => "System",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Page {
    #[default]
    General,
    Appearance,
    Usage,
    Terms,
    Queue,
    JobsView,
    Providers,
    Defaults,
    Permissions,
    Runtime,
    Data,
    About,
}

impl Page {
    pub const ALL: [Page; 12] = [
        Page::General,
        Page::Appearance,
        Page::Usage,
        Page::Terms,
        Page::Queue,
        Page::JobsView,
        Page::Providers,
        Page::Defaults,
        Page::Permissions,
        Page::Runtime,
        Page::Data,
        Page::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Appearance => "Appearance",
            Page::Usage => "Usage statistics",
            Page::Terms => "Your terms",
            Page::Queue => "Today queue",
            Page::JobsView => "Jobs view",
            Page::Providers => "Providers",
            Page::Defaults => "Run defaults",
            Page::Permissions => "Permissions",
            Page::Runtime => "Runtime status",
            Page::Data => "Data and privacy",
            Page::About => "About",
        }
    }

    pub fn group(self) -> Group {
        match self {
            Page::General | Page::Appearance | Page::Usage => Group::Personal,
            Page::Terms | Page::Queue | Page::JobsView => Group::JobSearch,
            Page::Providers | Page::Defaults | Page::Permissions => Group::ModelsAndProviders,
            Page::Runtime | Page::Data | Page::About => Group::System,
        }
    }

    /// The title in the page's head, which may say more than the rail's name for it.
    pub fn heading(self) -> &'static str {
        match self {
            Page::About => "About Jobdesk",
            page => page.title(),
        }
    }

    /// The line under the title.
    pub fn lead(self) -> &'static str {
        match self {
            Page::General => "Your job search folder, and the screen Jobdesk opens on.",
            Page::Appearance => "How Jobdesk looks on this computer.",
            Page::Usage => "What your tasks cost, counted from Jobdesk's own run logs on this computer.",
            Page::Terms => "The lines a job may not cross. Jobs checks every posting against them.",
            Page::Queue => "How Today deals your cards and how long it holds them back.",
            Page::JobsView => "What the Jobs list leaves out. Nothing here changes a job's score or list.",
            Page::Providers => "How tasks sign in when Jobdesk runs Claude Code on this computer.",
            Page::Defaults => "The model and budget each task starts with.",
            Page::Permissions => "Whether a task asks you before Claude uses a tool.",
            Page::Runtime => "Whether Claude Code, your profile, tools, portals, templates and sync are ready.",
            Page::Data => "What Jobdesk keeps on this computer, and for how long.",
            Page::About => "A desk for your ai-job-search folder.",
        }
    }

    /// Whether the page has its content yet. The others show a placeholder until their step lands.
    pub fn built(self) -> bool {
        matches!(self, Page::General | Page::Appearance | Page::Usage | Page::Defaults | Page::Permissions | Page::Runtime | Page::Data | Page::About)
    }
}

const GENERAL: [&str; 6] = [
    "Job search folder",
    "Folder",
    "The ai-job-search folder Jobdesk reads. Tasks run inside it.",
    "When Jobdesk opens",
    "Open on",
    "The screen you see first.",
];

const APPEARANCE_MAC: [&str; 8] = [
    "Look",
    "Theme",
    "System follows your Mac. This used to sit in the status bar.",
    "Text size",
    "Scales the whole window. Cmd + and Cmd − change it too.",
    "Motion",
    "Reduce motion",
    "Cards and the day stamp change at once, with no slide.",
];

const APPEARANCE_WINDOWS: [&str; 8] = [
    "Look",
    "Theme",
    "System follows Windows. This used to sit in the status bar.",
    "Text size",
    "Scales the whole window. Ctrl + and Ctrl − change it too.",
    "Motion",
    "Reduce motion",
    "Cards and the day stamp change at once, with no slide.",
];

/// The figures' labels, then the sections under them.
const USAGE: [&str; 8] = ["Spent", "Tokens", "Cache hits", "Tasks", "Per application sent", "Every day", "By command", "By model"];

/// Every task's two rows, the table's section, then the cap's section and the words of its row.
const DEFAULTS: [&str; 12] = [
    "Every task",
    "The launcher starts with these. You can still change them before you press Run.",
    "Model",
    "Claude default leaves the choice to Claude Code.",
    "Spend at most, USD",
    "Empty starts each command at its suggested budget, $1 to $5. An amount here starts every task there.",
    "Per command",
    "A command listed here starts with its own model and budget.",
    "Monthly cap",
    "Stop at",
    "a month",
    "Once the open folder's tasks cost that much this month, Run in the launcher is disabled until the 1st and says why. A task that is running finishes.",
];

/// The two choices, the two rows that hold either way, then the line on what allowing costs.
const PERMISSIONS: [&str; 12] = [
    "Tool permissions",
    "What happens when Claude wants to read, write or run something during a task.",
    "Allow and note in the activity",
    "Tasks never stop for a permission. Each tool Claude used is listed in the task's activity, under its step.",
    "Ask me before each tool",
    "The task waits for you each time, with what Claude wants to do. Safer with postings you do not trust, and slower.",
    "Either way",
    "Questions from Claude",
    "A question Claude asks you always reaches you, as Your turn on Today and Tasks.",
    "Remembered rules",
    "A tool you allow for the rest of a task stays allowed only until that task ends. It is never saved to your job search folder.",
    "Allowing every tool means a posting written to trick Claude could get a command run before you see it. The commands in your job search folder check postings with posting_guard before they score them.",
];

/// The data folder's section, the run logs', the tray's, then the line on who can read them.
const DATA: [&str; 11] = [
    "Data folder",
    "Everything Jobdesk keeps apart from your job search folder.",
    "Folder",
    "Size on disk",
    "Run logs",
    "Keep run logs for",
    "Each log holds the whole text of a task, postings and your replies included. Forever keeps them all. A shorter time deletes older ones when Jobdesk starts. Usage statistics count only the logs kept.",
    "Tray decisions",
    "Clear tray decisions",
    "Forgets this folder's decisions and skip reasons on Today, so Today deals those cards again. Other folders keep theirs, and run logs stay.",
    "Run logs and decisions are readable only by your user account. Only tasks send anything out, through Claude Code.",
];

/// The versions, the links, then the line on who makes Jobdesk.
const ABOUT: [&str; 13] = [
    "This copy",
    "Jobdesk",
    "Version and build, for a bug report.",
    "Claude Code",
    "The program each task runs, installed on its own.",
    "Read more",
    "How Jobdesk signs in",
    "Why an API key is the documented route, and what Jobdesk stores.",
    "What changed",
    "Every release, newest first.",
    "Licences",
    "Jobdesk and the open-source parts it is built from.",
    "Jobdesk runs Claude Code. It is not made by Anthropic.",
];

/// The lines Runtime status always draws come first. The folder's sections follow, drawn only once it is open and read.
const RUNTIME: [&str; 12] = [
    "Jobdesk reads your folder and changes it only when you save a Profile edit. Tasks you start change it through Claude Code on this computer.",
    "Claude Code",
    "On this computer",
    "Jobdesk runs this program for every task.",
    "Signed in with",
    "As the last task reported.",
    "Environment",
    "Profile",
    "Portals",
    "Templates",
    "Sync",
    "Reset",
];

/// The labels and help lines the page shows, which is what a search looks through.
/// A page not built yet has none.
pub fn index(page: Page) -> &'static [&'static str] {
    match page {
        Page::General => &GENERAL,
        Page::Appearance if cfg!(target_os = "macos") => &APPEARANCE_MAC,
        Page::Appearance => &APPEARANCE_WINDOWS,
        Page::Usage => &USAGE,
        Page::Defaults => &DEFAULTS,
        Page::Permissions => &PERMISSIONS,
        Page::Runtime => &RUNTIME,
        Page::Data => &DATA,
        Page::About => &ABOUT,
        _ => &[],
    }
}

/// How many of Runtime status's lines show before a folder is open and read.
const RUNTIME_WITHOUT_FOLDER: usize = 7;

/// The lines the page draws as things stand: without `folder_read`, Runtime status leaves out the folder's sections.
pub fn drawn_index(page: Page, folder_read: bool) -> &'static [&'static str] {
    match page {
        Page::Runtime if !folder_read => &RUNTIME[..RUNTIME_WITHOUT_FOLDER],
        page => index(page),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_built_when_it_has_an_index() {
        for page in Page::ALL {
            assert_eq!(page.built(), !index(page).is_empty(), "{}", page.title());
        }
    }

    #[test]
    fn every_group_has_a_page_and_every_page_is_listed_once() {
        for group in Group::ALL {
            assert!(Page::ALL.iter().any(|page| page.group() == group), "{}", group.title());
        }
        for (at, page) in Page::ALL.iter().enumerate() {
            assert!(!Page::ALL[..at].contains(page));
            assert!(!page.lead().is_empty());
        }
    }

    #[test]
    fn without_a_folder_runtime_status_keeps_only_what_it_draws() {
        let drawn = drawn_index(Page::Runtime, false);
        assert!(drawn.contains(&"Environment") && drawn.contains(&"Claude Code"));
        for heading in ["Profile", "Portals", "Templates", "Sync", "Reset"] {
            assert!(!drawn.contains(&heading), "{heading}");
        }
        assert_eq!(drawn_index(Page::Runtime, true), index(Page::Runtime));
        assert_eq!(drawn_index(Page::General, false), index(Page::General));
    }

    #[test]
    fn the_titles_are_the_canvas_ones() {
        assert_eq!(Page::Usage.title(), "Usage statistics");
        assert_eq!(Page::Runtime.title(), "Runtime status");
        assert_eq!(Page::Data.title(), "Data and privacy");
        assert_eq!(Page::Defaults.title(), "Run defaults");
        assert_eq!(Page::Permissions.title(), "Permissions");
        assert_eq!((Page::About.title(), Page::About.heading()), ("About", "About Jobdesk"));
        assert_eq!(Page::Data.heading(), Page::Data.title());
    }
}
