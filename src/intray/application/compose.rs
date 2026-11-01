use std::collections::HashMap;

use chrono::NaiveDate;

use crate::discovery::application::{FreelanceGates, Gate, GateReading, JobBoard, JobView, Lane, Verdict};
use crate::dossier::application::ArchiveContents;
use crate::intray::domain::{Action, Body, Card, CardKind, Check, Due, Effect, GateRow, SkipReason, Subject};
use crate::runs::application::{Prompt, PromptKind, Question, Run, closing_question, command_word, first_line, stop_point, wants_reply};
use crate::shared_kernel::dates::days_from;
use crate::tracking::application::{ApplicationCard, MAX_FOLLOW_UPS, Pipeline, Stage, StatusBucket};

/// Everything cards are derived from. All of it comes from a snapshot except live runs.
pub struct Sources<'a> {
    pub board: &'a JobBoard,
    pub pipeline: &'a Pipeline,
    pub archives: &'a HashMap<String, ArchiveContents>,
    pub runs: &'a [Run],
    pub today: NaiveDate,
}

/// `kept` holds the guard cards the user cleared, so their postings can reach triage.
pub fn derive_cards(sources: &Sources, kept: impl Fn(&str) -> bool) -> Vec<Card> {
    let a_run_works = sources.runs.iter().any(Run::is_busy);
    let mut cards: Vec<Card> = sources.runs.iter().filter_map(|run| paused_card(run).or_else(|| turn_card(run, a_run_works))).collect();
    for lane in [Lane::Shortlist, Lane::BelowThreshold] {
        for job in sources.board.jobs_in(lane) {
            if sources.pipeline.cards().any(|card| is_tracked(card, job)) {
                continue;
            }
            if job.guard_flagged && !kept(&guard_key(job)) {
                cards.push(guard_card(job, sources.today));
            } else if lane == Lane::Shortlist && !has_passed(job) {
                cards.push(triage_card(job));
            }
        }
    }
    cards.extend(sources.pipeline.cards().filter_map(|card| application_card(card, sources.archives, sources.today)));
    cards
}

pub fn guard_key(job: &JobView) -> String {
    format!("guard:{}", job.key)
}

fn is_tracked(card: &ApplicationCard, job: &JobView) -> bool {
    let same_url = card.source.is_some() && card.source == job.url;
    same_url || (card.company.eq_ignore_ascii_case(&job.company) && card.role.eq_ignore_ascii_case(&job.title))
}

fn has_passed(job: &JobView) -> bool {
    job.days_to_deadline.is_some_and(|days| days < 0)
}

fn first_word(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

/// The words of a card dealt for a run. `status` follows the command line in the subtitle.
struct RunWords {
    title: String,
    status: String,
    take: &'static str,
    when: &'static str,
    body: Body,
    note: &'static str,
}

/// A card for a run: it has no default and leads to the run. `words` gets the command word.
fn run_card(run: &Run, kind: CardKind, key: String, words: impl FnOnce(&str) -> RunWords) -> Card {
    let line = first_line(&run.line);
    let command = command_word(&line);
    let words = words(&command);
    Card {
        key,
        kind,
        name: command,
        over: None,
        subject: Subject::Run(run.id.clone()),
        title: words.title,
        subtitle: format!("{line} {}, ${:.2} of ${:.2} spent", words.status, run.cost_usd, run.budget_usd),
        take: words.take.into(),
        when: words.when.into(),
        due: None,
        weight: 0,
        body: words.body,
        primary: None,
        others: vec![Action::new("Open task", Effect::Run(run.id.clone()))],
        can_skip: false,
        note: words.note.into(),
        command: None,
    }
}

fn paused_card(run: &Run) -> Option<Card> {
    let prompt = run.current_prompt()?;
    let (kind, title, take, body, note) = match &prompt.kind {
        PromptKind::Permission { tool, detail, .. } => (
            CardKind::Permission,
            format!("allow {tool}?"),
            "Claude wants to use a tool that is not on your allow list.",
            Body::Code { label: "Claude wants to run".into(), text: detail.clone() },
            "No default here. Read it, then choose.",
        ),
        PromptKind::Questions(questions) => {
            let question = questions.first();
            (
                CardKind::Question,
                question.map_or("a question", |question| question.text.as_str()).to_owned(),
                "Claude needs an answer before the run can go on.",
                question.map_or(Body::Empty, question_options),
                "No default here. Answer it in the run.",
            )
        }
    };
    Some(run_card(run, kind, prompt_key(run, prompt), |command| RunWords {
        title: format!("{command}: {title}"),
        status: "is paused".into(),
        take,
        when: "Run is paused",
        body,
        note,
    }))
}

/// A run that waits for the user's reply, as `wants_reply` says, with no prompt open: a prompt is `paused_card`'s.
/// Only a reply moves it on, so the card has no default. It stays away while another run works, because a reply would be blocked,
/// and once every step is done.
fn turn_card(run: &Run, another_is_busy: bool) -> Option<Card> {
    let rail = run.steps.as_ref();
    if another_is_busy || run.needs_user() || !wants_reply(run) || rail.is_some_and(|rail| rail.is_complete()) {
        return None;
    }
    let stop = stop_point(run);
    let question = closing_question(run);
    let (status, take) = match (stop, rail.and_then(|rail| rail.labels.get(rail.current))) {
        (Some(_), Some(step)) => (format!("stopped after {step}"), "Claude finished a step and waits for your reply."),
        _ => ("is waiting".into(), "Claude asked a question and waits for your reply."),
    };
    let key = format!("turn:{}:{}", run.id, run.current_turn_start());
    Some(run_card(run, CardKind::Turn, key, |command| RunWords {
        title: match stop {
            Some(stop) => format!("{command}: {}?", stop.label),
            None => format!("{command} is waiting for your reply"),
        },
        status,
        take,
        when: "Run is waiting",
        body: question.map_or(Body::Empty, |question| Body::Code { label: "Claude asks".into(), text: question.text }),
        note: "No default here. Reply in the run.",
    }))
}

fn prompt_key(run: &Run, prompt: &Prompt) -> String {
    format!("run:{}:{}", run.id, prompt.request_id)
}

fn question_options(question: &Question) -> Body {
    Body::Options(question.options.iter().map(|option| (option.label.clone(), option.description.clone())).collect())
}

fn check_of(gate: Gate) -> Check {
    match gate {
        Gate::Pass => Check::Pass,
        Gate::Flag => Check::Flag,
        Gate::Fail => Check::Fail,
    }
}

fn gate_row(label: &'static str, reading: &GateReading) -> GateRow {
    GateRow { label, value: reading.value.clone(), check: check_of(reading.gate) }
}

/// The reason to suggest a skip, from the first freelance gate that fails.
fn skip_suggestion(gates: &FreelanceGates) -> Option<(SkipReason, &'static str, &'static str)> {
    if gates.contract.gate == Gate::Fail {
        Some((SkipReason::Scope, "Skip, not freelance", "The posting is not open to freelancers."))
    } else if gates.day_rate.gate == Gate::Fail {
        Some((SkipReason::Rate, "Skip, rate too low", "The stated rate is under your floor."))
    } else if gates.commute.gate == Gate::Fail {
        Some((SkipReason::Location, "Skip, too far", "The commute is over your limit."))
    } else {
        None
    }
}

const WHERE: &str = "Where";
const SAFETY: &str = "Safety check";

fn job_gates(job: &JobView) -> Vec<GateRow> {
    let gates = &job.freelance;
    let work_mode = job.location_gate.map(|gate| GateRow { label: WHERE, value: job.location.clone().unwrap_or_else(|| "Not stated".into()), check: check_of(gate) });
    let language = job.language_gate.map(|gate| GateRow { label: "Language", value: job.language_note.clone().unwrap_or_else(|| "No conflict".into()), check: check_of(gate) });
    let guard = GateRow {
        label: SAFETY,
        value: job.guard.as_deref().map_or_else(|| "Not checked".to_owned(), |guard| if guard.trim() == "clean" { "Nothing hidden in the ad".to_owned() } else { guard.to_owned() }),
        check: if job.guard_flagged { Check::Flag } else if job.guard.is_some() { Check::Pass } else { Check::Flag },
    };
    [Some(gate_row("Contract", &gates.contract)), Some(gate_row("Day rate", &gates.day_rate)), work_mode, Some(gate_row("Commute", &gates.commute)), language, Some(guard)]
        .into_iter()
        .flatten()
        .collect()
}

fn number_word(count: usize) -> &'static str {
    ["No", "One", "Two", "Three", "Four", "Five", "Six"].get(count).copied().unwrap_or("Many")
}

/// What to check first, for the first row that fails, else the first that is unclear.
fn weakest_check(rows: &[GateRow]) -> &'static str {
    let row = rows.iter().find(|row| row.check == Check::Fail).or_else(|| rows.iter().find(|row| row.check == Check::Flag));
    match row.map(|row| (row.label, row.check)) {
        Some(("Contract", Check::Flag)) => "The ad does not say it is freelance, so check before you apply.",
        Some(("Day rate", Check::Flag)) => "The day rate is not in the ad, so check it before you apply.",
        Some(("Commute", Check::Flag)) => "The commute is not clear from the ad, so check it before you apply.",
        Some((WHERE, Check::Flag)) => "The work mode is not clear from the ad, so check it before you apply.",
        Some((WHERE, Check::Fail)) => "The location does not suit you.",
        Some(("Language", Check::Flag)) => "The language needs a look, so check it before you apply.",
        Some(("Language", Check::Fail)) => "The language does not suit you.",
        Some((SAFETY, Check::Flag)) => "The ad has not had a safety check yet.",
        _ => "One check needs a look before you apply.",
    }
}

fn take_on(job: &JobView, rows: &[GateRow]) -> String {
    if let Some((_, _, reason)) = skip_suggestion(&job.freelance) {
        return format!("Claude suggests you skip this. {reason}");
    }
    let passing = rows.iter().filter(|row| row.check == Check::Pass).count();
    if passing == rows.len() {
        return format!("All {} checks pass. Scored from the job ad alone.", number_word(rows.len()).to_lowercase());
    }
    let passes = if passing == 1 { "check passes" } else { "checks pass" };
    format!("{} {passes}. {}", number_word(passing), weakest_check(rows))
}

fn fit_band(verdict: Option<Verdict>, score: u8) -> String {
    let words = match verdict {
        Some(Verdict::Strong) => "Strong fit",
        Some(Verdict::Good) => "Good fit",
        Some(Verdict::Moderate) => "Moderate fit",
        Some(Verdict::Weak) => "Weak fit",
        Some(Verdict::Poor) => "Poor fit",
        None => return format!("Scored {score} of 100."),
    };
    format!("{words}, {score} of 100.")
}

fn apply_line(job: &JobView) -> String {
    job.url.as_ref().map_or_else(|| "/apply".to_owned(), |url| format!("/apply {}", url.as_str()))
}

fn days_text(prefix: &str, days: i64) -> String {
    match days {
        ..0 => format!("{prefix} passed"),
        0 => format!("{prefix} today"),
        1 => format!("{prefix} tomorrow"),
        _ => format!("{prefix} in {days} days"),
    }
}

fn posting_action(job: &JobView) -> Option<Action> {
    job.url.clone().map(|url| Action::new("Open job ad", Effect::Posting(url)))
}

fn triage_card(job: &JobView) -> Card {
    let line = apply_line(job);
    let (primary, mut others, command, note) = match skip_suggestion(&job.freelance) {
        Some((reason, label, _)) => (Action::new(label, Effect::Skip(reason)), vec![Action::new("Draft anyway…", Effect::Command(line))], None, "Skips stay in the app. Nothing runs."),
        None => (Action::new("Draft application…", Effect::Command(line.clone())), vec![], Some(line), "Opens the drafting task. Nothing starts until you press Start."),
    };
    others.extend(posting_action(job));
    let score = job.score.unwrap_or_default();
    let gates = job_gates(job);
    let location = job.location.clone().unwrap_or_else(|| "No location stated".into());
    Card {
        key: format!("triage:{}", job.key),
        kind: CardKind::Triage,
        name: job.company.clone(),
        over: Some(job.company.clone()),
        subject: Subject::Job(job.key.clone()),
        title: job.title.clone(),
        subtitle: job.found_on.as_ref().map_or_else(|| format!("{location}."), |site| format!("{location}. Found on {site}.")),
        take: take_on(job, &gates),
        when: job.days_to_deadline.map_or_else(|| "No deadline".to_owned(), |days| days_text("Closes", days)),
        due: job.deadline.map(|on| Due { on, label: format!("{} closes", job.company) }),
        weight: -i64::from(score),
        body: Body::Triage { score, band: fit_band(job.verdict, score), gates, strengths: job.strengths.clone(), gaps: job.gaps.clone() },
        primary: Some(primary),
        others,
        can_skip: true,
        note: note.into(),
        command,
    }
}

fn guard_card(job: &JobView, today: NaiveDate) -> Card {
    let open_date = job.deadline.filter(|on| *on >= today);
    Card {
        key: guard_key(job),
        kind: CardKind::GuardFlag,
        name: job.company.clone(),
        over: Some(job.company.clone()),
        subject: Subject::Job(job.key.clone()),
        title: job.title.clone(),
        subtitle: "The safety check flagged this ad".into(),
        take: "This ad contains hidden text meant for an AI. Read what was found before you keep it.".into(),
        when: job.score.map_or_else(String::new, |score| format!("Score {score}")),
        due: open_date.map(|on| Due { on, label: format!("{} closes", job.company) }),
        weight: 0,
        body: Body::Code { label: "What the safety check found, shown as plain text".into(), text: job.guard.clone().unwrap_or_default() },
        primary: None,
        others: [Some(Action::new("Harmless, keep posting", Effect::Keep)), posting_action(job)].into_iter().flatten().collect(),
        can_skip: true,
        note: "Nothing runs until you choose.".into(),
        command: None,
    }
}

fn application_key(kind: &str, card: &ApplicationCard) -> String {
    format!("{kind}:{}|{}", card.company.to_lowercase(), card.role.to_lowercase())
}

fn base_card(card: &ApplicationCard, key: String, kind: CardKind) -> Card {
    Card {
        key,
        kind,
        name: card.company.clone(),
        over: Some(card.company.clone()),
        subject: Subject::Application { company: card.company.clone(), role: card.role.clone() },
        title: card.role.clone(),
        subtitle: String::new(),
        take: String::new(),
        when: String::new(),
        due: None,
        weight: 0,
        body: Body::Empty,
        primary: None,
        others: vec![],
        can_skip: false,
        note: String::new(),
        command: None,
    }
}

/// An application needs one decision at a time. The next round outranks chasing a reply.
fn application_card(card: &ApplicationCard, archives: &HashMap<String, ArchiveContents>, today: NaiveDate) -> Option<Card> {
    let archive = card.archive_folder.as_ref().and_then(|folder| archives.get(folder));
    match card.bucket {
        StatusBucket::Drafted => send_card(card, archive?, today),
        StatusBucket::Offer => Some(offer_card(card, today)),
        _ if card.is_final => None,
        _ => {
            if let Some(stage) = next_stage(card, today) {
                return prep_card(card, stage, archive?);
            }
            if card.stale {
                Some(close_card(card))
            } else if card.follow_up_due {
                Some(follow_up_card(card, archive))
            } else {
                None
            }
        }
    }
}

fn send_card(card: &ApplicationCard, archive: &ArchiveContents, today: NaiveDate) -> Option<Card> {
    if archive.documents.is_empty() {
        return None;
    }
    let line = format!("/outcome {}", card.company.to_lowercase());
    let compiled = archive.documents.iter().map(|document| document.label.as_str()).collect::<Vec<_>>().join(" and ");
    let days = card.deadline.map(|on| days_from(today, on));
    Some(Card {
        subtitle: format!("{compiled} compiled"),
        take: "Send it from your own mail or the portal, then mark it sent.".into(),
        when: days.map_or_else(|| "No deadline".to_owned(), |days| days_text("Deadline", days)),
        due: card.deadline.map(|on| Due { on, label: format!("{} deadline", card.company) }),
        primary: Some(Action::new("Mark as sent…", Effect::Command(line.clone()))),
        note: "Opens the task that records it as sent. The app never sends for you.".into(),
        command: Some(line),
        ..base_card(card, application_key("send", card), CardKind::Send)
    })
}

fn reply_text(on: NaiveDate, today: NaiveDate) -> String {
    if on < today { "Reply date passed".to_owned() } else { format!("Reply by {}", on.format("%-d %b")) }
}

fn offer_card(card: &ApplicationCard, today: NaiveDate) -> Card {
    let mut lines = Vec::new();
    lines.extend(card.fit_rating.map(|fit| ("Fit", fit.to_string())));
    if !card.notes.trim().is_empty() {
        lines.push(("Notes", card.notes.trim().to_owned()));
    }
    Card {
        subtitle: card.fit_rating.map_or_else(|| "Offer received".to_owned(), |fit| format!("Offer received, fit {fit}")),
        take: "Compare it with your other open rounds before you answer.".into(),
        when: card.deadline.map_or_else(|| "No reply date".to_owned(), |on| reply_text(on, today)),
        due: card.deadline.map(|on| Due { on, label: format!("{} offer reply", card.company) }),
        body: Body::Lines(lines),
        primary: card.archive_folder.clone().map(|folder| Action::new("Open dossier", Effect::Dossier(folder))),
        note: "Your decision. Claude only records it.".into(),
        command: Some(format!("/outcome {}", card.company.to_lowercase())),
        ..base_card(card, application_key("offer", card), CardKind::Offer)
    }
}

fn next_stage(card: &ApplicationCard, today: NaiveDate) -> Option<&Stage> {
    let stages = card.outcome.as_ref()?.stages.iter();
    stages.filter(|stage| stage.date.is_some_and(|on| on >= today)).min_by_key(|stage| stage.date)
}

fn prep_card(card: &ApplicationCard, stage: &Stage, archive: &ArchiveContents) -> Option<Card> {
    let on = stage.date?;
    let round = first_word(&stage.name).to_lowercase();
    let pack = archive.prep_packs.iter().find(|pack| pack.name.to_lowercase().contains(&round)).or(archive.prep_packs.first())?;
    let mut lines = vec![("Round", stage.name.clone()), ("Prep pack", pack.name.clone())];
    if !card.contact_person.is_empty() {
        lines.push(("Contact", card.contact_person.clone()));
    }
    Some(Card {
        subtitle: "Prep pack ready".into(),
        take: "Read the pack once before the call.".into(),
        when: on.format("%A %-d %b").to_string(),
        due: Some(Due { on, label: format!("{} interview", card.company) }),
        body: Body::Lines(lines),
        primary: card.archive_folder.clone().map(|folder| Action::new("Open dossier", Effect::Dossier(folder))),
        note: format!("{} prep pack", pack.name),
        command: None,
        ..base_card(card, format!("{}:{}:{on}", application_key("prep", card), stage.name.to_lowercase()), CardKind::Prep)
    })
}

fn follow_up_card(card: &ApplicationCard, archive: Option<&ArchiveContents>) -> Card {
    let line = format!("/outcome followup {}", card.company.to_lowercase());
    let draft = archive.and_then(|archive| archive.follow_ups.iter().max_by_key(|draft| &draft.name));
    let quiet = card.days_quiet.unwrap_or_default();
    let (take, body, primary) = match draft {
        Some(draft) => (
            "Claude drafted a short note. Copy it, send it yourself, then mark it sent.",
            Body::Code { label: format!("Draft of {}", draft.name), text: draft.text.clone() },
            "Copied and sent…",
        ),
        None => ("No draft yet. The command writes one, then logs it when you send it.", Body::Empty, "Draft a follow-up…"),
    };
    Card {
        subtitle: format!("{} of {MAX_FOLLOW_UPS} follow-ups sent", card.follow_ups_sent),
        take: take.into(),
        when: format!("Quiet {quiet} days"),
        weight: -quiet,
        body,
        primary: Some(Action::new(primary, Effect::Command(line.clone()))),
        others: draft.map(|draft| Action::new("Copy draft", Effect::Copy(draft.text.clone()))).into_iter().collect(),
        can_skip: true,
        note: "The app never sends for you.".into(),
        command: Some(line),
        ..base_card(card, format!("{}:{}", application_key("followup", card), card.follow_ups_sent), CardKind::FollowUp)
    }
}

fn close_card(card: &ApplicationCard) -> Card {
    let quiet = card.days_quiet.unwrap_or_default();
    Card {
        subtitle: format!("{} of {MAX_FOLLOW_UPS} follow-ups sent", card.follow_ups_sent),
        take: "No reply for a long time. Close it as no response.".into(),
        when: format!("Quiet {quiet} days"),
        weight: -quiet,
        primary: Some(Action::new("Close as no response…", Effect::Command("/outcome stale".into()))),
        note: format!("Opens the task that closes it. Confirm {} only.", card.company),
        command: Some("/outcome stale".into()),
        ..base_card(card, application_key("close", card), CardKind::Close)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::application::{LaneView, Signal};
    use crate::dossier::application::{NamedText, PdfDocument};
    use crate::runs::application::{AgentEvent, RunRequest};
    use crate::tracking::application::{OutcomeRecord, PipelineLane, ReachedStage};

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
    }

    fn read(gate: Gate) -> GateReading {
        GateReading { gate, value: "x".into() }
    }

    fn gates(contract: Gate, day_rate: Gate, commute: Gate) -> FreelanceGates {
        FreelanceGates { contract: read(contract), day_rate: read(day_rate), commute: read(commute) }
    }

    fn job(company: &str) -> JobView {
        JobView {
            key: company.to_lowercase(), title: "Lead".into(), company: company.into(), location: None, url: None, portal: None, found_on: None,
            deadline: None, days_to_deadline: None, score: Some(80), verdict: None, scraper_fit: None,
            location_gate: None, language_gate: None, language_note: None, freelance: gates(Gate::Pass, Gate::Pass, Gate::Pass),
            guard: Some("clean".into()), strengths: vec![], gaps: vec![], fetch_attempts: None, fetch_error: None,
            signals: Vec::<Signal>::new(), closing_soon: false, guard_flagged: false,
        }
    }

    fn application(bucket: StatusBucket) -> ApplicationCard {
        ApplicationCard {
            company: "Acme Corp".into(), role: "Lead".into(), sector: String::new(), channel: String::new(),
            status_label: String::new(), bucket, applied_on: String::new(), deadline: None, deadline_state: None,
            contact_person: String::new(), fit_rating: None, notes: String::new(), source: None, days_quiet: None,
            follow_ups_sent: 0, follow_up_due: false, stale: false, reached: ReachedStage::Applied,
            is_rejection: false, is_final: false, cv_file: String::new(), cover_letter_file: String::new(),
            archive_folder: Some("acme_corp_lead".into()), outcome: None,
        }
    }

    fn cards(board: &JobBoard, applications: Vec<ApplicationCard>, archive: ArchiveContents, runs: &[Run]) -> Vec<Card> {
        let pipeline = Pipeline { lanes: vec![PipelineLane { bucket: StatusBucket::Active, cards: applications }] };
        let archives = HashMap::from([("acme_corp_lead".to_owned(), archive)]);
        derive_cards(&Sources { board, pipeline: &pipeline, archives: &archives, runs, today: today() }, |_| false)
    }

    fn shortlist(jobs: Vec<JobView>) -> JobBoard {
        JobBoard { lanes: vec![LaneView { lane: Lane::Shortlist, jobs }], total_seen: 1 }
    }

    fn one_app(application: ApplicationCard, archive: ArchiveContents) -> Vec<Card> {
        cards(&JobBoard::default(), vec![application], archive, &[])
    }

    fn compiled() -> ArchiveContents {
        ArchiveContents { documents: vec![PdfDocument { label: "CV".into(), path: "cv.pdf".into(), modified: 0 }], ..Default::default() }
    }

    fn paused(prompt_kind: PromptKind) -> Run {
        let request = RunRequest { run_id: "r1".into(), line: "/apply x".into(), model: None, budget_usd: 3.0, started_at: String::new(), resume_session: None };
        let mut run = Run::new(&request);
        run.apply(AgentEvent::PromptRaised(Prompt { request_id: "p1".into(), kind: prompt_kind, input_json: "{}".into(), suggestions_json: None }));
        run
    }

    #[test]
    fn a_passing_job_drafts_on_y() {
        let job = JobView { url: crate::shared_kernel::WebUrl::parse("https://acme.example/1"), ..job("Acme") };
        let card = &cards(&shortlist(vec![job]), vec![], ArchiveContents::default(), &[])[0];
        assert_eq!(card.primary.as_ref().unwrap().effect, Effect::Command("/apply https://acme.example/1".into()));
        assert_eq!(card.title, "Lead");
        assert_eq!(card.over.as_deref(), Some("Acme"));
        assert_eq!(card.subject, Subject::Job("acme".into()));
    }

    #[test]
    fn a_passing_job_reads_as_a_fit_sentence() {
        let both = JobView { location_gate: Some(Gate::Pass), language_gate: Some(Gate::Pass), verdict: Some(Verdict::Strong), score: Some(84), location: Some("Brussels, hybrid".into()), found_on: Some("LinkedIn".into()), ..job("Acme") };
        let card = &cards(&shortlist(vec![both]), vec![], ArchiveContents::default(), &[])[0];
        assert_eq!(card.take, "All six checks pass. Scored from the job ad alone.");
        assert_eq!(card.subtitle, "Brussels, hybrid. Found on LinkedIn.");
        let Body::Triage { band, gates, .. } = &card.body else { panic!("a triage body") };
        assert_eq!(band, "Strong fit, 84 of 100.");
        assert_eq!(gates.iter().map(|row| row.label).collect::<Vec<_>>(), ["Contract", "Day rate", "Where", "Commute", "Language", "Safety check"]);
        assert_eq!(gates[5].value, "Nothing hidden in the ad");

        let plain = &cards(&shortlist(vec![job("Acme")]), vec![], ArchiveContents::default(), &[])[0];
        assert_eq!(plain.take, "All four checks pass. Scored from the job ad alone.");
        assert_eq!(plain.subtitle, "No location stated.");
        let Body::Triage { band, .. } = &plain.body else { panic!("a triage body") };
        assert_eq!(band, "Scored 80 of 100.");
    }

    #[test]
    fn the_weakest_check_is_named_not_counted() {
        let take = |job: JobView| cards(&shortlist(vec![job]), vec![], ArchiveContents::default(), &[]).remove(0).take;
        let rate = JobView { freelance: gates(Gate::Pass, Gate::Flag, Gate::Pass), location_gate: Some(Gate::Pass), language_gate: Some(Gate::Pass), ..job("Acme") };
        assert_eq!(take(rate), "Five checks pass. The day rate is not in the ad, so check it before you apply.");
        let mode = JobView { location_gate: Some(Gate::Flag), ..job("Acme") };
        assert_eq!(take(mode), "Four checks pass. The work mode is not clear from the ad, so check it before you apply.");
        let language = JobView { language_gate: Some(Gate::Fail), location_gate: Some(Gate::Flag), ..job("Acme") };
        assert_eq!(take(language), "Four checks pass. The language does not suit you.");
        let unchecked = JobView { guard: None, ..job("Acme") };
        assert_eq!(take(unchecked), "Three checks pass. The ad has not had a safety check yet.");
        let one = JobView { freelance: gates(Gate::Flag, Gate::Flag, Gate::Flag), guard: None, ..job("Acme") };
        assert_eq!(take(one), "No checks pass. The ad does not say it is freelance, so check before you apply.");
    }

    #[test]
    fn launcher_actions_end_in_an_ellipsis() {
        let url = crate::shared_kernel::WebUrl::parse("https://acme.example/1");
        let rate = JobView { url: url.clone(), freelance: gates(Gate::Pass, Gate::Fail, Gate::Pass), ..job("Rate") };
        let passing = JobView { url, ..job("Acme") };
        let dealt = cards(&shortlist(vec![rate, passing]), vec![], ArchiveContents::default(), &[]);
        assert_eq!(dealt[0].others[0].label, "Draft anyway…");
        assert_eq!(dealt[0].others[1].label, "Open job ad");
        assert_eq!(dealt[1].primary.as_ref().unwrap().label, "Draft application…");

        let due = ApplicationCard { follow_up_due: true, ..application(StatusBucket::Active) };
        assert_eq!(one_app(due.clone(), ArchiveContents::default())[0].primary.as_ref().unwrap().label, "Draft a follow-up…");
        let stale = ApplicationCard { stale: true, ..due };
        assert_eq!(one_app(stale, ArchiveContents::default())[0].primary.as_ref().unwrap().label, "Close as no response…");
        assert_eq!(one_app(application(StatusBucket::Drafted), compiled())[0].primary.as_ref().unwrap().label, "Mark as sent…");
    }

    #[test]
    fn the_command_is_the_primary_line() {
        let url = crate::shared_kernel::WebUrl::parse("https://acme.example/1");
        let passing = JobView { url, ..job("Acme") };
        let rate = JobView { freelance: gates(Gate::Pass, Gate::Fail, Gate::Pass), ..job("Rate") };
        let dealt = cards(&shortlist(vec![passing, rate]), vec![], ArchiveContents::default(), &[]);
        assert_eq!(dealt[0].command.as_deref(), Some("/apply https://acme.example/1"));
        assert_eq!(dealt[1].command, None, "a skip opens no command");

        assert_eq!(one_app(application(StatusBucket::Drafted), compiled())[0].command.as_deref(), Some("/outcome acme corp"));
        assert_eq!(one_app(application(StatusBucket::Offer), ArchiveContents::default())[0].command.as_deref(), Some("/outcome acme corp"));
        let permission = paused(PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), can_remember: true });
        let paused = &cards(&JobBoard::default(), vec![], ArchiveContents::default(), std::slice::from_ref(&permission))[0];
        assert_eq!((paused.command.as_deref(), paused.over.as_deref(), &paused.subject), (None, None, &Subject::Run("r1".into())));
    }

    #[test]
    fn a_failed_gate_turns_y_into_a_skip_with_its_reason() {
        let rate = JobView { freelance: gates(Gate::Pass, Gate::Fail, Gate::Pass), ..job("Rate") };
        let contract = JobView { freelance: gates(Gate::Fail, Gate::Fail, Gate::Pass), ..job("Contract") };
        let dealt = cards(&shortlist(vec![rate, contract]), vec![], ArchiveContents::default(), &[]);
        assert_eq!(dealt[0].primary.as_ref().unwrap().effect, Effect::Skip(SkipReason::Rate));
        assert_eq!(dealt[1].primary.as_ref().unwrap().effect, Effect::Skip(SkipReason::Scope));
        assert_eq!(dealt[0].others[0].label, "Draft anyway…");
    }

    #[test]
    fn a_stated_flag_never_suggests_a_skip() {
        let unstated = JobView { freelance: gates(Gate::Flag, Gate::Flag, Gate::Pass), ..job("Acme") };
        let card = &cards(&shortlist(vec![unstated]), vec![], ArchiveContents::default(), &[])[0];
        assert_eq!(card.primary.as_ref().unwrap().label, "Draft application…");
        assert_eq!(card.take, "Two checks pass. The ad does not say it is freelance, so check before you apply.");
    }

    #[test]
    fn a_tracked_or_expired_job_is_not_triaged() {
        let expired = JobView { days_to_deadline: Some(-1), ..job("Gone") };
        let tracked = job("Acme Corp");
        let dealt = cards(&shortlist(vec![expired, tracked]), vec![application(StatusBucket::Active)], ArchiveContents::default(), &[]);
        assert!(dealt.is_empty());
    }

    #[test]
    fn a_guard_flag_has_no_default_and_holds_back_triage() {
        let flagged = JobView { guard: Some("flagged: 1 hidden block".into()), guard_flagged: true, ..job("Halden") };
        let dealt = cards(&shortlist(vec![flagged.clone()]), vec![], ArchiveContents::default(), &[]);
        assert_eq!((dealt.len(), dealt[0].kind), (1, CardKind::GuardFlag));
        assert_eq!(dealt[0].primary, None);
        assert!(dealt[0].can_skip);

        let board = shortlist(vec![flagged.clone()]);
        let pipeline = Pipeline::default();
        let archives = HashMap::new();
        let kept = derive_cards(&Sources { board: &board, pipeline: &pipeline, archives: &archives, runs: &[], today: today() }, |key| key == guard_key(&flagged));
        assert_eq!(kept[0].kind, CardKind::Triage);
    }

    #[test]
    fn paused_runs_have_no_default_and_lead_to_the_run() {
        let permission = paused(PromptKind::Permission { tool: "Bash".into(), detail: "lualatex main.tex".into(), can_remember: true });
        let question = paused(PromptKind::Questions(vec![]));
        for run in [permission, question] {
            let card = &cards(&JobBoard::default(), vec![], ArchiveContents::default(), std::slice::from_ref(&run))[0];
            assert_eq!(card.primary, None);
            assert!(!card.can_skip);
            assert_eq!(card.others[0].effect, Effect::Run("r1".into()));
            assert_eq!(card.key, "run:r1:p1");
        }
    }

    /// An `/apply` run that read the evaluation guide, said `message` and ended its turn.
    fn waiting(message: &str) -> Run {
        let request = RunRequest { run_id: "r1".into(), line: "/apply x".into(), model: None, budget_usd: 3.0, started_at: String::new(), resume_session: None };
        let mut run = Run::new(&request);
        run.apply(AgentEvent::ToolStarted { id: "1".into(), name: "Read".into(), summary: "skills/04-job-evaluation.md".into(), nested: false });
        run.apply(AgentEvent::ToolFinished { id: "1".into(), outcome: Ok(()) });
        run.apply(AgentEvent::AssistantText { text: message.into(), nested: false });
        end_turn(&mut run, 0.94);
        run
    }

    fn end_turn(run: &mut Run, cost_usd: f64) {
        run.apply(AgentEvent::TurnEnded { failed: false, message: None, cost_usd, duration_ms: 1 });
    }

    fn turn_card_of(run: &Run) -> Option<Card> {
        cards(&JobBoard::default(), vec![], ArchiveContents::default(), std::slice::from_ref(run)).into_iter().next()
    }

    #[test]
    fn a_waiting_run_deals_a_your_turn_card_with_no_default() {
        let run = waiting("Fit 78.\n\nShall I proceed with drafting?");
        let card = turn_card_of(&run).unwrap();
        assert_eq!(card.kind, CardKind::Turn);
        assert_eq!(card.title, "/apply: proceed with drafting?");
        assert_eq!(card.subtitle, "/apply x stopped after Evaluate, $0.94 of $3.00 spent");
        assert_eq!(card.body, Body::Code { label: "Claude asks".into(), text: "Shall I proceed with drafting?".into() });
        assert_eq!(card.key, "turn:r1:1");
        assert_eq!(card.primary, None);
        assert!(!card.can_skip);
        assert_eq!(card.others, [Action::new("Open run", Effect::Run("r1".into()))]);
    }

    fn run_of(id: &str, line: &str) -> Run {
        Run::new(&RunRequest { run_id: id.into(), line: line.into(), model: None, budget_usd: 2.0, started_at: String::new(), resume_session: None })
    }

    fn says(run: &mut Run, text: &str) {
        run.apply(AgentEvent::AssistantText { text: text.into(), nested: false });
    }

    #[test]
    fn a_question_without_a_stop_point_deals_the_waiting_form() {
        let mut run = run_of("r2", "/rank");
        says(&mut run, "Rank all postings, or only the shortlist?");
        end_turn(&mut run, 0.5);
        let card = turn_card_of(&run).unwrap();
        assert_eq!(card.title, "/rank is waiting for your reply");
        assert_eq!(card.subtitle, "/rank is waiting, $0.50 of $2.00 spent");
        assert_eq!(card.take, "Claude asked a question and waits for your reply.");
        assert_eq!(card.body, Body::Code { label: "Claude asks".into(), text: "Rank all postings, or only the shortlist?".into() });
    }

    #[test]
    fn a_stop_point_says_a_step_finished() {
        let card = turn_card_of(&waiting("Shall I proceed with drafting?")).unwrap();
        assert_eq!(card.take, "Claude finished a step and waits for your reply.");
    }

    #[test]
    fn a_finished_run_with_nothing_to_answer_deals_no_card() {
        let mut rank = run_of("r2", "/rank");
        says(&mut rank, "Ranked 12 postings.");
        end_turn(&mut rank, 0.5);
        assert!(turn_card_of(&rank).is_none());

        let mut apply = run_of("r3", "/apply x");
        apply.apply(AgentEvent::ToolStarted { id: "1".into(), name: "Edit".into(), summary: "job_search_tracker.csv".into(), nested: false });
        says(&mut apply, "Drafted and tracked.");
        end_turn(&mut apply, 0.5);
        assert!(apply.steps.as_ref().unwrap().is_complete());
        assert!(turn_card_of(&apply).is_none());
    }

    #[test]
    fn a_question_after_the_last_step_deals_no_card() {
        let mut apply = run_of("r3", "/apply x");
        apply.apply(AgentEvent::ToolStarted { id: "1".into(), name: "Edit".into(), summary: "job_search_tracker.csv".into(), nested: false });
        says(&mut apply, "Anything else?");
        end_turn(&mut apply, 0.5);
        assert!(turn_card_of(&apply).is_none());
    }

    #[test]
    fn no_turn_card_while_another_run_works() {
        let waiting = waiting("Shall I proceed with drafting?");
        let working = run_of("r2", "/rank");
        assert!(working.is_busy());
        let dealt = cards(&JobBoard::default(), vec![], ArchiveContents::default(), &[waiting, working]);
        assert!(dealt.is_empty());
    }

    #[test]
    fn a_turn_claude_starts_on_its_own_deals_a_new_key() {
        let mut run = waiting("Shall I proceed with drafting?");
        let first = turn_card_of(&run).unwrap().key;
        // The user sets the card aside. A background agent reports, and Claude asks again without being replied to.
        run.apply(AgentEvent::SessionStarted { session_id: "s".into(), model: "m".into(), api_key_source: "none".into() });
        says(&mut run, "The reviewer is done. Shall I proceed with drafting now?");
        end_turn(&mut run, 1.2);
        let second = turn_card_of(&run).unwrap();
        assert_ne!(second.key, first);
        assert_eq!(second.body, Body::Code { label: "Claude asks".into(), text: "The reviewer is done. Shall I proceed with drafting now?".into() });
    }

    #[test]
    fn a_reply_ends_the_turn_card() {
        let mut run = waiting("Shall I proceed with drafting?");
        assert!(turn_card_of(&run).is_some());
        run.note_reply("Yes, proceed with drafting.");
        assert!(turn_card_of(&run).is_none());
    }

    #[test]
    fn a_later_stop_deals_a_new_key() {
        let mut run = waiting("Shall I proceed with drafting?");
        let first = turn_card_of(&run).unwrap().key;
        run.note_reply("Yes, proceed with drafting.");
        says(&mut run, "The draft is ready. Should the cover letter be shorter?");
        end_turn(&mut run, 1.5);
        assert_ne!(turn_card_of(&run).unwrap().key, first);
    }

    #[test]
    fn a_prompt_deals_its_own_card_and_not_a_turn_card() {
        let mut run = waiting("Shall I proceed with drafting?");
        run.apply(AgentEvent::PromptRaised(Prompt { request_id: "p1".into(), kind: PromptKind::Questions(vec![]), input_json: "{}".into(), suggestions_json: None }));
        let dealt = cards(&JobBoard::default(), vec![], ArchiveContents::default(), std::slice::from_ref(&run));
        assert_eq!(dealt.iter().map(|card| card.kind).collect::<Vec<_>>(), [CardKind::Question]);
        assert!(turn_card(&run, false).is_none(), "a pending prompt never deals a turn card, though wants_reply holds");
    }

    #[test]
    fn a_stop_part_way_that_asks_nothing_deals_no_card() {
        let mut run = run_of("r3", "/apply x");
        says(&mut run, "The posting URL returned 404, so I could not fetch it.");
        end_turn(&mut run, 0.2);
        assert!(!run.steps.as_ref().unwrap().is_complete());
        assert!(turn_card(&run, false).is_none());
    }

    #[test]
    fn a_draft_needs_compiled_pdfs_to_be_sent() {
        let drafted = application(StatusBucket::Drafted);
        assert!(one_app(drafted.clone(), ArchiveContents::default()).is_empty());
        let card = &one_app(drafted, compiled())[0];
        assert_eq!((card.kind, card.subtitle.as_str()), (CardKind::Send, "CV compiled"));
        assert_eq!(card.primary.as_ref().unwrap().effect, Effect::Command("/outcome acme corp".into()));
    }

    #[test]
    fn an_offer_opens_the_dossier_and_keeps_its_reply_date() {
        let on = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        let card = &one_app(ApplicationCard { deadline: Some(on), ..application(StatusBucket::Offer) }, ArchiveContents::default())[0];
        assert_eq!(card.primary.as_ref().unwrap().effect, Effect::Dossier("acme_corp_lead".into()));
        assert_eq!(card.when, "Reply by 6 Nov");
        assert_eq!(card.due.as_ref().unwrap().label, "Acme Corp offer reply");
    }

    #[test]
    fn a_passed_reply_date_reads_as_passed() {
        let on = today() - chrono::Days::new(1);
        let card = &one_app(ApplicationCard { deadline: Some(on), ..application(StatusBucket::Offer) }, ArchiveContents::default())[0];
        assert_eq!(card.when, "Reply date passed");
        let undated = &one_app(application(StatusBucket::Offer), ArchiveContents::default())[0];
        assert_eq!(undated.when, "No reply date");
    }

    #[test]
    fn a_dated_round_with_a_pack_is_prep_and_outranks_chasing() {
        let stage = Stage { name: "Technical interview".into(), reached: false, date: NaiveDate::from_ymd_opt(2026, 11, 4) };
        let outcome = OutcomeRecord { stages: vec![stage], ..Default::default() };
        let waiting = ApplicationCard { outcome: Some(outcome), follow_up_due: true, ..application(StatusBucket::Interview) };
        let pack = NamedText { name: "interview_prep_technical.md".into(), text: "x".into() };

        let card = &one_app(waiting.clone(), ArchiveContents { prep_packs: vec![pack], ..Default::default() })[0];
        assert_eq!(card.kind, CardKind::Prep);
        assert_eq!(card.when, "Wednesday 4 Nov");
        assert_eq!(card.due.as_ref().unwrap().label, "Acme Corp interview");

        assert!(one_app(waiting, ArchiveContents::default()).is_empty());
    }

    #[test]
    fn a_follow_up_card_changes_key_with_each_one_sent() {
        let due = ApplicationCard { follow_up_due: true, days_quiet: Some(12), ..application(StatusBucket::Active) };
        let first = one_app(due.clone(), ArchiveContents::default());
        let second = one_app(ApplicationCard { follow_ups_sent: 1, ..due }, ArchiveContents::default());
        assert_eq!(first[0].primary.as_ref().unwrap().label, "Draft a follow-up…");
        assert_ne!(first[0].key, second[0].key);
    }

    #[test]
    fn a_follow_up_offers_the_newest_draft() {
        let due = ApplicationCard { follow_up_due: true, ..application(StatusBucket::Active) };
        let draft = |name: &str| NamedText { name: name.into(), text: name.into() };
        let card = &one_app(due, ArchiveContents { follow_ups: vec![draft("followup_2026-10-31.md"), draft("followup_2026-10-01.md")], ..Default::default() })[0];
        assert_eq!(card.others[0].effect, Effect::Copy("followup_2026-10-31.md".into()));
        assert_eq!(card.primary.as_ref().unwrap().label, "Copied and sent…");
    }

    #[test]
    fn stale_closes_instead_of_chasing() {
        let stale = ApplicationCard { stale: true, follow_up_due: true, days_quiet: Some(64), ..application(StatusBucket::Active) };
        let card = &one_app(stale, ArchiveContents::default())[0];
        assert_eq!(card.kind, CardKind::Close);
        assert_eq!(card.primary.as_ref().unwrap().effect, Effect::Command("/outcome stale".into()));
    }

    #[test]
    fn a_final_application_needs_nothing() {
        let closed = ApplicationCard { is_final: true, stale: true, ..application(StatusBucket::Closed) };
        assert!(one_app(closed, ArchiveContents::default()).is_empty());
    }
}
