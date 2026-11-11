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

/// The note on where the terms come from, the contract and rate's words with the floor's help for a floor stated and for
/// none, then the commute's words with the row for tiers not stated. The floor's amount and the tiers themselves come from
/// the profile, so a search cannot look through them. `drawn_index` keeps only the lines the page draws.
const TERMS: [&str; 10] = [
    "Jobs checks postings against these. The floor and the commute tiers come from your profile, and Jobdesk never changes them. To change one, edit your profile or ask Claude to.",
    "Contract and rate",
    "Contract",
    "Fixed in Jobdesk: jobs that offer only a permanent contract fail this check.",
    "Day-rate floor",
    "A job that states a lower day rate fails this check.",
    "Your profile does not state one, so every job's day-rate check reads Unclear instead of guessing.",
    "Commute",
    "Commute tiers",
    "Your profile does not state them, so the commute check reads Unclear for a job that is not fully remote, unless the location check ruled it out.",
];

/// How Today deals, then the skip reasons. The reasons themselves are the user's, so a search cannot look through them.
const QUEUE: [&str; 7] = [
    "Dealing",
    "Coming up shows",
    "How far ahead Coming up lists dated cards. Cards dated inside it also deal before the rest.",
    "Later sets a card aside",
    "How long L keeps a card out of the queue. Coming up can bring it back sooner.",
    "Skip reasons",
    "Offered when you skip a card with N. A reason is optional, and past skips keep theirs.",
];

/// The checks a job can fail, then the score and the safety check. The floor's help comes from your terms, and the count
/// from the jobs, so a search cannot look through them.
const JOBS_VIEW: [&str; 11] = [
    "Hide jobs that fail a check",
    "Day rate under your floor",
    "Too far to commute",
    "In your Too far tier and not fully remote, or a place the location check ruled out.",
    "Language does not match",
    "The posting asks for a language your profile does not list.",
    "Score and safety",
    "Hide jobs scored under",
    "Applies to every list on Jobs.",
    "Hide jobs the safety check flagged",
    "Flagged ads still wait in Today as Check this ad, so you decide on each one.",
];

/// What Test connection costs and leaves out, on the Claude sign-in's detail.
const TEST_HELP: &str =
    "Sends one short message through Claude Code, with no tools, capped at $0.01. It costs about a cent, which Usage statistics do not count.";

/// The three ways in, each a title over a line, then the Claude sign-in's detail: its sentence, its rows' labels and
/// Test connection's help. Only what the page draws as it opens, on the Claude sign-in.
const PROVIDERS_MAC: [&str; 11] = [
    "Claude sign-in",
    "Your login in Claude Code",
    "API key",
    "A Console key, in your keychain",
    "Cloud providers",
    "Amazon Bedrock, Google Vertex AI",
    "Tasks use the account you signed in to in Claude Code on this computer. Jobdesk never sees it and stores nothing for it. Usage counts against that account's plan.",
    "Claude Code",
    "Switch account",
    "Check it",
    TEST_HELP,
];

const PROVIDERS_WINDOWS: [&str; 11] = [
    "Claude sign-in",
    "Your login in Claude Code",
    "API key",
    "A Console key, in Credential Manager",
    "Cloud providers",
    "Amazon Bedrock, Google Vertex AI",
    "Tasks use the account you signed in to in Claude Code on this computer. Jobdesk never sees it and stores nothing for it. Usage counts against that account's plan.",
    "Claude Code",
    "Switch account",
    "Check it",
    TEST_HELP,
];

/// Every task's two rows, the table's section, then the cap's section and the words of its row.
const DEFAULTS: [&str; 12] = [
    "Every task",
    "The launcher starts with these. You can still change them before you press Run.",
    "Model",
    "Claude default leaves the choice to Claude Code.",
    "Spend at most, USD",
    "Empty starts each command at its suggested budget, $1 to $25. An amount here starts every task there.",
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

/// The data folder's section, the run logs', the tray's, then the line on who can read them and what leaves this
/// computer.
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
    "Forgets this folder's decisions on Today and the reasons given for past skips, so Today deals those cards again. Other folders keep theirs, and run logs stay.",
    "Run logs and decisions are readable only by your user account. Only tasks send your job search out, through Claude Code. The update check asks GitHub for the latest release, with Jobdesk's version and nothing about your job search. Turn it off in About.",
];

/// Who holds the sign-in, on About. Windows keeps the key in Credential Manager, as Providers says.
const SIGN_IN_HELP: &str = if cfg!(windows) {
    "Claude Code signs in on its own. Jobdesk keeps only an optional API key, in Credential Manager."
} else {
    "Claude Code signs in on its own. Jobdesk keeps only an optional API key, in your keychain."
};

/// The versions, the updates, the links, then the line on who makes Jobdesk. The switch has a help for on and one for
/// off, and `drawn_index` keeps the one it shows. Latest version's help says the state of the check, so a search cannot
/// look through it.
const ABOUT: [&str; 18] = [
    "This copy",
    "Jobdesk",
    "Version and build, for a bug report.",
    "Claude Code",
    "The program each task runs, installed on its own.",
    "Updates",
    "Check for updates automatically",
    AUTO_CHECK_ON,
    AUTO_CHECK_OFF,
    "Latest version",
    "Read more",
    "How tasks sign in",
    SIGN_IN_HELP,
    "What changed",
    "Every release, newest first.",
    "Licences",
    "Jobdesk and the open-source parts it is built from.",
    "Jobdesk runs Claude Code. It is not made by Anthropic.",
];

/// The help under Check for updates automatically while it is on, then off, as the updates feature words them.
const AUTO_CHECK_ON: &str =
    "When Jobdesk opens and about every 6 hours, it reads the latest release on GitHub. It downloads nothing until you choose Download.";
const AUTO_CHECK_OFF: &str = "Off. Jobdesk looks for a new version only when you choose Check now.";

/// The lines Runtime status always draws come first. The folder's sections follow, drawn only once it is open and read.
const RUNTIME: [&str; 12] = [
    "Jobdesk reads your folder and changes it only when you save a Profile edit, switch a portal on or off, or remove one, and when a Jobdesk update brings newer versions of its files, which you can undo right after. Tasks you start change it through Claude Code on this computer.",
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

/// The fixed labels and help lines the page shows, which is what a search looks through.
pub fn index(page: Page) -> &'static [&'static str] {
    match page {
        Page::General => &GENERAL,
        Page::Appearance if cfg!(target_os = "macos") => &APPEARANCE_MAC,
        Page::Appearance => &APPEARANCE_WINDOWS,
        Page::Usage => &USAGE,
        Page::Terms => &TERMS,
        Page::Queue => &QUEUE,
        Page::JobsView => &JOBS_VIEW,
        Page::Providers if cfg!(windows) => &PROVIDERS_WINDOWS,
        Page::Providers => &PROVIDERS_MAC,
        Page::Defaults => &DEFAULTS,
        Page::Permissions => &PERMISSIONS,
        Page::Runtime => &RUNTIME,
        Page::Data => &DATA,
        Page::About => &ABOUT,
    }
}

/// How many of Runtime status's lines show before a folder is open and read.
const RUNTIME_WITHOUT_FOLDER: usize = 7;

/// How Your terms draws one part, the floor or the commute tiers, as its file read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartDrawn {
    /// No folder, the folder not read yet, or the part's file not read.
    Hidden,
    Stated,
    /// The file read and does not state the part, so its row reads Not stated with a help of its own.
    NotStated,
}

/// Which parts of Your terms the page draws. A hidden part takes the note with it, as the alert stands in its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermsDrawn {
    pub rate: PartDrawn,
    pub commute: PartDrawn,
}

impl TermsDrawn {
    pub const NOTHING: Self = Self { rate: PartDrawn::Hidden, commute: PartDrawn::Hidden };
    pub const STATED: Self = Self { rate: PartDrawn::Stated, commute: PartDrawn::Stated };
}

/// What the pages draw as things stand, which is all a search may match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drawn {
    /// Whether the folder is open and read.
    pub folder_read: bool,
    pub terms: TermsDrawn,
    /// Whether Check for updates automatically is on, which picks the help About shows under it.
    pub auto_check: bool,
}

impl Drawn {
    /// A folder read whole with every term stated, or none: Your terms draws every part, or nothing. The automatic check
    /// is on, as it is by default.
    pub fn folder(read: bool) -> Self {
        Self { folder_read: read, terms: if read { TermsDrawn::STATED } else { TermsDrawn::NOTHING }, auto_check: true }
    }
}

/// The lines the page draws as things stand: without a folder read, Runtime status leaves out the folder's sections, and
/// Your terms shows only the parts that read, each with the help for a part stated or not, and About shows the help of
/// the automatic check as it is switched.
pub fn drawn_index(page: Page, drawn: Drawn) -> Vec<&'static str> {
    match page {
        Page::Runtime if !drawn.folder_read => RUNTIME[..RUNTIME_WITHOUT_FOLDER].to_vec(),
        Page::Terms => terms_index(drawn.terms),
        Page::About => {
            let hidden = if drawn.auto_check { AUTO_CHECK_OFF } else { AUTO_CHECK_ON };
            ABOUT.into_iter().filter(|&line| line != hidden).collect()
        }
        page => index(page).to_vec(),
    }
}

fn terms_index(drawn: TermsDrawn) -> Vec<&'static str> {
    let [note, section, contract, contract_help, floor, floor_help, no_floor, commute, tiers, no_tiers] = TERMS;
    let mut lines = Vec::new();
    // The note goes with the alert, so it shows only while both parts do.
    if drawn.rate != PartDrawn::Hidden && drawn.commute != PartDrawn::Hidden {
        lines.push(note);
    }
    // The contract shows only beside the floor.
    match drawn.rate {
        PartDrawn::Hidden => {}
        PartDrawn::Stated => lines.extend([section, contract, contract_help, floor, floor_help]),
        PartDrawn::NotStated => lines.extend([section, contract, contract_help, floor, no_floor]),
    }
    match drawn.commute {
        PartDrawn::Hidden => {}
        PartDrawn::Stated => lines.push(commute),
        PartDrawn::NotStated => lines.extend([commute, tiers, no_tiers]),
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_has_an_index() {
        for page in Page::ALL {
            assert!(!index(page).is_empty(), "{}", page.title());
        }
    }

    #[test]
    fn without_a_folder_your_terms_draws_nothing_to_search() {
        assert!(drawn_index(Page::Terms, Drawn::folder(false)).is_empty());
        assert_eq!(drawn_index(Page::Terms, Drawn::folder(true)), [&TERMS[..6], &["Commute"]].concat(), "every term stated");
        assert_eq!(drawn_index(Page::Queue, Drawn::folder(false)), index(Page::Queue), "the queue's settings need no folder");
        assert_eq!(drawn_index(Page::JobsView, Drawn::folder(false)), index(Page::JobsView));
    }

    fn terms(rate: PartDrawn, commute: PartDrawn) -> Vec<&'static str> {
        drawn_index(Page::Terms, Drawn { folder_read: true, terms: TermsDrawn { rate, commute }, auto_check: true })
    }

    #[test]
    fn a_failed_file_leaves_only_the_part_still_read_to_search() {
        let floor_help = "A job that states a lower day rate fails this check.";
        assert_eq!(terms(PartDrawn::Stated, PartDrawn::Hidden), ["Contract and rate", "Contract", TERMS[3], "Day-rate floor", floor_help]);
        assert_eq!(terms(PartDrawn::Hidden, PartDrawn::Stated), ["Commute"]);
        assert!(terms(PartDrawn::Hidden, PartDrawn::Hidden).is_empty(), "both failed");
    }

    #[test]
    fn a_part_not_stated_draws_its_own_help_in_place_of_the_stated_one() {
        let rate = terms(PartDrawn::NotStated, PartDrawn::Hidden);
        assert!(rate.contains(&TERMS[6]) && !rate.contains(&TERMS[5]));
        assert_eq!(terms(PartDrawn::Hidden, PartDrawn::NotStated), ["Commute", "Commute tiers", TERMS[9]]);
        assert_eq!(terms(PartDrawn::NotStated, PartDrawn::NotStated).len(), 1 + 5 + 3, "the note too");
    }

    #[test]
    fn every_terms_line_is_drawn_in_some_state() {
        let parts = [PartDrawn::Stated, PartDrawn::NotStated];
        for line in TERMS {
            assert!(parts.iter().any(|&rate| parts.iter().any(|&commute| terms(rate, commute).contains(&line))), "{line}");
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
        let drawn = drawn_index(Page::Runtime, Drawn::folder(false));
        assert!(drawn.contains(&"Environment") && drawn.contains(&"Claude Code"));
        for heading in ["Profile", "Portals", "Templates", "Sync", "Reset"] {
            assert!(!drawn.contains(&heading), "{heading}");
        }
        assert_eq!(drawn_index(Page::Runtime, Drawn::folder(true)), index(Page::Runtime));
        assert_eq!(drawn_index(Page::General, Drawn::folder(false)), index(Page::General));
    }

    #[test]
    fn about_finds_the_updates_in_the_words_the_page_draws() {
        let about = index(Page::About);
        for line in ["Updates", "Check for updates automatically", "Latest version"] {
            assert!(about.contains(&line), "{line}");
        }
        for auto in [true, false] {
            let help = crate::updates::idle_updates("0.1.0", auto).view().about.auto_help;
            let drawn = drawn_index(Page::About, Drawn { auto_check: auto, ..Drawn::folder(true) });
            assert!(drawn.contains(&help), "the switch's help, as the updates feature words it");
            let other = crate::updates::idle_updates("0.1.0", !auto).view().about.auto_help;
            assert!(!drawn.contains(&other), "and only the one it shows");
            assert!(about.contains(&help));
        }
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
