//! Exercises the real `claude` CLI. Costs a few cents, so it runs only on request:
//! `cargo test --test live_claude -- --ignored --nocapture`

use std::path::Path;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use jobdesk::runs;
use jobdesk::runs::application::{ActivityKind, Decision, PromptKind, RunDesk, RunStatus, StartOptions, TaskOutcome, ToolState};

const LIMIT: Duration = Duration::from_secs(120);

fn options() -> StartOptions {
    StartOptions { model: Some("haiku".into()), budget_usd: 1.0 }
}

fn wait(desk: &mut RunDesk, id: &str, mut done: impl FnMut(&mut RunDesk) -> bool) {
    let start = Instant::now();
    while !done(desk) {
        assert!(start.elapsed() < LIMIT, "timed out: {:#?}", desk.run(id).map(|run| (&run.status, run.activity.last())));
        sleep(Duration::from_millis(100));
        desk.poll();
    }
}

fn dump(desk: &RunDesk, id: &str) {
    let run = desk.run(id).unwrap();
    println!("status {:?} cost {} session {:?} model {:?}", run.status, run.cost_usd, run.session_id, run.model);
    for item in &run.activity {
        println!("  {:?}", item.kind);
    }
}

fn desk(workspace: &Path, logs: &Path) -> RunDesk {
    runs::open_desk(workspace, logs.to_owned(), Arc::new(|| {}), Arc::new(|| Ok(None)))
}

#[test]
#[ignore = "calls the real Claude CLI"]
fn a_full_session_works() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("w");
    let logs = dir.path().join("logs");
    std::fs::create_dir(&workspace).unwrap();
    let mut desk = desk(&workspace, &logs);

    let id = desk
        .start(
            "Use AskUserQuestion to ask yes or no about colours. Then use Bash to run exactly: touch probe.txt . Then stop and say DONE.",
            &options(),
            chrono::Local::now().naive_local(),
        )
        .unwrap();

    // Question, then permission with a remembered rule, then a stop point.
    let (mut asked, mut allowed) = (false, false);
    wait(&mut desk, &id, |desk| {
        let run = desk.run(&id).unwrap();
        if let Some(prompt) = run.current_prompt().cloned() {
            match &prompt.kind {
                PromptKind::Questions(questions) => {
                    asked = true;
                    let answers = questions.iter().map(|q| (q.text.clone(), q.options[0].label.clone())).collect();
                    desk.decide(&id, &prompt.request_id, Decision::Answer(answers));
                }
                PromptKind::Permission { tool, can_remember, .. } => {
                    println!("permission for {tool}, can remember {can_remember}");
                    allowed = true;
                    desk.decide(&id, &prompt.request_id, Decision::Allow { remember: *can_remember });
                }
            }
            return false;
        }
        matches!(run.status, RunStatus::AwaitingReply | RunStatus::Failed(_))
    });
    dump(&desk, &id);
    assert_eq!(desk.run(&id).unwrap().status, RunStatus::AwaitingReply);
    assert!(asked && allowed);
    assert!(workspace.join("probe.txt").exists());
    assert!(!workspace.join(".claude/settings.local.json").exists(), "a remembered rule must not reach the checkout");

    // Reply on the live process.
    desk.reply(&id, "Reply with the single word SECOND.").unwrap();
    wait(&mut desk, &id, |desk| desk.run(&id).unwrap().status == RunStatus::AwaitingReply);

    // Resume in a new process after ending the session.
    desk.close(&id);
    assert!(!desk.has_live_process());
    desk.reply(&id, "Reply with the single word THIRD.").unwrap();
    wait(&mut desk, &id, |desk| desk.run(&id).unwrap().status == RunStatus::AwaitingReply);
    dump(&desk, &id);
    let last = desk.run(&id).unwrap().activity.iter().rev().find_map(|item| match &item.kind {
        ActivityKind::Claude(text) => Some(text.clone()),
        _ => None,
    });
    assert!(last.unwrap().contains("THIRD"));
    let cost_usd = desk.run(&id).unwrap().cost_usd;
    desk.close(&id);

    // History rebuilds from the log.
    let restored = runs::open_desk(&workspace, logs.clone(), Arc::new(|| {}), Arc::new(|| Ok(None)));
    let run = &restored.runs()[0];
    assert_eq!(run.status, RunStatus::Closed);
    assert!(run.session_id.is_some() && run.cost_usd > 0.0);
    assert!(run.activity.iter().any(|item| matches!(&item.kind, ActivityKind::You(text) if text.contains("THIRD"))));

    // Each process's total starts again from zero, so usage must add the processes, not take the last total.
    let log = std::fs::read_to_string(logs.join(format!("{id}.jsonl"))).unwrap();
    let resume_at = log.lines().position(|line| line == r#"{"type":"jobdesk_resume"}"#).expect("a resume marker");
    let result_cost = |line: &str| {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        (value["type"] == "result").then(|| value["total_cost_usd"].as_f64()).flatten()
    };
    let before = log.lines().take(resume_at).filter_map(result_cost).last().expect("a result before the resume");
    let after = log.lines().skip(resume_at).find_map(result_cost).expect("a result after the resume");
    assert!(after < before, "the resumed process reports its own total: {after} after {before}");

    // Usage adds up both processes from the log.
    let tasks = runs::usage_logs(logs, &workspace).tasks();
    println!("usage {tasks:?}");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].outcome, TaskOutcome::Finished);
    assert!((tasks[0].cost_usd - cost_usd).abs() < 1e-9, "{} against {cost_usd}", tasks[0].cost_usd);
    assert!(tasks[0].models.iter().map(|model| model.tokens_in).sum::<u64>() > 0);
    let by_model: f64 = tasks[0].models.iter().map(|model| model.cost_usd).sum();
    assert!((by_model - tasks[0].cost_usd).abs() < 1e-6, "{by_model} against {}", tasks[0].cost_usd);
}

#[test]
#[ignore = "calls the real Claude CLI"]
fn stop_interrupts_a_running_tool() {
    let dir = tempfile::tempdir().unwrap();
    let mut desk = desk(dir.path(), &dir.path().join("logs"));
    let id = desk.start("Use Bash to run exactly: python3 -c 'import time; time.sleep(60)'", &options(), chrono::Local::now().naive_local()).unwrap();

    wait(&mut desk, &id, |desk| {
        if let Some(prompt) = desk.run(&id).unwrap().current_prompt().cloned() {
            desk.decide(&id, &prompt.request_id, Decision::Allow { remember: false });
        }
        desk.run(&id).unwrap().activity.iter().any(|item| matches!(item.kind, ActivityKind::Tool { state: ToolState::Running, .. }))
    });
    sleep(Duration::from_secs(2));
    let stopped_at = Instant::now();
    desk.stop(&id);
    wait(&mut desk, &id, |desk| desk.run(&id).unwrap().status == RunStatus::Stopped);
    println!("stopped in {:?}", stopped_at.elapsed());
    dump(&desk, &id);
    assert!(stopped_at.elapsed() < Duration::from_secs(20));
}

/// A run may edit a profile file under `.claude/`, as `/setup --section` does from the Profile screen.
#[test]
#[ignore = "calls the real Claude CLI"]
fn a_run_can_edit_a_profile_file_under_dot_claude() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("w");
    let profile = workspace.join(".claude/skills/job-application-assistant/01-candidate-profile.md");
    std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
    std::fs::write(&profile, "Name: Sam\n").unwrap();
    let mut desk = desk(&workspace, &dir.path().join("logs"));

    let id = desk
        .start(
            "Use the Edit tool to change \"Name: Sam\" to \"Name: Alex\" in .claude/skills/job-application-assistant/01-candidate-profile.md. Then say DONE.",
            &options(),
            chrono::Local::now().naive_local(),
        )
        .unwrap();

    wait(&mut desk, &id, |desk| {
        let run = desk.run(&id).unwrap();
        if let Some(prompt) = run.current_prompt().cloned() {
            let decision = match &prompt.kind {
                PromptKind::Questions(questions) => Decision::Answer(questions.iter().map(|q| (q.text.clone(), q.options[0].label.clone())).collect()),
                PromptKind::Permission { tool, .. } => {
                    println!("permission for {tool}");
                    Decision::Allow { remember: false }
                }
            };
            desk.decide(&id, &prompt.request_id, decision);
            return false;
        }
        matches!(run.status, RunStatus::AwaitingReply | RunStatus::Failed(_))
    });
    dump(&desk, &id);
    assert_eq!(desk.run(&id).unwrap().status, RunStatus::AwaitingReply);
    assert_eq!(std::fs::read_to_string(&profile).unwrap().trim_end(), "Name: Alex");
    desk.close(&id);
}

/// Test connection on the Claude sign-in: one `claude -p` with no tools, capped at a cent, in an empty temporary folder.
#[test]
#[ignore = "calls the real Claude CLI"]
fn test_connection_passes_on_the_claude_sign_in() {
    use jobdesk::settings;
    use jobdesk::settings::application::{Outcome, Settings, SignIn};

    let mut providers = settings::open_providers(settings::memory_keychain(None), settings::claude_tester(Arc::new(|| {})));
    let mut preferences = Settings::unsaved();
    providers.test_sign_in(false).unwrap();
    let start = Instant::now();
    while !providers.poll(&mut preferences, chrono::Local::now().naive_local()) {
        assert!(start.elapsed() < LIMIT, "the test never ended");
        sleep(Duration::from_millis(100));
    }
    let check = providers.sign_in_check().expect("a sign-in result");
    println!("outcome {:?} cost {} source {:?} said {:?}", check.outcome, check.cost_usd, check.source, check.said);
    assert_eq!(check.outcome, Outcome::Passed, "{}", check.said);
    assert!(check.cost_usd < 0.02, "capped at a cent: {}", check.cost_usd);
    assert_eq!(preferences.preferences().sign_in, SignIn::ClaudeSignIn, "a sign-in test changes no method");
}
