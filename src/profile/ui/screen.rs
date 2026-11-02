use chrono::NaiveDateTime;
use egui::{Align, Frame, Key, Label, Layout, Margin, Modifiers, Rect, Ui, UiBuilder, pos2};
use egui_commonmark::CommonMarkCache;

use std::time::Duration;

use crate::profile::application::{
    BlockKey, Fill, Mark, ProfileEditor, ProfileFile, ProfileView, SectionView, TOPICS, TopicId, TopicView, UpdatedMarks, topic_counts,
};
use crate::profile::ui::editor::{Answer, Ask, Edit, Noun, ONE_AT_A_TIME, ROLE_STUB, discard_ask, save, slip, take_theirs};
use crate::profile::ui::rail::{UPDATED_HOVER, loading_rail, rail};
use crate::shared_kernel::visible_text::visible_chars;
use crate::ui_kit::tokens::{BUTTON_GAP, DEAL_PADDING, GUTTER, ITEM_GAP, ROLE_DATES, ROLE_PADDING, RUN_SIDE, SCREEN_GAP, SKELETON_GAP, SKELETON_LINE};
use crate::ui_kit::{
    EditorAction, Ink, RoleEntry, ScreenState, Subtitle, Title, Tone, Type, button, desk_label, divider, empty_note, empty_state, empty_state_with_command,
    enabled_when, faded, faint_mono, headed_paper, page_body, page_head, paper, primary_button, primary_command_action, profile_document, rich, role_entry,
    skeleton, skeleton_lines, small_button, small_command_action, tag,
};

/// What the head says under the title once the profile is read.
const SUBTITLE: &str = "Claude reads this before it scores a job or writes a CV or a letter.";
const CLAUDE: &str = "Update with Claude…";
/// Why Update with Claude… waits while a run works.
const BUSY: &str = "Another run is active. One run at a time keeps your files safe.";
/// Why Update with Claude… waits while a file of the topic cannot be read.
const UNREADABLE: &str = "Fix the file first, so Claude does not write over it.";
/// What the paper says of a topic a file hid.
const HIDDEN: &str = "Jobdesk could not read a file that holds this topic, so it is not shown. Nothing in the file was changed. Jobdesk reads it again as soon as the file changes.";
/// Why Write it myself waits when the file lacks the topic's section.
const NO_SECTION: &str = "This topic's section is not in the file. Update with Claude… adds it.";
/// The same for a topic `/setup` has no section for.
const NO_SECTION_BY_HAND: &str = "This topic's section is not in the file, so there is nothing to edit yet.";

/// What stays between frames: the topic on paper, whether its empty state shows its command, the rendered Markdown, the
/// one edit on show, and the ask that waits on it.
#[derive(Default)]
pub struct ProfileScreen {
    picked: Option<TopicId>,
    show_command: bool,
    markdown: CommonMarkCache,
    edit: Option<Edit>,
    /// What waits on the discard ask, and whether Keep editing still has to take the focus.
    ask: Option<(Ask, bool)>,
}

impl ProfileScreen {
    /// The user picked another tab. With unsaved text it asks first and says so, and the tab waits for `show` to return
    /// `Leave`. Without, the edit closes and the tab changes at once.
    pub fn ask_to_leave(&mut self, editor: &mut ProfileEditor) -> bool {
        if editor.is_dirty() {
            self.ask = Some((Ask::Tab, true));
            return true;
        }
        editor.cancel();
        self.edit = None;
        false
    }

    /// Profile is no longer on show, so no ask waits there. The draft stays for when it comes back.
    pub fn put_away(&mut self) {
        self.ask = None;
    }
}

/// What the screen asks of the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileRequest {
    /// A block was written, so the profile on show is out of date.
    Saved,
    /// The user dropped their draft to leave for the tab they picked.
    Leave,
}

/// The moment the screen draws in: whether a run works, so Save and Update with Claude… wait, and the time, which ages
/// the head line and the foot's Changed on disk.
#[derive(Debug, Clone, Copy)]
pub struct Moment {
    pub busy: bool,
    pub now: NaiveDateTime,
}

/// The rail of topics beside the one picked, on paper. Until a topic is picked, the first with something left to fill opens.
/// A picked topic is open, so its rail mark clears, and picking another leaves it, so its head line goes.
/// One block at a time opens in the editor. Unsaved text asks before it goes, and Save waits while a run works.
/// Every Claude action only opens the launcher: nothing here starts a run.
pub fn show(
    ui: &mut Ui,
    screen: &mut ProfileScreen,
    state: &ScreenState,
    view: &ProfileView,
    editor: &mut ProfileEditor,
    marks: &mut UpdatedMarks,
    moment: Moment,
) -> Option<ProfileRequest> {
    if editor.editing().is_none() {
        screen.edit = None;
    }
    let mut request = None;
    // The ask goes first, so its Esc and the command key and Enter never reach the editor behind it.
    if let Some((ask, focus)) = screen.ask.as_mut() {
        let ask = *ask;
        ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter));
        let noun = screen.edit.as_ref().map_or(Noun::Part, Edit::noun);
        match discard_ask(ui.ctx(), ask, noun, focus) {
            Some(Answer::Keep) => screen.ask = None,
            Some(Answer::Discard) => {
                screen.ask = None;
                editor.cancel();
                screen.edit = None;
                match ask {
                    Ask::Cancel => {}
                    Ask::Topic(next) => screen.switch(next, marks),
                    Ask::Tab => request = Some(ProfileRequest::Leave),
                }
            }
            None => {}
        }
    }
    let reading = state.reading();
    let subtitle = if reading { "Reading your profile…" } else { SUBTITLE };
    page_head(ui, Title::Slab("Profile"), Some(Subtitle::Text(subtitle)), |_| {});
    page_body(ui, |ui| {
        (state.alerts)(ui);
        if reading {
            columns(ui, |ui| loading_rail(ui, TOPICS.len()), loading_paper);
            return;
        }
        let counts = topic_counts(view);
        if !counts.any_found && counts.hidden == 0 {
            no_profile(ui);
            return;
        }
        let Some(picked) = screen.picked.or_else(|| first_to_fill(view)) else { return };
        screen.picked = Some(picked);
        marks.open(picked);
        let Some(topic) = view.topics.iter().find(|topic| topic.id == picked) else { return };
        let mut chosen = None;
        let mut intent = None;
        columns(
            ui,
            |ui| chosen = rail(ui, view, counts, picked, marks),
            |ui| {
                ui.push_id(("profile topic", picked), |ui| {
                    let open = Open { updated: marks.head_line(picked), busy: moment.busy, now: moment.now };
                    let edit = screen.edit.as_mut().filter(|edit| edit.topic == picked);
                    let mut paper = Paper { cache: &mut screen.markdown, editor: &mut *editor, edit, busy: moment.busy, intent: None };
                    topic_paper(ui, &mut paper, &mut screen.show_command, topic, &open);
                    intent = paper.intent;
                });
            },
        );
        if let Some(intent) = intent {
            request = request.or(screen.follow(intent, picked, editor, moment));
        }
        if let Some(next) = chosen.filter(|&next| next != picked) {
            if editor.is_dirty() {
                screen.ask = Some((Ask::Topic(next), true));
            } else {
                editor.cancel();
                screen.edit = None;
                screen.switch(next, marks);
            }
        }
    });
    request
}

/// What the user asked of the paper this frame.
enum Intent {
    /// Edit a block, as `noun`, from `text` as the file holds it.
    Open {
        key: BlockKey,
        text: String,
        noun: Noun,
    },
    /// Add a role at the end of the section `after` names.
    AddRole {
        after: BlockKey,
    },
    Editor(EditorAction),
}

impl ProfileScreen {
    /// Leaves the topic on paper for `next`, which clears its rail mark and drops the last one's head line.
    fn switch(&mut self, next: TopicId, marks: &mut UpdatedMarks) {
        if let Some(picked) = self.picked {
            marks.leave(picked);
        }
        marks.open(next);
        self.picked = Some(next);
        self.show_command = false;
    }

    /// Does what the paper asked. Only one edit opens at a time, and Cancel with unsaved text asks first.
    fn follow(&mut self, intent: Intent, topic: TopicId, editor: &mut ProfileEditor, moment: Moment) -> Option<ProfileRequest> {
        match intent {
            Intent::Open { .. } | Intent::AddRole { .. } if editor.editing().is_some() => {}
            Intent::Open { key, text, noun } => {
                editor.open(key, text);
                self.edit = Some(Edit::new(topic, noun));
            }
            Intent::AddRole { after } => {
                editor.open_new(after, ROLE_STUB.to_owned());
                self.edit = Some(Edit::new(topic, Noun::Role));
            }
            Intent::Editor(action) => {
                let edit = self.edit.as_mut()?;
                match action {
                    EditorAction::Save => {
                        let written = save(editor, edit, moment.busy, moment.now);
                        if editor.editing().is_none() {
                            self.edit = None;
                        }
                        if written {
                            return Some(ProfileRequest::Saved);
                        }
                    }
                    EditorAction::Cancel if editor.is_dirty() => self.ask = Some((Ask::Cancel, true)),
                    EditorAction::Cancel => {
                        editor.cancel();
                        self.edit = None;
                    }
                    EditorAction::TakeTheirs => take_theirs(editor, edit),
                }
            }
        }
        None
    }
}

/// What the paper draws with: the rendered Markdown, the editor and the edit on show in this topic, if any, and what the
/// user asked of it this frame.
struct Paper<'a> {
    cache: &'a mut CommonMarkCache,
    editor: &'a mut ProfileEditor,
    edit: Option<&'a mut Edit>,
    busy: bool,
    intent: Option<Intent>,
}

impl Paper<'_> {
    /// Why a block's Edit waits, if it does: another edit is open.
    fn blocked(&self) -> Option<&'static str> {
        self.editor.editing().is_some().then_some(ONE_AT_A_TIME)
    }

    /// Whether the block `key` names is open in the editor, in place.
    fn editing(&self, key: &BlockKey) -> bool {
        self.edit.is_some() && !self.editor.is_new() && self.editor.editing() == Some(key)
    }

    /// Whether a new block is open that goes at the end of the block `key` names.
    fn adding(&self, key: &BlockKey) -> bool {
        self.edit.is_some() && self.editor.is_new() && self.editor.editing() == Some(key)
    }

    /// Whether the open edit has a place among the topic's blocks. One whose block left the file still shows, at the top.
    fn placed(&self, topic: &TopicView) -> bool {
        let Some(key) = self.editor.editing() else { return true };
        let roles = |section: &SectionView| section.block.key.file == ProfileFile::Candidate && topic.id == TopicId::Experience;
        topic.sections.iter().any(|section| {
            let own = &section.block.key == key && (!self.editor.is_new() || roles(section));
            own || (!self.editor.is_new() && section.children.iter().any(|child| &child.key == key))
        })
    }

    fn ask(&mut self, intent: Intent) {
        self.intent = Some(intent);
    }

    /// The editor's slip, in place of the block.
    fn slip(&mut self, ui: &mut Ui) {
        let Some(edit) = self.edit.as_deref_mut() else { return };
        if let Some(action) = slip(ui, self.cache, self.editor, edit, self.busy) {
            self.intent = Some(Intent::Editor(action));
        }
    }
}

/// The topic a fresh screen opens on: the first with something left to fill, or else the first.
fn first_to_fill(view: &ProfileView) -> Option<TopicId> {
    let open = view.topics.iter().find(|topic| matches!(topic.fill, Fill::ToFill(_) | Fill::NotStarted));
    open.or(view.topics.first()).map(|topic| topic.id)
}

/// The rail at the left in `RUN_SIDE`, and the paper beside it, both as tall as the room left.
fn columns(ui: &mut Ui, side: impl FnOnce(&mut Ui), main: impl FnOnce(&mut Ui)) {
    let area = ui.available_rect_before_wrap();
    let rail_rect = Rect::from_min_max(area.min, pos2((area.left() + RUN_SIDE).min(area.right()), area.bottom()));
    let page_rect = Rect::from_min_max(pos2((rail_rect.right() + f32::from(GUTTER)).min(area.right()), area.top()), area.max);
    let column = |rect: Rect, salt: &str| UiBuilder::new().id_salt(salt).max_rect(rect).layout(Layout::top_down(Align::Min));
    let mut rail_ui = ui.new_child(column(rail_rect, "profile_rail_column"));
    rail_ui.set_width(rail_rect.width());
    side(&mut rail_ui);
    let mut page_ui = ui.new_child(column(page_rect, "profile_page_column"));
    page_ui.set_width(page_rect.width());
    main(&mut page_ui);
    ui.advance_cursor_after_rect(area);
}

/// Why Update with Claude… waits, if it does.
fn claude_wait(topic: &TopicView, busy: bool) -> Option<&'static str> {
    if busy {
        Some(BUSY)
    } else if !topic.readable {
        Some(UNREADABLE)
    } else {
        None
    }
}

/// Update with Claude… opens the launcher on the topic's `/setup` line. A topic `/setup` has no section for has none.
fn update_with_claude(ui: &mut Ui, topic: &TopicView, busy: bool, primary: bool) {
    let Some(line) = topic.setup_line.as_deref() else { return };
    let wait = claude_wait(topic, busy);
    enabled_when(ui, wait.is_none(), wait.unwrap_or_default(), |ui| {
        if primary { primary_command_action(ui, CLAUDE, line) } else { small_command_action(ui, CLAUDE, line) }
    });
}

/// What the paper needs beside the topic: its Updated mark, whether a run works, and the time that ages the mark.
struct Open<'a> {
    updated: Option<&'a Mark>,
    busy: bool,
    now: NaiveDateTime,
}

/// The topic on paper. While a block of it is open in the editor, the topic shows its blocks with the editor in place,
/// whatever its fill, so the draft never leaves the screen.
fn topic_paper(ui: &mut Ui, paper: &mut Paper, show_command: &mut bool, topic: &TopicView, open: &Open) {
    let blocked = paper.blocked();
    let changed_at = paper.edit.as_ref().and_then(|edit| edit.changed_at());
    let mut head_intent = None;
    headed_paper(
        ui,
        |ui| head_intent = head(ui, topic, open, blocked),
        |ui| match topic.fill {
            _ if paper.edit.is_some() => {
                if topic.fill == Fill::Hidden {
                    hidden(ui, topic);
                }
                sections(ui, paper, topic, true);
            }
            Fill::Hidden => hidden(ui, topic),
            Fill::NotStarted => not_started(ui, paper, show_command, topic, open.busy),
            Fill::Filled | Fill::ToFill(_) => sections(ui, paper, topic, true),
        },
        |ui| foot(ui, topic, changed_at.map(|at| age(at, open.now))),
    );
    if let Some(intent) = head_intent {
        paper.ask(intent);
    }
    if let Some(at) = changed_at {
        ui.ctx().request_repaint_after(next_age_change(at, open.now));
    }
}

/// The title, what the topic is for, and the Updated line, with the actions at the right. A topic of one section edits
/// it from here. Returns what was asked.
fn head(ui: &mut Ui, topic: &TopicView, open: &Open, blocked: Option<&str>) -> Option<Intent> {
    let (updated, busy, now) = (open.updated, open.busy, open.now);
    let mut intent = None;
    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        update_with_claude(ui, topic, busy, false);
        if let [section] = topic.sections.as_slice()
            && topic.readable
            && topic.fill != Fill::NotStarted
            && enabled_when(ui, blocked.is_none(), blocked.unwrap_or_default(), |ui| small_button(ui, "Edit section")).clicked()
        {
            intent = Some(open_section(section));
        }
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
            ui.label(rich(ui, topic.title, Type::DEAL, Ink::Text));
            ui.add(Label::new(rich(ui, lead(topic), Type::BODY, Ink::Muted)).wrap());
            if let Some(mark) = updated {
                // The age words change on their own, so the window wakes when they would.
                ui.ctx().request_repaint_after(next_age_change(mark.at, now));
                ui.add_space(ITEM_GAP);
                ui.horizontal(|ui| {
                    tag(ui, "Updated", Tone::Active).on_hover_text(UPDATED_HOVER);
                    ui.add(Label::new(rich(ui, updated_words(mark, now), Type::NOTE, Ink::Muted)).wrap());
                });
            }
        });
    });
    intent
}

/// Edit section: the whole section, its blocks included, in the editor.
fn open_section(section: &SectionView) -> Intent {
    Intent::Open { key: section.block.key.clone(), text: section.block.text.clone(), noun: Noun::Section }
}

/// One line on what the topic is for, in the voice of the rail.
fn lead(topic: &TopicView) -> String {
    match topic.id {
        TopicId::Identity => "Who you are and the languages you speak. Every CV and letter opens with it.".to_owned(),
        TopicId::Experience => {
            let roles = topic.sections.iter().flat_map(|section| &section.children).filter(|child| child.role.is_some()).count();
            // The roles stay in the order the file has them, which need not be by date.
            match roles {
                _ if !topic.readable => "Your roles. Claude picks from these for each CV.".to_owned(),
                0 => "No roles here yet. Claude picks from your roles for each CV.".to_owned(),
                1 => "1 role. Claude picks from your roles for each CV.".to_owned(),
                roles => format!("{roles} roles. Claude picks from these for each CV."),
            }
        }
        TopicId::Education => "Your degrees and courses. Claude lists them on each CV.".to_owned(),
        TopicId::Skills => "What you know and how well. Claude matches it against what each job asks for.".to_owned(),
        TopicId::Publications => "Side projects, papers and awards. Claude adds the ones a job cares about.".to_owned(),
        TopicId::References => "People who can vouch for you. Claude names them when an application asks.".to_owned(),
        TopicId::Statements => "The short pitch at the top of a CV. Claude tailors one for each job.".to_owned(),
        TopicId::HowIWork => "How you like to work. Claude weighs it when it scores a team and its pace.".to_owned(),
        TopicId::Goals => "What you want, and what rules a job out. Claude scores every job against it.".to_owned(),
        TopicId::Stories => "Your STAR stories. Claude picks from these when it prepares you for an interview.".to_owned(),
        TopicId::JobSearch => "What to search for, where and in which languages. Every job search runs on it.".to_owned(),
    }
}

/// "by the last task, 4 minutes ago. It changed Lead Engineer - Calder (2021 – now)." Headings show as plain text.
fn updated_words(mark: &Mark, now: NaiveDateTime) -> String {
    let blocks: Vec<String> = mark.blocks.iter().map(|block| visible_chars(block).collect()).collect();
    format!("by the last task, {}. It changed {}.", age(mark.at, now), and_list(&blocks))
}

/// How long ago `at` was, in whole minutes, or hours past the first one.
fn age(at: NaiveDateTime, now: NaiveDateTime) -> String {
    let minutes = (now - at).num_minutes();
    match minutes {
        ..1 => "just now".to_owned(),
        1 => "1 minute ago".to_owned(),
        2..60 => format!("{minutes} minutes ago"),
        60..120 => "1 hour ago".to_owned(),
        _ => format!("{} hours ago", minutes / 60),
    }
}

/// How long until `age` words the time since `at` differently: the next whole minute in the first hour, then the next hour.
fn next_age_change(at: NaiveDateTime, now: NaiveDateTime) -> Duration {
    let seconds = (now - at).num_seconds().max(0).unsigned_abs();
    let step = if seconds < 3600 { 60 } else { 3600 };
    Duration::from_secs(step - seconds % step)
}

/// "A", "A and B", "A, B and C".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The topic's blocks, as written. Each section shows under its label when the topic has more than one.
/// A role reads as a CV line, and the rest goes through `profile_document`, as ARCHITECTURE.md#trust says.
/// With `edits`, every `###` has its Edit and every labelled section its Edit section, and the open edit shows in place.
fn sections(ui: &mut Ui, paper: &mut Paper, topic: &TopicView, edits: bool) {
    if edits && !paper.placed(topic) {
        ui.add_space(SCREEN_GAP / 2.0);
        paper.slip(ui);
    }
    let headed = topic.sections.len() > 1;
    for (index, section) in topic.sections.iter().enumerate() {
        ui.push_id(("profile section", index), |ui| {
            if index > 0 {
                ui.add_space(SCREEN_GAP / 2.0);
                divider(ui);
            }
            ui.add_space(SCREEN_GAP / 2.0);
            let label = headed.then(|| section_label(topic, section));
            let roles = topic.id == TopicId::Experience && section.block.key.file == ProfileFile::Candidate;
            section_blocks(ui, paper, section, label, edits.then_some(roles));
        });
    }
}

/// The `CLAUDE.md` headings that sum up a topic's own section, so they read as its summary.
const SUMMARIES: [&str; 4] = ["Identity", "Professional Experience", "Education", "Behavioral Profile"];

/// What a section's sub-heading says. A part from `CLAUDE.md` is named by where it lives: "Summary in CLAUDE.md" where
/// it sums up the topic's own section, and its own heading in `CLAUDE.md` otherwise. When `CLAUDE.md` adds to a topic
/// held in one other section, that section takes a short label, such as Roles, but only one its blocks bear out.
/// Every other section keeps its own heading.
fn section_label(topic: &TopicView, section: &SectionView) -> String {
    let in_claude = |section: &SectionView| section.block.key.file == ProfileFile::ClaudeMd;
    let own = || visible_chars(&section.block.heading).collect::<String>();
    if in_claude(section) {
        return if SUMMARIES.contains(&section.block.heading.as_str()) { "Summary in CLAUDE.md".to_owned() } else { format!("{} in CLAUDE.md", own()) };
    }
    let others = topic.sections.iter().filter(|section| !in_claude(section)).count();
    match short_label(topic.id, section) {
        Some(label) if others == 1 && topic.sections.len() > others => label.to_owned(),
        _ => own(),
    }
}

/// The label of a topic's one section outside `CLAUDE.md`, beside the part there.
fn short_label(topic: TopicId, section: &SectionView) -> Option<&'static str> {
    match topic {
        TopicId::Identity => Some("Details and languages"),
        TopicId::Experience if section.children.iter().any(|child| child.role.is_some()) => Some("Roles"),
        TopicId::Education => Some("Degrees"),
        _ => None,
    }
}

/// One section's blocks. `edits` is `None` for a section shown only for what it holds, and otherwise says whether the
/// section holds the roles, so Add a role goes after its last one.
fn section_blocks(ui: &mut Ui, paper: &mut Paper, section: &SectionView, label: Option<String>, edits: Option<bool>) {
    ui.spacing_mut().item_spacing.y = ITEM_GAP;
    let blocked = paper.blocked();
    match label {
        Some(label) if edits.is_some() => {
            if headed_row(ui, &label, Type::SLIP_TITLE, "Edit section", blocked) {
                paper.ask(open_section(section));
            }
        }
        Some(label) => heading(ui, &label, Type::SLIP_TITLE),
        None => {}
    }
    if edits.is_some() && paper.editing(&section.block.key) {
        paper.slip(ui);
        return;
    }
    let own = !section.block.body.is_empty();
    if own {
        profile_document(ui, paper.cache, &section.block.body);
    }
    // A heading needs no rule under it, so only words of the section's own put one over its first role.
    let mut first = !own;
    for child in &section.children {
        if edits.is_some() && paper.editing(&child.key) {
            if !first {
                divider(ui);
            }
            ui.add_space(ROLE_PADDING.0);
            paper.slip(ui);
            ui.add_space(ROLE_PADDING.1);
            first = false;
            continue;
        }
        let noun = if child.role.is_some() { Noun::Role } else { Noun::Part };
        let edit = match &child.role {
            Some(role) => {
                let entry = RoleEntry { dates: &role.dates, title: &role.title, company: &role.company, body: &child.body, first };
                // A role shown only for what it holds has nothing to edit yet: Write it myself opens the section.
                role_entry(ui, paper.cache, &entry, if edits.is_some() { blocked } else { Some(OUTLINE) }).clicked()
            }
            None => {
                ui.add_space(ITEM_GAP);
                let edit = match edits {
                    Some(_) => headed_row(ui, &child.heading, Type::SECTION, "Edit", blocked),
                    None => {
                        heading(ui, &child.heading, Type::SECTION);
                        false
                    }
                };
                profile_document(ui, paper.cache, &child.body);
                edit
            }
        };
        if edit {
            paper.ask(Intent::Open { key: child.key.clone(), text: child.text.clone(), noun });
        }
        first = false;
    }
    if edits == Some(true) && section.block.key.level == 2 {
        add_role(ui, paper, section, first);
    }
}

/// Why a role in an empty topic's outline has no Edit.
const OUTLINE: &str = "Write it myself opens this topic in the editor.";

/// After the last role: the new role's editor while it is open, and else Add a role, under the dates column.
fn add_role(ui: &mut Ui, paper: &mut Paper, section: &SectionView, first: bool) {
    if !first {
        divider(ui);
    }
    ui.add_space(ROLE_PADDING.0);
    if paper.adding(&section.block.key) {
        paper.slip(ui);
    } else {
        let blocked = paper.blocked();
        let added = ui
            .horizontal(|ui| {
                ui.add_space(ROLE_DATES);
                enabled_when(ui, blocked.is_none(), blocked.unwrap_or_default(), |ui| small_button(ui, "Add a role")).clicked()
            })
            .inner;
        if added {
            paper.ask(Intent::AddRole { after: section.block.key.clone() });
        }
    }
    ui.add_space(ROLE_PADDING.1);
}

/// A heading with its edit button at the paper's right edge, outside the heading's text. Returns whether it was clicked.
fn headed_row(ui: &mut Ui, text: &str, style: Type, action: &str, blocked: Option<&str>) -> bool {
    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
        let clicked = enabled_when(ui, blocked.is_none(), blocked.unwrap_or_default(), |ui| small_button(ui, action)).clicked();
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| heading(ui, text, style));
        clicked
    })
    .inner
}

/// A block's heading, as plain text with hidden characters removed.
fn heading(ui: &mut Ui, text: &str, style: Type) {
    desk_label(ui, visible_chars(text).collect::<String>(), style, Ink::Text);
}

/// A topic whose blocks hold nothing yet: what Claude will ask, the ways to fill it and the command it opens, then its
/// outline, faded.
/// Write it myself opens the topic's first section. Without a `--section` it is the only action, so it leads.
fn not_started(ui: &mut Ui, paper: &mut Paper, show_command: &mut bool, topic: &TopicView, busy: bool) {
    ui.add_space(SCREEN_GAP / 2.0);
    let body = match asks(topic.id) {
        Some(asks) if topic.setup_line.is_some() => format!("Claude asks {asks}, then writes it here. You can change any of it afterwards."),
        _ => "Write it in your own words. Claude reads it as you leave it.".to_owned(),
    };
    let first = topic.sections.first().filter(|_| !topic.missing_heading);
    let wait = match first {
        None if topic.setup_line.is_some() => Some(NO_SECTION),
        None => Some(NO_SECTION_BY_HAND),
        Some(_) => paper.blocked(),
    };
    let lead = topic.setup_line.is_none();
    let mut write = false;
    let actions = |ui: &mut Ui| {
        update_with_claude(ui, topic, busy, true);
        let shown = enabled_when(ui, wait.is_none(), wait.unwrap_or_default(), |ui| {
            if lead { primary_button(ui, "Write it myself") } else { button(ui, "Write it myself") }
        });
        write = shown.clicked();
    };
    match topic.setup_line.as_deref() {
        Some(line) => empty_state_with_command(ui, "Not filled in yet", &body, actions, line, show_command),
        None => empty_state(ui, "Not filled in yet", &body, actions),
    }
    if let (true, Some(section)) = (write, first) {
        paper.ask(open_section(section));
    }
    if topic.sections.is_empty() {
        return;
    }
    ui.add_space(SCREEN_GAP / 2.0);
    divider(ui);
    ui.add_space(SCREEN_GAP / 2.0);
    desk_label(ui, "What this topic holds", Type::NOTE, Ink::Muted);
    faded(ui, |ui| sections(ui, paper, topic, false));
}

/// What `/setup` asks for a topic, in the empty state's sentence.
fn asks(topic: TopicId) -> Option<&'static str> {
    match topic {
        TopicId::Identity => Some("your name, where you live and the languages you speak"),
        TopicId::Experience => Some("about each role you held, newest first"),
        TopicId::Education => Some("about your degrees and courses"),
        TopicId::Skills => Some("what you know and how well"),
        TopicId::Publications => Some("about your side projects, papers and awards"),
        TopicId::References => Some("who can vouch for you"),
        TopicId::HowIWork => Some("how you like to work and where you do your best work"),
        TopicId::Goals => Some("what you want next, what you will not accept and how far you will travel"),
        TopicId::JobSearch => Some("what to search for, where and in which languages"),
        TopicId::Statements | TopicId::Stories => None,
    }
}

/// A topic whose file could not be read: hidden, not gone, and back once the file is fixed.
/// The note names no count of other topics: the screen cannot tell which file hid which topic.
fn hidden(ui: &mut Ui, topic: &TopicView) {
    ui.add_space(SCREEN_GAP / 2.0);
    empty_note(ui, hidden_title(topic.id), HIDDEN);
}

fn hidden_title(topic: TopicId) -> &'static str {
    match topic {
        TopicId::Identity => "Your identity and languages are hidden, not gone",
        TopicId::Experience => "Your experience is hidden, not gone",
        TopicId::Education => "Your education is hidden, not gone",
        TopicId::Skills => "Your skills are hidden, not gone",
        TopicId::Publications => "Your projects and publications are hidden, not gone",
        TopicId::References => "Your references are hidden, not gone",
        TopicId::Statements => "Your profile statements are hidden, not gone",
        TopicId::HowIWork => "How you work is hidden, not gone",
        TopicId::Goals => "Your goals and deal-breakers are hidden, not gone",
        TopicId::Stories => "Your interview stories are hidden, not gone",
        TopicId::JobSearch => "Your job search is hidden, not gone",
    }
}

/// Where the topic lives: the only place a file name shows on this screen. The count to fill sits at the right, or how
/// long ago a save found the open block changed on disk. The headings are cut to the line, with the whole on hover.
fn foot(ui: &mut Ui, topic: &TopicView, changed: Option<String>) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let right = match (changed, topic.fill) {
            (Some(age), _) => Some(format!("Changed on disk {age}")),
            (None, Fill::ToFill(left)) => Some(format!("{left} to fill in this topic")),
            (None, _) => None,
        };
        if let Some(right) = right {
            desk_label(ui, right, Type::NOTE, Ink::Muted);
            ui.add_space(ITEM_GAP * 2.0);
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            desk_label(ui, "Kept in ", Type::NOTE, Ink::Muted);
            for (index, file) in topic.files.iter().enumerate() {
                if index > 0 {
                    desk_label(ui, ", ", Type::NOTE, Ink::Muted);
                }
                faint_mono(ui, *file);
            }
            if !topic.headings.is_empty() {
                ui.add(Label::new(rich(ui, format!(", under {}", and_list(&topic.headings)), Type::NOTE, Ink::Muted)).truncate());
            }
        });
    });
}

/// No profile file in the folder: one way in, through `/setup`.
fn no_profile(ui: &mut Ui) {
    let body = "Your job search folder holds none of the profile files yet. Claude asks about you, your work and what you want, then writes them.";
    empty_state(ui, "No profile yet", body, |ui| {
        primary_command_action(ui, "Set up your profile…", "/setup");
    });
}

/// The paper before the first read: grey lines where the head and three roles go.
fn loading_paper(ui: &mut Ui) {
    paper(ui, false, |ui| {
        ui.set_min_height(ui.available_height());
        let (x, y) = DEAL_PADDING;
        Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            skeleton_lines(ui, &[220.0, 380.0], skeleton);
        });
        divider(ui);
        Frame::new().inner_margin(Margin::symmetric(x, 0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for index in 0..3 {
                if index > 0 {
                    divider(ui);
                }
                ui.add_space(f32::from(y));
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(ROLE_DATES, SKELETON_LINE), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(ROLE_DATES);
                        skeleton(ui, 80.0, SKELETON_LINE);
                    });
                    skeleton_lines(ui, &[240.0, 180.0, 520.0, 460.0, 380.0], skeleton);
                });
                ui.add_space(f32::from(y) - SKELETON_GAP);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    #[test]
    fn the_head_line_says_how_long_ago_and_what_changed() {
        let mark = Mark { at: at(9, 30), blocks: vec!["Lead Engineer - Calder (2021 – now)".to_owned(), "the section".to_owned()] };
        assert_eq!(updated_words(&mark, at(9, 30)), "by the last task, just now. It changed Lead Engineer - Calder (2021 – now) and the section.");
        assert_eq!(age(at(9, 30), at(9, 31)), "1 minute ago");
        assert_eq!(age(at(9, 30), at(9, 34)), "4 minutes ago");
        assert_eq!(age(at(9, 30), at(10, 45)), "1 hour ago");
        assert_eq!(age(at(9, 30), at(12, 0)), "2 hours ago");
        let hidden = Mark { at: at(9, 30), blocks: vec!["Lead\u{202e} Engineer".to_owned()] };
        assert_eq!(updated_words(&hidden, at(9, 30)), "by the last task, just now. It changed Lead Engineer.");
    }

    #[test]
    fn the_window_wakes_when_the_age_words_change() {
        let after = |minutes: i64, seconds: i64| at(9, 30) + chrono::Duration::minutes(minutes) + chrono::Duration::seconds(seconds);
        assert_eq!(next_age_change(at(9, 30), at(9, 30)), Duration::from_secs(60));
        assert_eq!(next_age_change(at(9, 30), after(0, 45)), Duration::from_secs(15));
        assert_eq!(next_age_change(at(9, 30), after(4, 10)), Duration::from_secs(50));
        assert_eq!(next_age_change(at(9, 30), after(75, 0)), Duration::from_secs(45 * 60));
        assert_eq!(age(at(9, 30), after(0, 59)), "just now");
        assert_eq!(age(at(9, 30), after(1, 0)), "1 minute ago");
    }

    fn view(candidate: &str, claude: Option<&str>) -> ProfileView {
        let (candidate, claude) = (candidate.to_owned(), claude.map(str::to_owned));
        crate::profile::application::profile_view(&|file| {
            Ok(match file {
                ProfileFile::Candidate => Some(candidate.clone()),
                ProfileFile::ClaudeMd => claude.clone(),
                _ => None,
            })
        })
        .0
    }

    fn topic(view: &ProfileView, id: TopicId) -> &TopicView {
        view.topics.iter().find(|topic| topic.id == id).unwrap()
    }

    /// The lead counts the roles, and claims no order the file may not keep.
    #[test]
    fn experience_counts_its_roles() {
        let two = view("## Professional Experience\n\n### Dev - Acme (2018 – 2021)\nBuilt.\n\n### Lead - Calder (2021 – now)\nShipped.\n", None);
        assert_eq!(lead(topic(&two, TopicId::Experience)), "2 roles. Claude picks from these for each CV.");
        let none = view("## Professional Experience\nI freelance.\n", None);
        assert_eq!(lead(topic(&none, TopicId::Experience)), "No roles here yet. Claude picks from your roles for each CV.");
        assert!(!lead(topic(&two, TopicId::Experience)).contains("newest"));
    }

    /// A part from CLAUDE.md says so, and the one part beside it takes a short label, so the two never read alike.
    #[test]
    fn sections_from_claude_md_are_named_by_where_they_live() {
        let candidate = "## Professional Experience\n\n### Lead - Calder (2021 – now)\nShipped.\n\n## Technical Skills\nRust\n";
        let claude = "## Candidate Profile\n\n### Professional Experience\n- Lead at Calder\n\n### Technical Skills\n- Rust\n\n### Certifications\n- CKA\n\n### Deal-breakers\n- No travel\n";
        let view_of = view;
        let view = view_of(candidate, Some(claude));
        let labels = |id: TopicId| {
            let topic = topic(&view, id);
            topic.sections.iter().map(|section| section_label(topic, section)).collect::<Vec<_>>()
        };
        assert_eq!(labels(TopicId::Experience), ["Roles", "Summary in CLAUDE.md"]);
        assert_eq!(labels(TopicId::Skills), ["Technical Skills", "Technical Skills in CLAUDE.md", "Certifications in CLAUDE.md"]);
        assert_eq!(labels(TopicId::Goals), ["Deal-breakers in CLAUDE.md"], "a part that sums up nothing of the topic's keeps its name");
        let no_roles = view_of("## Professional Experience\nI freelance.\n", Some(claude));
        let experience = topic(&no_roles, TopicId::Experience);
        let labels: Vec<String> = experience.sections.iter().map(|section| section_label(experience, section)).collect();
        assert_eq!(labels, ["Professional Experience", "Summary in CLAUDE.md"], "Roles only where the section holds roles");
    }

    #[test]
    fn lists_read_as_a_sentence() {
        let words = |items: &[&str]| and_list(&items.iter().map(|item| item.to_string()).collect::<Vec<_>>());
        assert_eq!(words(&["A"]), "A");
        assert_eq!(words(&["A", "B"]), "A and B");
        assert_eq!(words(&["A", "B", "C"]), "A, B and C");
    }
}
