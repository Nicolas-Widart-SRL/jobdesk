use std::collections::HashSet;

use egui::Ui;

use crate::discovery::application::{CheckRow, FitLevel, JobBoard, JobChecks, JobView, Lane, Signal, Verdict, fit_band};
use crate::ui_kit::tokens::{ITEM_GAP, ROW_GAP, ROW_ITEM_GAP, SCREEN_GAP};
use crate::ui_kit::{
    CheckMark, ColumnWidth, Ink, PickTable, Subtitle, TableColumn, Title, Tone, Type, check_tag, command_action, divider, empty_note, empty_state, muted, open_link, page_body, page_head, panel, panel_body,
    paper_with_footer, pick_table, primary_command_action, rich, score, segmented, stamp, stamp_square, strong, tag, title_over, with_side_column,
};

/// What the user is looking at on this screen.
pub struct JobsScreen {
    lane: Lane,
    selected: Option<String>,
}

impl Default for JobsScreen {
    fn default() -> Self {
        Self { lane: Lane::Shortlist, selected: None }
    }
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

fn score_jobs_label(new: usize) -> String {
    match new {
        0 => "Score new jobs…".to_owned(),
        1 => "Score 1 new job…".to_owned(),
        new => format!("Score {new} new jobs…"),
    }
}

impl JobsScreen {
    /// `waiting` holds the key of every job with a card in Today, and `kept` every job whose flagged ad the user kept there.
    pub fn show(&mut self, ui: &mut Ui, board: &JobBoard, waiting: &HashSet<String>, kept: &HashSet<String>) {
        let subtitle = Subtitle::Text("Scored from the job ad alone. Drafting an application also checks the company.");
        let new = board.count(Lane::New);
        page_head(ui, Title::Slab("Jobs"), Some(subtitle), |ui| {
            primary_command_action(ui, &score_jobs_label(new), "/rank");
            command_action(ui, "Find new jobs…", "/scrape");
        });
        page_body(ui, |ui| self.body(ui, board, waiting, kept));
    }

    fn body(&mut self, ui: &mut Ui, board: &JobBoard, waiting: &HashSet<String>, kept: &HashSet<String>) {
        if !lane_shown(self.lane, board) {
            self.lane = Lane::Shortlist;
        }
        let options: Vec<_> = LANES.into_iter().filter(|lane| lane_shown(*lane, board)).map(|lane| (lane, format!("{}  {}", lane_label(lane), board.count(lane)))).collect();
        if let Some(lane) = segmented(ui, self.lane, &options) {
            self.lane = lane;
            self.selected = None;
        }
        ui.add_space(SCREEN_GAP);

        let jobs = board.jobs_in(self.lane);
        if jobs.is_empty() {
            lane_empty(ui, self.lane, board);
            return;
        }
        if !self.selected.as_ref().is_some_and(|key| jobs.iter().any(|job| &job.key == key)) {
            self.selected = jobs.first().map(|job| job.key.clone());
        }
        let selected = jobs.iter().find(|job| Some(&job.key) == self.selected.as_ref());

        with_side_column(
            ui,
            340.0,
            |ui| {
                // The fixed columns overflow a narrow window, so the table scrolls sideways there.
                panel(ui, |ui| {
                    egui::ScrollArea::horizontal().id_salt("jobs_table_x").auto_shrink([false, true]).show(ui, |ui| self.table(ui, jobs, waiting));
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

    fn table(&mut self, ui: &mut Ui, jobs: &[JobView], waiting: &HashSet<String>) {
        // The detail names the site too, so a narrow window drops the column.
        let wide = ui.ctx().content_rect().width() >= FOUND_ON_FROM_WIDTH;
        let shown: Vec<_> = COLUMNS.iter().filter(|(column, _)| wide || *column != JobColumn::FoundOn).collect();
        let heads: Vec<_> = shown.iter().map(|(_, head)| *head).collect();
        let picked = jobs.iter().position(|job| Some(&job.key) == self.selected.as_ref());
        let table = PickTable { id_salt: "jobs_table", columns: &heads, row_height: 58.0, rows: jobs.len(), picked };
        if let Some(row) = pick_table(ui, &table, |row| spoken(&jobs[row]), |ui, row, column| job_cell(ui, &jobs[row], shown[column].0, waiting)) {
            self.selected = Some(jobs[row].key.clone());
        }
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
                    let also: Vec<_> = rest.iter().map(|signal| signal_tag(signal).0).collect();
                    muted(ui, format!("+{}", rest.len())).on_hover_text(format!("Also: {}", also.join(", ")));
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
                command_action(ui, "Find new jobs…", "/scrape");
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
    primary_command_action(ui, "Draft application…", &apply);
    command_action(ui, "Find skill gaps…", "/upskill");
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
