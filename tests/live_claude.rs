//! Exercises the real `claude` CLI. Costs a few cents, so it runs only on request:
//! `cargo test --test live_claude -- --ignored --nocapture`

use std::path::Path;
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use jobdesk::runs;
use jobdesk::runs::application::{ActivityKind, Decision, PromptKind, RunDesk, RunStatus, StartOptions, ToolState};

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
    runs::open_desk(workspace, logs.to_owned(), Arc::new(|| {}))
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
        .start("Use AskUserQuestion to ask yes or no about colours. Then use Bash to run exactly: touch probe.txt . Then stop and say DONE.", &options(), chrono::Local::now().naive_local())
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
    desk.close(&id);

    // History rebuilds from the log.
    let restored = runs::open_desk(&workspace, logs, Arc::new(|| {}));
    let run = &restored.runs()[0];
    assert_eq!(run.status, RunStatus::Closed);
    assert!(run.session_id.is_some() && run.cost_usd > 0.0);
    assert!(run.activity.iter().any(|item| matches!(&item.kind, ActivityKind::You(text) if text.contains("THIRD"))));
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
