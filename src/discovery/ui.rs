use std::collections::HashSet;

use egui::Ui;

use crate::discovery::application::{
    CheckRow, FirstSearch, FitLevel, JobBoard, JobChecks, JobFilters, JobView, Lane, Shown, Signal, Verdict, fit_band, shown_in,
};
use crate::ui_kit::tokens::{ITEM_GAP, ROW_GAP, ROW_ITEM_GAP, SCREEN_GAP, SEGMENT_HEIGHT};
use crate::ui_kit::{
    CheckMark, ColumnWidth, Ink, Load, PickTable, ScreenState, Subtitle, TableColumn, Title, Tone, Type, check_tag, command_action, divider, empty_note,
    empty_state, link, muted, note, open_link, page_body, page_head, panel, panel_body, paper_with_footer, pick_table, primary_command_action, rich, score,
    segmented, skeleton, skeleton_lines, skeleton_table, stamp, stamp_square, strong, tag, title_over, with_side_column,
};

/// What the user is looking at on this screen.
#[derive(Default)]
pub struct JobsScreen {
    /// The list the user picked. Until they pick one, Jobs opens on New after the first search, else on Shortlist.
    lane: Option<Lane>,
    selected: Option<String>,
    /// Settings › Jobs view: which jobs the lists leave out.
    filters: JobFilters,
    /// Whether the jobs the filters hide show anyway, after the user pressed Show them.
    show_hidden: bool,
}

/// The segments in the order a person works through them.
const LANES: [Lane; 7] = [Lane::Shortlist, Lane::New, Lane::BelowThreshold, Lane::Excluded, Lane::CouldNotFetch, Lane::Skipped, Lane::Expired];
const WAITING: &str = "Waiting in Today";
const NOT_STATED: &str = "Not stated";

fn lane_label(lane: Lane) -> &'static str {
    match lane {
        Lane::Shortlist => "Shortlist",
        Lane::BelowThreshold => "Low score",
        Lane::Excluded => "Ruled out",
        Lane::CouldNotFetch => "Could not open",
        Lane::New => "New",
        Lane::Skipped => "Skipped",
        Lane::Expired => "Expired",
    }
}

/// Expired shows only once a job has expired, so an empty segment does not crowd the row.
fn lane_shown(lane: Lane, board: &JobBoard) -> bool {
    lane != Lane::Expired || board.count(lane) > 0
}

fn signal_tag(signal: &Signal) -> (String, Tone) {
    match signal {
        Signal::ClosingIn { days } => (format!("Closes in {}", days_words(*days)), Tone::Interview),
        Signal::PostedDaysAgo { days } => (format!("Posted {days} days ago"), Tone::Interview),
        Signal::LocationFlag => ("Location unclear".into(), Tone::Interview),
        Signal::LanguageFlag => ("Language unclear".into(), Tone::Interview),
        Signal::LocationFail => ("Location does not suit".into(), Tone::Closed),
        Signal::LanguageFail => ("Language does not suit".into(), Tone::Closed),
        Signal::GuardFlag => ("Check this ad".into(), Tone::Interview),
        Signal::ContractFail => ("Not freelance".into(), Tone::Closed),
        Signal::RateBelowFloor => ("Rate under your floor".into(), Tone::Closed),
    }
}

fn days_words(days: i64) -> String {
    match days {
        0 => "under a day".to_owned(),
        1 => "1 day".to_owned(),
        days => format!("{days} days"),
    }
}

fn fit_words(fit: FitLevel) -> &'static str {
    match fit {
        FitLevel::High => "high",
        FitLevel::Medium => "medium",
        FitLevel::Low => "low",
    }
}

fn verdict_tone(verdict: Verdict) -> Tone {
    match verdict {
        Verdict::Strong => Tone::Hired,
        Verdict::Good => Tone::Active,
        Verdict::Moderate => Tone::Interview,
        Verdict::Weak | Verdict::Poor => Tone::Closed,
    }
}

/// The stated rate without the floor it is read against. The slip shows the floor.
fn day_rate(job: &JobView) -> &str {
    let value = job.freelance.day_rate.value.as_str();
    value.split_once(", ").map_or(value, |(rate, _)| rate)
}

fn jobs_words(n: usize) -> String {
    if n == 1 { "1 job".to_owned() } else { format!("{n} jobs") }
}

fn score_jobs_label(new: usize) -> String {
    match new {
        0 => "Score new jobs".to_owned(),
        1 => "Score 1 new job".to_owned(),
        new => format!("Score {new} new jobs"),
    }
}

impl JobsScreen {
    /// Takes the Jobs view filters from Settings. They hide rows only: a job stays in the list `/rank` put it in.
    pub fn set_filters(&mut self, filters: JobFilters) {
        self.filters = filters;
    }

    /// Opens the New list, as the first search's end does, whatever list was open.
    pub fn open_new(&mut self) {
        self.lane = Some(Lane::New);
        self.selected = None;
    }

    /// The list on show: the one picked while it shows, else New after the first search, else Shortlist.
    fn lane(&self, board: &JobBoard) -> Lane {
        let unpicked = if board.first_search().is_some() { Lane::New } else { Lane::Shortlist };
        self.lane.filter(|lane| lane_shown(*lane, board)).unwrap_or(unpicked)
    }

    /// The jobs a list shows, and how many of them the filters hide. While hidden jobs show, the list shows every job.
    fn shown<'a>(&self, board: &'a JobBoard, lane: Lane, kept: &HashSet<String>) -> Shown<'a> {
        let shown = shown_in(board, lane, &self.filters, kept);
        if self.show_hidden && shown.hidden > 0 { Shown { jobs: board.jobs_in(lane).iter().collect(), hidden: shown.hidden } } else { shown }
    }

    /// `waiting` holds the key of every job with a card in Today, and `kept` every job whose flagged ad the user kept there.
    /// While the saved jobs cannot be read, the list and its segments are hidden, never shown as empty.
    /// Until the first read, the table's head and grey rows stand in for the jobs, so no lane reads as empty.
    pub fn show(&mut self, ui: &mut Ui, state: &ScreenState, board: &JobBoard, waiting: &HashSet<String>, kept: &HashSet<String>) {
        let subtitle = match state.load {
            Load::Reading => "Reading your saved jobs…",
            Load::Failed => "Not shown while your saved jobs cannot be read.",
            Load::Ready => "Scored from the job ad alone. Drafting an application also checks the company.",
        };
        let new = board.count(Lane::New);
        // After the first search, the note under the head offers the scoring, so the head keeps only the search.
        let first = (state.load == Load::Ready).then(|| board.first_search()).flatten();
        page_head(ui, Title::Slab("Jobs"), Some(Subtitle::Text(subtitle)), |ui| {
            if first.is_none() {
                primary_command_action(ui, &score_jobs_label(new), "/rank");
            }
            command_action(ui, "Find new jobs", "/scrape");
        });
        page_body(ui, |ui| {
            (state.alerts)(ui);
            match state.load {
                Load::Reading => loading(ui),
                Load::Failed => {
                    let body =
                        "Jobdesk changed nothing in your folder. The list comes back by itself once the saved jobs file is fixed, because Jobdesk watches it.";
                    empty_note(ui, "Your saved jobs are hidden, not gone", body);
                }
                Load::Ready => self.body(ui, board, waiting, kept),
            }
        });
    }

    fn body(&mut self, ui: &mut Ui, board: &JobBoard, waiting: &HashSet<String>, kept: &HashSet<String>) {
        if let Some(found) = board.first_search() {
            first_search_note(ui, found);
            ui.add_space(ROW_GAP);
        }
        let options: Vec<_> = LANES
            .into_iter()
            .filter(|lane| lane_shown(*lane, board))
            .map(|lane| (lane, format!("{}  {}", lane_label(lane), self.shown(board, lane, kept).jobs.len())))
            .collect();
        if let Some(lane) = segmented(ui, self.lane(board), &options) {
            self.lane = Some(lane);
            self.selected = None;
        }
        let lane = self.lane(board);
        let Shown { jobs, hidden } = self.shown(board, lane, kept);
        if hidden > 0 {
            ui.add_space(ITEM_GAP);
            self.hidden_line(ui, hidden);
        }
        ui.add_space(SCREEN_GAP);

        if jobs.is_empty() {
            if hidden == 0 {
                lane_empty(ui, lane, board);
            }
            return;
        }
        if !self.selected.as_ref().is_some_and(|key| jobs.iter().any(|job| &job.key == key)) {
            self.selected = jobs.first().map(|job| job.key.clone());
        }
        let selected = jobs.iter().copied().find(|job| Some(&job.key) == self.selected.as_ref());

        with_side_column(
            ui,
            340.0,
            |ui| {
                // The fixed columns overflow a narrow window, so the table scrolls sideways there.
                panel(ui, |ui| {
                    egui::ScrollArea::horizontal().id_salt("jobs_table_x").auto_shrink([false, true]).show(ui, |ui| self.table(ui, &jobs, waiting));
                });
                if jobs.iter().any(|job| waiting.contains(&job.key)) {
                    ui.add_space(ROW_GAP);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
                        stamp_square(ui, WAITING);
                        muted(ui, "Waiting in Today. Decide there, or open the drafting task from here.");
                    });
                }
            },
            |ui| {
                if let Some(job) = selected {
                    paper_with_footer(ui, |ui| details(ui, job, kept.contains(&job.key)), |ui| actions(ui, job));
                }
            },
        );
    }

    fn table(&mut self, ui: &mut Ui, jobs: &[&JobView], waiting: &HashSet<String>) {
        let shown = shown_columns(ui);
        let heads: Vec<_> = shown.iter().map(|(_, head)| *head).collect();
        let picked = jobs.iter().position(|job| Some(&job.key) == self.selected.as_ref());
        let table = PickTable { id_salt: "jobs_table", columns: &heads, row_height: ROW_HEIGHT, rows: jobs.len(), picked };
        if let Some(row) = pick_table(ui, &table, |row| spoken(jobs[row]), |ui, row, column| job_cell(ui, jobs[row], shown[column].0, waiting)) {
            self.selected = Some(jobs[row].key.clone());
        }
    }

    /// Says how many jobs of the open list the filters hide, with a link that shows them or hides them again.
    fn hidden_line(&mut self, ui: &mut Ui, hidden: usize) {
        let jobs = jobs_words(hidden);
        let (text, action) = if self.show_hidden {
            (format!("Showing {jobs} your Jobs view hides. "), "Hide them")
        } else {
            (format!("{jobs} hidden by Settings › Jobs view. "), "Show them")
        };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label(rich(ui, text, Type::NOTE, Ink::Muted));
            if link(ui, action).clicked() {
                self.show_hidden = !self.show_hidden;
            }
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobColumn {
    Score,
    Job,
    Where,
    DayRate,
    Closes,
    WorthKnowing,
    FoundOn,
    Waiting,
}

const COLUMNS: [(JobColumn, TableColumn<'static>); 8] = [
    (JobColumn::Score, TableColumn { head: "Score", width: ColumnWidth::Exact(60.0) }),
    (JobColumn::Job, TableColumn { head: "Job", width: ColumnWidth::Rest { least: 160.0 } }),
    (JobColumn::Where, TableColumn { head: "Where", width: ColumnWidth::Exact(112.0) }),
    (JobColumn::DayRate, TableColumn { head: "Day rate", width: ColumnWidth::Exact(80.0) }),
    (JobColumn::Closes, TableColumn { head: "Closes", width: ColumnWidth::Exact(96.0) }),
    (JobColumn::WorthKnowing, TableColumn { head: "Worth knowing", width: ColumnWidth::Exact(168.0) }),
    (JobColumn::FoundOn, TableColumn { head: "Found on", width: ColumnWidth::Exact(84.0) }),
    (JobColumn::Waiting, TableColumn { head: "", width: ColumnWidth::Exact(24.0) }),
];

/// Below this window width the table drops its Found on column.
const FOUND_ON_FROM_WIDTH: f32 = 1100.0;

/// The columns the table shows. The detail names the site too, so a narrow window drops the column.
fn shown_columns(ui: &Ui) -> Vec<(JobColumn, TableColumn<'static>)> {
    let wide = ui.ctx().content_rect().width() >= FOUND_ON_FROM_WIDTH;
    COLUMNS.into_iter().filter(|(column, _)| wide || *column != JobColumn::FoundOn).collect()
}

/// The table's row height, read or loading.
const ROW_HEIGHT: f32 = 58.0;

/// What a screen reader says for a job's row, such as "Tech Lead Java, Real-Time Traffic, Meridian Rail Systems, 84".
fn spoken(job: &JobView) -> String {
    let score = job.score.map_or_else(|| "not scored".to_owned(), |score| score.to_string());
    format!("{}, {}, {score}", job.title, job.company)
}

fn job_cell(ui: &mut Ui, job: &JobView, column: JobColumn, waiting: &HashSet<String>) {
    match column {
        JobColumn::Score => {
            score(ui, job.score.map_or_else(|| job.scraper_fit.map_or("-", fit_words).to_owned(), |s| s.to_string()));
        }
        JobColumn::Job => {
            title_over(ui, &job.title, &job.company);
        }
        JobColumn::Where => {
            ui.add(egui::Label::new(rich(ui, job.location.as_deref().unwrap_or(NOT_STATED), Type::BODY, Ink::Muted)).wrap());
        }
        JobColumn::DayRate => {
            let rate = day_rate(job);
            let ink = if rate == NOT_STATED { Ink::Faint } else { Ink::Text };
            ui.add(egui::Label::new(rich(ui, rate, Type::BODY, ink)).wrap());
        }
        JobColumn::Closes => {
            match job.deadline {
                Some(deadline) => ui.label(deadline.format("%a %-d %b").to_string()),
                None => ui.label(rich(ui, "None", Type::BODY, Ink::Faint)),
            };
        }
        JobColumn::WorthKnowing => {
            if let Some((first, rest)) = job.signals.split_first() {
                let (text, tone) = signal_tag(first);
                tag(ui, text, tone);
                if !rest.is_empty() {
                    muted(ui, format!("+{}", rest.len())).on_hover_ui(|ui| {
                        let also: Vec<_> = rest.iter().map(|signal| signal_tag(signal).0).collect();
                        ui.label(format!("Also: {}", also.join(", ")));
                    });
                }
            }
        }
        JobColumn::FoundOn => {
            if let Some(site) = &job.found_on {
                ui.add(egui::Label::new(site.as_str()).truncate());
            }
        }
        JobColumn::Waiting => {
            if waiting.contains(&job.key) {
                stamp_square(ui, WAITING);
            }
        }
    }
}

/// Grey blocks where each column's content goes, by the column.
fn column_skeleton(column: JobColumn) -> &'static [f32] {
    match column {
        JobColumn::Score => &[32.0],
        JobColumn::Job => &[190.0, 120.0],
        JobColumn::Where => &[80.0],
        JobColumn::DayRate => &[56.0],
        JobColumn::Closes => &[64.0],
        JobColumn::WorthKnowing => &[110.0],
        JobColumn::FoundOn => &[56.0],
        JobColumn::Waiting => &[],
    }
}

const LOADING_ROWS: usize = 6;

/// The shape of the screen before the first read: the segments, the table's head over grey rows, and the details beside it.
fn loading(ui: &mut Ui) {
    skeleton(ui, 520.0, SEGMENT_HEIGHT);
    ui.add_space(SCREEN_GAP);
    with_side_column(
        ui,
        340.0,
        |ui| {
            panel(ui, |ui| {
                // In the same sideways scroll as the table, so the columns land where the rows will.
                egui::ScrollArea::horizontal().id_salt("jobs_loading_x").auto_shrink([false, true]).show(ui, |ui| {
                    let shown = shown_columns(ui);
                    let heads: Vec<_> = shown.iter().map(|(_, head)| *head).collect();
                    skeleton_table(ui, &heads, ROW_HEIGHT, LOADING_ROWS, |column| column_skeleton(shown[column].0));
                });
            });
        },
        |ui| {
            panel(ui, |ui| {
                panel_body(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = ROW_GAP;
                        skeleton(ui, 62.0, 50.0);
                        skeleton_lines(ui, &[120.0, 200.0, 160.0], skeleton);
                    });
                });
                divider(ui);
                panel_body(ui, |ui| skeleton_lines(ui, &[260.0, 230.0, 250.0, 200.0], skeleton));
            });
        },
    );
}

/// What the first search found, over the lists, with the scoring it leads to. Score N new jobs… only opens the
/// launcher on `/rank`, and the user starts the run there. The note gives the time, never a spend.
fn first_search_note(ui: &mut Ui, found: FirstSearch) {
    note(ui, |ui| {
        ui.horizontal(|ui| {
            // The words leave the button its room, and wrap in a narrow window.
            let words = (ui.available_width() - NOTE_ACTION_ROOM).max(0.0);
            ui.vertical(|ui| {
                ui.set_max_width(words);
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add(egui::Label::new(rich(ui, found.found(), Type::STRONG, Ink::Text)).wrap());
                let why = "Scoring reads each ad against your profile and puts the best fits first. It takes about 5 minutes.";
                ui.add(egui::Label::new(rich(ui, why, Type::BODY, Ink::Muted)).wrap());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                primary_command_action(ui, &found.score_label(), "/rank");
            });
        });
    });
}

/// The room the first search note keeps at its right for Score N new jobs….
const NOTE_ACTION_ROOM: f32 = 240.0;

fn lane_empty(ui: &mut Ui, lane: Lane, board: &JobBoard) {
    let new = board.count(Lane::New);
    match lane {
        Lane::Shortlist if new > 0 => {
            let title = if new == 1 { "1 new job waits for a score.".to_owned() } else { format!("{new} new jobs wait for a score.") };
            empty_state(ui, &title, "", |ui| {
                primary_command_action(ui, &score_jobs_label(new), "/rank");
            });
        }
        Lane::Shortlist | Lane::New => {
            empty_state(ui, "No jobs wait here.", "Search your job sites for new ones.", |ui| {
                command_action(ui, "Find new jobs", "/scrape");
            });
        }
        _ => empty_note(ui, &format!("Nothing in {}.", lane_label(lane)), ""),
    }
}

/// The verdict and the checks in one line, worded as the Today card words them.
/// The page subtitle already says the score rests on the ad alone, so the line leaves that out.
fn verdict_line(job: &JobView, checks: &JobChecks) -> String {
    match (job.verdict, job.scraper_fit) {
        (Some(verdict), _) => format!("{} {}", fit_band(Some(verdict), job.score.unwrap_or_default()), checks.summary),
        (None, Some(fit)) => format!("Not scored yet. The search guessed a {} fit.", fit_words(fit)),
        (None, None) => "Not scored yet.".to_owned(),
    }
}

fn details(ui: &mut Ui, job: &JobView, kept: bool) {
    let checks = JobChecks::of(job, job.guard_flagged && kept);
    panel_body(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ROW_GAP;
            match (job.score, job.verdict) {
                (Some(value), Some(verdict)) => stamp(ui, &value.to_string(), verdict_tone(verdict)),
                _ => stamp(ui, "-", Tone::Plain),
            };
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                muted(ui, job.company.as_str());
                ui.add(egui::Label::new(rich(ui, job.title.as_str(), Type::SLIP_TITLE, Ink::Text)).wrap());
                ui.add(egui::Label::new(rich(ui, verdict_line(job, &checks), Type::NOTE, Ink::Muted)).wrap());
            });
        });
    });
    divider(ui);
    panel_body(ui, |ui| {
        for check in &checks.rows {
            check_row(ui, check);
        }
        if let Some(attempts) = job.fetch_attempts {
            let tries = if attempts == 1 { "1 try".to_owned() } else { format!("{attempts} tries") };
            muted(ui, format!("Could not open after {tries}: {}", job.fetch_error.as_deref().unwrap_or("the page did not load")));
        }
        if !job.signals.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for signal in &job.signals {
                    let (text, tone) = signal_tag(signal);
                    tag(ui, text, tone);
                }
            });
        }
    });
    if !job.strengths.is_empty() || !job.gaps.is_empty() {
        divider(ui);
        panel_body(ui, |ui| {
            bullets(ui, "Why it fits", &job.strengths);
            if !job.strengths.is_empty() && !job.gaps.is_empty() {
                ui.add_space(ITEM_GAP);
            }
            bullets(ui, "What is missing", &job.gaps);
        });
    }
}

/// What the user can do with the job, pinned under its details.
fn actions(ui: &mut Ui, job: &JobView) {
    let apply = job.url.as_ref().map_or_else(|| "/apply".to_owned(), |url| format!("/apply {}", url.as_str()));
    primary_command_action(ui, "Draft application", &apply);
    command_action(ui, "Find skill gaps", "/upskill");
    open_link(ui, "Open job ad", job.url.as_ref());
}

/// A check: its name on the left, what the posting says and the result on the right.
/// The value wraps in the room between them, so a guard's reason or a language note shows in full.
fn check_row(ui: &mut Ui, check: &CheckRow) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = ROW_GAP;
        muted(ui, check.kind.label());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(mark) = CheckMark::of(check.state) {
                check_tag(ui, mark);
            }
            ui.add(egui::Label::new(check.value.as_str()).wrap());
        });
    });
}

fn bullets(ui: &mut Ui, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    strong(ui, title);
    for item in items {
        muted(ui, item);
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::discovery::application::fixtures::{board, view};

    const HIDDEN: &str = "1 job hidden by Settings › Jobs view.";
    const SHOWING: &str = "Showing 1 job your Jobs view hides.";

    fn setup(ctx: &egui::Context) {
        crate::ui_kit::install_fonts(ctx);
        crate::ui_kit::install_theme(ctx);
    }

    fn under_60() -> JobFilters {
        JobFilters { under_score: true, ..JobFilters::default() }
    }

    fn screen(board: JobBoard, filters: JobFilters) -> Harness<'static, JobsScreen> {
        let mut jobs = JobsScreen::default();
        jobs.set_filters(filters);
        let (waiting, kept) = (HashSet::new(), HashSet::new());
        let mut harness = Harness::builder().with_size([1200.0, 800.0]).with_context_setup(setup).build_ui_state(
            move |ui, jobs: &mut JobsScreen| {
                let state = ScreenState { load: Load::Ready, unread: &[], alerts: &|_| {} };
                jobs.show(ui, &state, &board, &waiting, &kept);
            },
            jobs,
        );
        harness.run();
        harness
    }

    fn low_and_top() -> JobBoard {
        board(vec![(Lane::Shortlist, vec![view("top"), JobView { score: Some(40), ..view("low") }])])
    }

    #[test]
    fn no_filter_on_shows_no_line() {
        let harness = screen(low_and_top(), JobFilters::default());
        assert!(harness.query_by_label_contains("Jobs view").is_none());
        harness.get_by_label("Shortlist  2");
        harness.get_by_label("Lead low, Company low, 40");
        harness.get_by_label("Score");
    }

    #[test]
    fn show_them_and_hide_them_toggle_the_line() {
        let mut harness = screen(low_and_top(), under_60());
        harness.get_by_label_contains(HIDDEN);
        harness.get_by_label("Shortlist  1");
        assert!(harness.query_by_label("Lead low, Company low, 40").is_none());

        harness.get_by_label("Show them").click();
        harness.run();
        harness.get_by_label_contains(SHOWING);
        assert!(harness.query_by_label_contains(HIDDEN).is_none());
        harness.get_by_label("Shortlist  2");
        harness.get_by_label("Lead low, Company low, 40");

        harness.get_by_label("Hide them").click();
        harness.run();
        harness.get_by_label_contains(HIDDEN);
        assert!(harness.query_by_label("Lead low, Company low, 40").is_none());
    }

    #[test]
    fn a_list_with_every_job_hidden_shows_only_the_line() {
        let all_low = board(vec![(Lane::Shortlist, vec![JobView { score: Some(40), ..view("low") }])]);
        let harness = screen(all_low, under_60());
        harness.get_by_label_contains(HIDDEN);
        harness.get_by_label("Shortlist  0");
        assert!(harness.query_by_label("No jobs wait here.").is_none(), "the list is not empty, only hidden");
        assert!(harness.query_by_label("Score").is_none(), "no table");
    }

    #[test]
    fn a_selection_the_filters_hide_moves_to_the_first_job_shown() {
        let mut harness = screen(low_and_top(), JobFilters::default());
        harness.get_by_label("Lead low, Company low, 40").click();
        harness.run();
        assert_eq!(harness.state().selected.as_deref(), Some("low"));

        harness.state_mut().set_filters(under_60());
        harness.run();
        assert_eq!(harness.state().selected.as_deref(), Some("top"));
    }
}
