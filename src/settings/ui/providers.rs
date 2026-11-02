use chrono::NaiveDateTime;
use egui::{Align, Label, Layout, Rect, Response, Sense, Ui, UiBuilder, pos2, vec2};

use crate::settings::application::{
    ClaudeVersion, KeyFailure, KeyMask, Outcome, Page, Providers, Saved, Settings, SignIn, SignInCheck, TestGate, TestTarget, age_words, index, sign_in_words,
};
use crate::settings::ui::SettingsRequest;
use crate::settings::ui::about::open;
use crate::ui_kit::tokens::{BUTTON_GAP, COMMAND_NAME_WIDTH, FIELD_GAP, ITEM_GAP, NOTE_PADDING, PICK_PADDING, ROW_ITEM_GAP, ROW_TAG_HEIGHT};
use crate::ui_kit::{
    Confirm, Ink, Secret, Tone, Type, button, code_box, confirm_popover, divider, enabled_when, hit_mark, link, paper, pick_row, primary_button,
    prompt_box_tone, rich, row_tag, ruled_panel, secret_field, small_button, small_danger_button, tag,
};

/// Where a key is made. A fixed `https` address, never read from a file.
const CONSOLE_URL: &str = "https://console.anthropic.com/settings/keys";
const CONSOLE_LINK: &str = "Create a key in the Console";
/// The keychain item the key is kept in, as the system keychain adapter names it.
const ITEM: &str = "Jobdesk API key";
const KEYCHAIN: &str = if cfg!(windows) { "Credential Manager" } else { "macOS Keychain" };
const ONLY_KEYCHAIN: &str = if cfg!(windows) {
    "Jobdesk runs Claude Code, which signs in on its own. Only an API key is kept by Jobdesk, and only in Credential Manager."
} else {
    "Jobdesk runs Claude Code, which signs in on its own. Only an API key is kept by Jobdesk, and only in your keychain."
};
const NOT_YET: &str = "Not available yet.";
const BLANK: &str = "Paste a key first.";
const TEST: &str = "Test connection";
const USE_FOR_TASKS: &str = "Use for tasks";
const NO_TASK_YET: &str = "The sign-in each task uses shows here after your first task. Claude Code reports it itself.";
const REPORTS_ITSELF: &str = "Claude Code reports this itself.";

/// How wide the ways in are, and the gap to the detail beside them, as on the canvas.
const WAYS_WIDTH: f32 = 290.0;
const COLUMN_GAP: f32 = 20.0;
/// The room above the ways, under the note on the last task.
const BODY_GAP: f32 = 16.0;
/// The gap between a way's title and its line.
const WAY_LINE_GAP: f32 = 2.0;
/// The detail's padding, left and right, top, bottom, and the gap between its parts.
const DETAIL_PADDING: (i8, i8, i8) = (16, 14, 16);
const DETAIL_GAP: f32 = 12.0;
/// The least height of a row in the detail, and the room above and under its words.
const KV_MIN: f32 = 40.0;
const KV_PADDING: f32 = 6.0;
/// How wide Remove's popover is.
const REMOVE_WIDTH: f32 = 340.0;

/// The way picked on the left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    ClaudeSignIn,
    ApiKey,
}

/// What the page keeps between frames: the way picked, the key as typed, and Remove's popover.
#[derive(Debug, Default)]
pub struct ProvidersScreen {
    /// None until the page opens, which picks the way tasks use.
    way: Option<Way>,
    secret: Secret,
    /// Replace opened the field over a saved key.
    replacing: bool,
    /// Remove's popover: open, and whether Keep it still takes the focus.
    removing: Option<bool>,
    /// Why the last action failed, in words that never hold the key.
    problem: Option<String>,
}

impl ProvidersScreen {
    /// The page was left or Settings closed: the field forgets what was typed, and the way in use is picked next time.
    pub fn reset(&mut self) {
        self.secret.clear();
        (self.way, self.replacing, self.removing, self.problem) = (None, false, None, None);
    }

    /// Whether Remove's popover asks, so Esc is its own.
    pub fn asking(&self) -> bool {
        self.removing.is_some()
    }
}

/// The newest task that said how it signed in, as the run desk knows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LastTask {
    /// The CLI's `apiKeySource`.
    pub source: String,
    /// How Jobdesk started it, which says whether a key it reports came from Jobdesk.
    pub method: SignIn,
    /// When it started, if its start reads as a time.
    pub at: Option<NaiveDateTime>,
}

/// What the page shows that the shell knows.
#[derive(Clone, Copy)]
pub struct ProvidersView<'a> {
    pub last_task: Option<&'a LastTask>,
    pub claude: ClaudeVersion<'a>,
    /// A task is working, so Test connection waits.
    pub task_running: bool,
    /// The app's clock, for each age shown.
    pub now: NaiveDateTime,
}

/// The texts a way in shows, and its tag.
struct WayLine<'a> {
    title: &'a str,
    line: &'a str,
    tag: (&'static str, Tone),
}

/// The note on the last task, the three ways in at the left, and the one picked in detail at the right.
/// Returns what the user asked of the shell.
pub fn body(
    ui: &mut Ui,
    screen: &mut ProvidersScreen,
    settings: &mut Settings,
    providers: &mut Providers,
    view: ProvidersView,
    query: &str,
) -> Option<SettingsRequest> {
    let &[login, login_line, key, key_line, cloud, cloud_line, login_text, claude_code, switch, check, test_help] = index(Page::Providers) else { return None };
    let in_use = providers.task_sign_in(settings.preferences());
    let way = *screen.way.get_or_insert(match in_use {
        SignIn::ApiKey => Way::ApiKey,
        SignIn::ClaudeSignIn => Way::ClaudeSignIn,
    });
    let mut request = None;
    if last_task_note(ui, view.last_task, view.now) {
        request = Some(SettingsRequest::OpenLastTask);
    }
    ui.add_space(BODY_GAP);
    let login_tag = if in_use == SignIn::ClaudeSignIn { ("In use", Tone::Hired) } else { ("Available", Tone::Plain) };
    let key_tag = key_tag(providers, settings, in_use);
    let lines = [
        WayLine { title: login, line: login_line, tag: login_tag },
        WayLine { title: key, line: key_line, tag: key_tag },
        WayLine { title: cloud, line: cloud_line, tag: ("Later", Tone::Plain) },
    ];
    let spacing = ui.spacing().item_spacing;
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 0.0), Layout::left_to_right(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let width = WAYS_WIDTH.min(ui.available_width() / 2.0);
        ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = spacing;
            if let Some(picked) = ways(ui, way, &lines, query)
                && picked != way
            {
                screen.reset();
                screen.way = Some(picked);
            }
        });
        ui.add_space(COLUMN_GAP);
        let width = ui.available_width();
        ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = spacing;
            detail_frame(ui, |ui| match screen.way.unwrap_or(way) {
                Way::ClaudeSignIn => {
                    let words = SignInWords { title: login, text: login_text, claude_code, switch, check, test_help };
                    sign_in_detail(ui, screen, settings, providers, view, &words, (in_use, login_tag), query);
                }
                Way::ApiKey => key_detail(ui, screen, settings, providers, view, key, (in_use, key_tag)),
            });
        });
    });
    request
}

/// The API key's tag: in use, refused by its last test, saved, or not set.
fn key_tag(providers: &Providers, settings: &Settings, in_use: SignIn) -> (&'static str, Tone) {
    let saved = matches!(providers.saved(), Saved::Present(_) | Saved::Unreadable(_));
    let refused = settings.preferences().key_test.is_some_and(|test| !test.passed);
    match in_use {
        SignIn::ApiKey => ("In use", Tone::Hired),
        SignIn::ClaudeSignIn if saved && refused => ("Refused", Tone::Closed),
        SignIn::ClaudeSignIn if saved => ("Available", Tone::Plain),
        SignIn::ClaudeSignIn => ("Not set", Tone::Plain),
    }
}

/// The line on how the last task signed in, as Claude Code reported it, with Open the task. Before any task, it says
/// it will show. Returns whether Open the task was pressed.
fn last_task_note(ui: &mut Ui, last: Option<&LastTask>, now: NaiveDateTime) -> bool {
    let (x, y) = NOTE_PADDING;
    ruled_panel(ui, (x, y, y), |ui| {
        let Some(last) = last else {
            ui.add(Label::new(rich(ui, NO_TASK_YET, Type::BODY, Ink::Muted)).wrap());
            return false;
        };
        let strong = format!("Last task signed in with {}", sign_in_words(&last.source, last.method));
        let rest = match last.at {
            Some(at) => format!(", {}. {REPORTS_ITSELF}", age_words(at, now)),
            None => format!(". {REPORTS_ITSELF}"),
        };
        ui.horizontal(|ui| {
            // The link goes in first, at the right, so the sentence wraps in the room it leaves.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let open = link(ui, "Open the task").clicked();
                ui.add_space(ROW_ITEM_GAP);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(rich(ui, strong, Type::STRONG, Ink::Text));
                        ui.label(rich(ui, rest, Type::BODY, Ink::Muted));
                    });
                });
                open
            })
            .inner
        })
        .inner
    })
}

/// The three ways in, one under the other, with the one picked marked. Cloud providers waits, faded.
/// Returns the way clicked.
fn ways(ui: &mut Ui, way: Way, lines: &[WayLine; 3], query: &str) -> Option<Way> {
    let mut picked = None;
    paper(ui, false, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (at, line) in lines.iter().enumerate() {
            if at > 0 {
                divider(ui);
            }
            let this = [Some(Way::ClaudeSignIn), Some(Way::ApiKey), None][at];
            let row = |ui: &mut Ui| pick_row(ui, this == Some(way), |ui| way_row(ui, line, query));
            let clicked = match this {
                Some(_) => row(ui).clicked(),
                None => enabled_when(ui, false, NOT_YET, row).clicked(),
            };
            if clicked {
                picked = this;
            }
        }
    });
    ui.add_space(f32::from(PICK_PADDING.0));
    ui.add(Label::new(rich(ui, ONLY_KEYCHAIN, Type::NOTE, Ink::Muted)).wrap());
    picked
}

/// A way's title over its line, and its tag at the top right.
/// The line is in `Type::NOTE`, as on the canvas, where `title_over`'s body type would cut "Amazon Bedrock, Google
/// Vertex AI". Both mark the hits of a search.
/// The tag sits beside the title only, so the line under it takes the row's whole width.
fn way_row(ui: &mut Ui, line: &WayLine, query: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = WAY_LINE_GAP;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), ROW_TAG_HEIGHT), Layout::right_to_left(Align::Center), |ui| {
            let (text, tone) = line.tag;
            row_tag(ui, text, tone);
            ui.add_space(ITEM_GAP);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| hit_mark(ui, line.title, Type::STRONG, Ink::Text, query));
        });
        hit_mark(ui, line.line, Type::NOTE, Ink::Muted, query);
    });
}

/// The detail of the way picked: a panel with a hairline round it, which still shows in dark, where panel and paper meet.
fn detail_frame(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ruled_panel(ui, DETAIL_PADDING, |ui| {
        ui.spacing_mut().item_spacing.y = DETAIL_GAP;
        add(ui);
    });
}

/// The detail's title and its tag.
fn detail_head(ui: &mut Ui, title: &str, (text, tone): (&str, Tone)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
        ui.label(rich(ui, title, Type::SLIP_TITLE, Ink::Text));
        tag(ui, text, tone);
    });
}

/// The detail's rows, ruled off from each other and from what is above them.
fn kv_rows(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui);
    });
}

/// One row of the detail: `label` in a column at the left, `value` beside it, `actions` at the right.
/// `align` is `Center` for a one-line value and `Min` for one that wraps.
fn kv(ui: &mut Ui, label: &str, query: &str, align: Align, value: impl FnOnce(&mut Ui), actions: impl FnOnce(&mut Ui)) {
    divider(ui);
    let id = ui.next_auto_id();
    let full = ui.available_width();
    let origin = ui.cursor().min;
    let least = KV_MIN - KV_PADDING * 2.0;
    let band = Rect::from_min_size(pos2(origin.x, origin.y + KV_PADDING), vec2(full, least));
    let part = |ui: &mut Ui, salt: &str, rect: Rect| ui.new_child(UiBuilder::new().id_salt(id.with(salt)).max_rect(rect).layout(Layout::left_to_right(align)));
    // The actions go in first, at the right, so the value takes the room they leave.
    let mut side = ui.new_child(UiBuilder::new().id_salt(id.with("kv_actions")).max_rect(band).layout(Layout::right_to_left(align)));
    side.spacing_mut().item_spacing.x = BUTTON_GAP;
    actions(&mut side);
    let used = side.min_rect();
    let start = band.left() + COMMAND_NAME_WIDTH;
    let end = if used.width() > 0.0 { used.left() - ROW_ITEM_GAP } else { band.right() };
    let mut shown = part(ui, "kv_value", Rect::from_min_max(pos2(start, band.top()), pos2(end.max(start), band.bottom())));
    shown.spacing_mut().item_spacing.x = 0.0;
    value(&mut shown);
    let mut named = part(ui, "kv_label", Rect::from_min_size(band.min, vec2(COMMAND_NAME_WIDTH - ROW_ITEM_GAP, least)));
    hit_mark(&mut named, label, Type::NOTE, Ink::Muted, query);
    let height = [named.min_rect(), shown.min_rect(), used].iter().map(Rect::height).fold(least, f32::max);
    ui.allocate_rect(Rect::from_min_size(origin, vec2(full, height + KV_PADDING * 2.0)), Sense::hover());
}

/// Words in a row, one after another, each in its own type.
fn words(ui: &mut Ui, parts: &[(&str, Type, Ink)]) {
    for &(text, style, ink) in parts {
        ui.label(rich(ui, text, style, ink));
    }
}

/// The Claude sign-in's texts, as the index has them.
struct SignInWords<'a> {
    title: &'a str,
    text: &'a str,
    claude_code: &'a str,
    switch: &'a str,
    check: &'a str,
    test_help: &'a str,
}

/// Jobdesk stores nothing for the Claude sign-in: its detail names Claude Code's version, how to switch account, and
/// Test connection. Use for tasks shows while tasks use the key.
#[allow(clippy::too_many_arguments)]
fn sign_in_detail(
    ui: &mut Ui,
    screen: &mut ProvidersScreen,
    settings: &mut Settings,
    providers: &mut Providers,
    view: ProvidersView,
    words_of: &SignInWords,
    (in_use, tag): (SignIn, (&str, Tone)),
    query: &str,
) {
    detail_head(ui, words_of.title, tag);
    hit_mark(ui, words_of.text, Type::BODY, Ink::Text, query);
    let gate = providers.test_gate(view.task_running);
    let testing = providers.testing() == Some(TestTarget::ClaudeSignIn);
    let mut test = false;
    kv_rows(ui, |ui| {
        kv(
            ui,
            words_of.claude_code,
            query,
            Align::Center,
            |ui| match view.claude {
                ClaudeVersion::Found(version) => words(ui, &[(&format!("{version} on this computer"), Type::BODY, Ink::Text)]),
                ClaudeVersion::Checking => words(ui, &[("Checking…", Type::BODY, Ink::Muted)]),
                ClaudeVersion::NotFound => words(ui, &[("Not found on this computer", Type::BODY, Ink::Muted)]),
            },
            |ui| match view.claude {
                ClaudeVersion::Found(_) => {
                    row_tag(ui, "Installed", Tone::Hired);
                }
                ClaudeVersion::NotFound => {
                    row_tag(ui, "Missing", Tone::Closed);
                }
                ClaudeVersion::Checking => {}
            },
        );
        kv(
            ui,
            words_of.switch,
            query,
            Align::Center,
            |ui| {
                words(
                    ui,
                    &[
                        ("Run ", Type::BODY, Ink::Text),
                        ("claude", Type::MONO, Ink::Text),
                        (" in a terminal, then ", Type::BODY, Ink::Text),
                        ("/login", Type::MONO, Ink::Text),
                    ],
                )
            },
            |_| {},
        );
        kv(
            ui,
            words_of.check,
            query,
            Align::Min,
            |ui| {
                hit_mark(ui, words_of.test_help, Type::NOTE, Ink::Muted, query);
            },
            |ui| test = test_button(ui, gate, testing).clicked(),
        );
        if let Some(check) = providers.sign_in_check() {
            kv(ui, "Last test", "", Align::Center, |ui| last_check(ui, check.outcome, check.at, check.cost_usd, view.now), |_| {});
        }
    });
    if let Some(check) = providers.sign_in_check().filter(|check| check.outcome != Outcome::Passed) {
        sign_in_failure(ui, check);
    }
    if in_use != SignIn::ClaudeSignIn {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            if primary_button(ui, USE_FOR_TASKS).clicked() {
                providers.use_claude_sign_in(settings);
            }
            ui.add(Label::new(rich(ui, "Tasks go back to your Claude sign-in at once. The key stays saved.", Type::NOTE, Ink::Muted)).wrap());
        });
    }
    if test {
        screen.problem = providers.test_sign_in(view.task_running).err();
    }
    problem_line(ui, screen);
}

/// What a sign-in test that did not pass found, as plain text.
fn sign_in_failure(ui: &mut Ui, check: &SignInCheck) {
    let headline = match check.outcome {
        Outcome::Refused => "Claude Code is not signed in.",
        _ => "Claude Code could not be reached.",
    };
    failure_box(ui, headline, &check.said, "Run claude in a terminal, then /login, and test again.");
}

/// A test that did not pass: what it means in `Closed`, what Claude Code said as plain text, then what to do.
fn failure_box(ui: &mut Ui, headline: &str, said: &str, next: &str) {
    prompt_box_tone(ui, Tone::Closed, |ui| {
        ui.spacing_mut().item_spacing.y = ROW_ITEM_GAP - 2.0;
        ui.label(rich(ui, headline, Type::STRONG, Tone::Closed));
        if !said.is_empty() {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = FIELD_GAP;
                ui.label(rich(ui, "It said", Type::NOTE, Ink::Muted));
                // Plain text only, never Markdown: it comes from outside Jobdesk.
                code_box(ui, |ui| ui.add(Label::new(rich(ui, said, Type::MONO, Ink::Text)).wrap()));
            });
        }
        ui.add(Label::new(rich(ui, next, Type::BODY, Ink::Text)).wrap());
    });
}

/// Test connection, which reads Testing… while its test runs, and waits with its reason while it cannot start.
fn test_button(ui: &mut Ui, gate: TestGate, testing: bool) -> Response {
    let label = if testing || gate == TestGate::Testing { "Testing…" } else { TEST };
    enabled_when(ui, gate == TestGate::Ready, gate.reason().unwrap_or_default(), |ui| small_button(ui, label))
}

/// How a test went: its tag, then its age and its cost, which shows only here.
fn last_check(ui: &mut Ui, outcome: Outcome, at: NaiveDateTime, cost_usd: f64, now: NaiveDateTime) {
    let (text, tone) = match outcome {
        Outcome::Passed => ("Passed", Tone::Hired),
        Outcome::Refused => ("Failed", Tone::Closed),
        Outcome::Unreached => ("Not reached", Tone::Plain),
    };
    ui.spacing_mut().item_spacing.x = ITEM_GAP + 2.0;
    row_tag(ui, text, tone);
    ui.label(rich(ui, format!("{}, for {}", age_words(at, now), cost_words(cost_usd)), Type::NOTE, Ink::Muted));
}

/// A test's cost: a fraction of a cent keeps its third decimal, so about a cent never reads as nothing.
fn cost_words(usd: f64) -> String {
    if usd > 0.0 && usd < 0.01 { format!("${usd:.3}") } else { format!("${usd:.2}") }
}

/// Why the last action failed, if it did, in `Closed`.
fn problem_line(ui: &mut Ui, screen: &ProvidersScreen) {
    if let Some(problem) = &screen.problem {
        ui.add(Label::new(rich(ui, problem, Type::NOTE, Tone::Closed)).wrap());
    }
}

/// The API key: the field to paste one, the saved one, or what its failed test said.
fn key_detail(
    ui: &mut Ui,
    screen: &mut ProvidersScreen,
    settings: &mut Settings,
    providers: &mut Providers,
    view: ProvidersView,
    title: &str,
    (in_use, tag): (SignIn, (&str, Tone)),
) {
    detail_head(ui, title, tag);
    let in_use = in_use == SignIn::ApiKey;
    match providers.saved().clone() {
        Saved::Present(mask) if !screen.replacing => match providers.key_failure().cloned() {
            // A test of the key that runs again shows as running, not as the failure before it.
            Some(failure) if providers.testing() != Some(TestTarget::Key) => failed_key(ui, screen, settings, providers, view, &mask, &failure),
            _ => saved_key(ui, screen, settings, providers, view, Ok(&mask), in_use),
        },
        Saved::Unreadable(why) if !screen.replacing => saved_key(ui, screen, settings, providers, view, Err(&why), in_use),
        _ => key_entry(ui, screen, settings, providers, view),
    }
    problem_line(ui, screen);
}

/// The field to paste a key, Save key, Cancel and the way to the Console. Save key writes the key to the keychain and
/// tests it, and the field is emptied either way.
fn key_entry(ui: &mut Ui, screen: &mut ProvidersScreen, settings: &mut Settings, providers: &mut Providers, view: ProvidersView) {
    ui.add(
        Label::new(rich(
            ui,
            "Paste a key from the Anthropic Console. Tasks then bill that Console account instead of your Claude plan.",
            Type::BODY,
            Ink::Text,
        ))
        .wrap(),
    );
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = FIELD_GAP;
        let label = ui.label(rich(ui, "Key", Type::NOTE, Ink::Muted));
        let field = secret_field(ui, &mut screen.secret, "sk-ant-…");
        ui.ctx().accesskit_node_builder(field.id, |node| node.set_label("API key"));
        field.labelled_by(label.id);
    });
    let (mut save, mut cancel) = (false, false);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        save = enabled_when(ui, !screen.secret.is_blank(), BLANK, |ui| primary_button(ui, "Save key")).clicked();
        cancel = button(ui, "Cancel").clicked();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| open(ui, CONSOLE_LINK, CONSOLE_URL));
    });
    let note = format!(
        "Save key keeps it in the {KEYCHAIN}, never in a file, then tests it for about a cent. Tasks switch to the key once the test passes. Jobdesk passes it only to the Claude Code process of each task."
    );
    ui.add(Label::new(rich(ui, note, Type::NOTE, Ink::Muted)).wrap());
    if save && !screen.secret.is_blank() {
        // Taken first, so the field is empty even when the keychain refuses the key.
        let typed = screen.secret.take();
        screen.problem = providers.save_key(&typed, settings, view.task_running).err();
        if screen.problem.is_none() {
            screen.replacing = false;
        }
    } else if cancel {
        let saved = matches!(providers.saved(), Saved::Present(_) | Saved::Unreadable(_));
        screen.reset();
        // With no key to go back to, Cancel goes back to the way tasks use.
        if saved {
            screen.way = Some(Way::ApiKey);
        }
    }
}

/// The key as the screen may show it: `sk-ant-`, the dots and its last four characters.
fn mask_words(ui: &mut Ui, mask: &KeyMask) {
    words(ui, &[(&mask.shown(), Type::MONO, Ink::Text)]);
    if let Some(ending) = mask.ending() {
        words(ui, &[(" ending in ", Type::BODY, Ink::Muted), (ending, Type::MONO, Ink::Text)]);
    }
}

/// The saved key: its mask with Replace and Remove, where it is kept, and its last test with Test connection.
/// `mask` is the keychain's error when the key could not be read.
fn saved_key(
    ui: &mut Ui,
    screen: &mut ProvidersScreen,
    settings: &mut Settings,
    providers: &mut Providers,
    view: ProvidersView,
    mask: Result<&KeyMask, &str>,
    in_use: bool,
) {
    let lead = if in_use {
        "Tasks use this key and bill its Console account. Jobdesk passes it only to the Claude Code process of each task."
    } else {
        "This key is saved, and tasks use your Claude sign-in. Jobdesk passes it only to the Claude Code process of each task it signs in."
    };
    ui.add(Label::new(rich(ui, lead, Type::BODY, Ink::Text)).wrap());
    let gate = providers.test_gate(view.task_running);
    let testing = providers.testing() == Some(TestTarget::Key);
    let (mut replace, mut remove, mut test) = (false, None, false);
    kv_rows(ui, |ui| {
        kv(
            ui,
            "Key",
            "",
            Align::Center,
            |ui| match mask {
                Ok(mask) => mask_words(ui, mask),
                Err(_) => words(ui, &[("Could not be read", Type::BODY, Tone::Closed.into())]),
            },
            |ui| {
                remove = Some(small_danger_button(ui, "Remove"));
                replace = small_button(ui, "Replace").clicked();
            },
        );
        kv(ui, "Kept in", "", Align::Center, |ui| words(ui, &[(&format!("{KEYCHAIN}, as "), Type::BODY, Ink::Text), (ITEM, Type::MONO, Ink::Text)]), |_| {});
        kv(
            ui,
            "Last test",
            "",
            Align::Center,
            |ui| match settings.preferences().key_test {
                _ if testing => words(ui, &[("Testing…", Type::NOTE, Ink::Muted)]),
                Some(last) => last_check(ui, if last.passed { Outcome::Passed } else { Outcome::Refused }, last.at, last.cost_usd, view.now),
                None => words(ui, &[("Not tested yet", Type::NOTE, Ink::Muted)]),
            },
            |ui| test = test_button(ui, gate, testing).clicked(),
        );
    });
    if let Err(why) = mask {
        ui.add(Label::new(rich(ui, why, Type::NOTE, Ink::Muted)).wrap());
    }
    let mut use_key = false;
    if in_use {
        ui.add(
            Label::new(rich(
                ui,
                "To go back to your Claude sign-in, pick it on the left and choose Use for tasks. The key stays saved.",
                Type::NOTE,
                Ink::Muted,
            ))
            .wrap(),
        );
    } else {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            use_key = enabled_when(ui, gate == TestGate::Ready, gate.reason().unwrap_or_default(), |ui| primary_button(ui, USE_FOR_TASKS)).clicked();
            ui.add(Label::new(rich(ui, "Tests the key first, for about a cent. Tasks switch to it once it passes.", Type::NOTE, Ink::Muted)).wrap());
        });
    }
    if replace {
        screen.replacing = true;
        screen.problem = None;
    }
    if test || use_key {
        screen.problem = providers.test_key(view.task_running).err();
    }
    if let Some(remove) = remove {
        ask_to_remove(screen, settings, providers, &remove);
    }
}

/// The key's last test did not pass: what Claude Code said, then the key with Remove, and Replace key and Test again.
/// A refused key leaves tasks on the Claude sign-in. A test that never got an answer says so, and changes no method.
fn failed_key(
    ui: &mut Ui,
    screen: &mut ProvidersScreen,
    settings: &mut Settings,
    providers: &mut Providers,
    view: ProvidersView,
    mask: &KeyMask,
    failure: &KeyFailure,
) {
    let (headline, next) = match failure.outcome {
        Outcome::Refused => (
            "Claude Code refused this key.",
            "Check that you pasted the whole key, or create a new one in the Console. Tasks keep using your Claude sign-in until a test passes.",
        ),
        _ => (
            "Claude Code could not test this key.",
            "Nothing was learnt about the key, so it stays saved and tasks sign in as before. Test again once Claude Code can answer.",
        ),
    };
    failure_box(ui, headline, &failure.said, next);
    let mut remove = None;
    kv_rows(ui, |ui| {
        kv(ui, "Key", "", Align::Center, |ui| mask_words(ui, mask), |ui| remove = Some(small_danger_button(ui, "Remove")));
        kv(ui, "Last test", "", Align::Center, |ui| last_check(ui, failure.outcome, failure.at, failure.cost_usd, view.now), |_| {});
    });
    let gate = providers.test_gate(view.task_running);
    let (mut replace, mut again) = (false, false);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        replace = primary_button(ui, "Replace key").clicked();
        again = enabled_when(ui, gate == TestGate::Ready, gate.reason().unwrap_or_default(), |ui| button(ui, "Test again")).clicked();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| open(ui, CONSOLE_LINK, CONSOLE_URL));
    });
    if replace {
        screen.replacing = true;
        screen.problem = None;
    }
    if again {
        screen.problem = providers.test_key(view.task_running).err();
    }
    if let Some(remove) = remove {
        ask_to_remove(screen, settings, providers, &remove);
    }
}

/// Remove asks first: Keep it is the default and takes the focus, and Esc keeps it. Remove key deletes the keychain
/// item, and tasks go back to the Claude sign-in.
fn ask_to_remove(screen: &mut ProvidersScreen, settings: &mut Settings, providers: &mut Providers, remove: &Response) {
    if remove.clicked() {
        screen.removing = Some(true);
    }
    let note = format!("Jobdesk deletes it from the {KEYCHAIN}, and tasks go back to your Claude sign-in.");
    let words = Confirm { question: "Remove the API key?", note: &note, keep: "Keep it", act: "Remove key", esc_note: "Esc keeps it." };
    if confirm_popover(remove, &mut screen.removing, &words, REMOVE_WIDTH) {
        screen.problem = providers.remove_key(settings).err();
        screen.replacing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fraction_of_a_cent_keeps_its_third_decimal() {
        assert_eq!(cost_words(0.004), "$0.004");
        assert_eq!(cost_words(0.0), "$0.00");
        assert_eq!(cost_words(0.012), "$0.01");
    }
}
