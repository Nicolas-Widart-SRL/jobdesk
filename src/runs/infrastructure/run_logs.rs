use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::runs::domain::{AgentEvent, ModelCost, Run, RunHistory, RunRequest, RunStatus, TaskCost, TaskOutcome, UsageLogs};
use crate::runs::infrastructure::stream_json::{ProcessTotals, parse_line, process_totals};
use crate::shared_kernel::visible_text::visible_chars;

const META: &str = "jobdesk_meta";
const USER: &str = "jobdesk_user";
/// A new `claude` process took the run up again, so its running totals start from zero.
const RESUME_LINE: &str = r#"{"type":"jobdesk_resume"}"#;
/// The user asked the run to stop. The CLI writes nothing of its own for that.
const STOP_LINE: &str = r#"{"type":"jobdesk_stop"}"#;
/// How every `UserLine` starts, since its fields serialize in order.
const USER_LINE_START: &str = r#"{"type":"jobdesk_user","#;

#[derive(Serialize, Deserialize)]
struct Meta {
    r#type: String,
    workspace: String,
    id: String,
    line: String,
    model: Option<String>,
    budget_usd: f64,
    started_at: String,
}

#[derive(Serialize, Deserialize)]
struct UserLine {
    r#type: String,
    text: String,
}

/// One append-only file per run in the app's data folder, holding the raw stream.
pub struct RunLog {
    file: File,
}

impl RunLog {
    /// A resumed run keeps writing to the file it started.
    pub fn open(dir: &Path, workspace: &Path, request: &RunRequest) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.jsonl", request.run_id));
        let is_new = !path.exists();
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut log = Self { file: options.open(path)? };
        if is_new {
            let meta = Meta {
                r#type: META.to_owned(),
                workspace: workspace.display().to_string(),
                id: request.run_id.clone(),
                line: request.line.clone(),
                model: request.model.clone(),
                budget_usd: request.budget_usd,
                started_at: request.started_at.clone(),
            };
            log.append(&serde_json::to_string(&meta).unwrap_or_default());
        } else {
            log.append(RESUME_LINE);
            log.append_user(&request.line);
        }
        Ok(log)
    }

    pub fn note_stop(&mut self) {
        self.append(STOP_LINE);
    }

    pub fn append(&mut self, line: &str) {
        let _ = writeln!(self.file, "{line}");
    }

    pub fn append_user(&mut self, text: &str) {
        let line = UserLine { r#type: USER.to_owned(), text: text.to_owned() };
        self.append(&serde_json::to_string(&line).unwrap_or_default());
    }
}

/// Past runs of one workspace, rebuilt from their logs.
pub struct RunLogs {
    dir: PathBuf,
    workspace: String,
}

impl RunLogs {
    pub fn new(dir: PathBuf, workspace: &Path) -> Self {
        Self { dir, workspace: workspace.display().to_string() }
    }

    fn replay(&self, path: &Path) -> Option<Run> {
        let mut lines = BufReader::new(File::open(path).ok()?).lines().map_while(Result::ok);
        let meta: Meta = serde_json::from_str(&lines.next()?).ok()?;
        if meta.r#type != META || meta.workspace != self.workspace {
            return None;
        }
        let mut run = Run::new(&RunRequest {
            run_id: meta.id,
            line: meta.line,
            model: meta.model,
            budget_usd: meta.budget_usd,
            started_at: meta.started_at,
            resume_session: None,
        });
        for line in lines {
            if line == STOP_LINE {
                run.note_stop_requested();
                continue;
            }
            match serde_json::from_str::<UserLine>(&line) {
                Ok(user) if user.r#type == USER => run.note_reply(&user.text),
                _ => parse_line(&line).into_iter().for_each(|event| run.apply(event)),
            }
        }
        // The process ended while it was stopping, so the stop went through.
        if run.status == RunStatus::Stopping {
            run.apply(AgentEvent::ProcessExited { failure: None });
        }
        run.settle_after_restore();
        Some(run)
    }
}

impl RunHistory for RunLogs {
    fn recent(&self, limit: usize) -> Vec<Run> {
        log_paths(&self.dir).iter().rev().filter_map(|path| self.replay(path)).take(limit).collect()
    }
}

fn log_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|ext| ext == "jsonl")).collect();
    paths.sort();
    paths
}

/// What every past task of one workspace cost, read from all of its logs.
/// A log is read again only when its size or modification time changes.
pub struct RunLogUsage {
    dir: PathBuf,
    workspace: String,
    cache: HashMap<PathBuf, (u64, SystemTime, Option<TaskCost>)>,
}

impl RunLogUsage {
    pub fn new(dir: PathBuf, workspace: &Path) -> Self {
        Self { dir, workspace: workspace.display().to_string(), cache: HashMap::new() }
    }

    /// The task one log records. None for another workspace's log, or one with no result and no stop:
    /// that task is still running or never ran.
    /// A line that is not UTF-8 ends the log there, as it does in replay.
    fn read(&self, path: &Path) -> io::Result<Option<TaskCost>> {
        let mut lines = BufReader::new(File::open(path)?).lines().map_while(Result::ok);
        let Some(first) = lines.next() else { return Ok(None) };
        let Ok(meta) = serde_json::from_str::<Meta>(&first) else { return Ok(None) };
        if meta.r#type != META || meta.workspace != self.workspace {
            return Ok(None);
        }
        let Some(started_on) = meta.started_at.get(..10).and_then(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()) else { return Ok(None) };
        // The last result of each process, since its figures are running totals.
        let mut ended: Vec<ProcessTotals> = Vec::new();
        let mut current: Option<ProcessTotals> = None;
        let (mut stopped, mut ever_stopped, mut last_failed) = (false, false, None);
        for line in lines {
            if line == RESUME_LINE {
                ended.extend(current.take());
                stopped = false;
            } else if line.starts_with(USER_LINE_START) {
                // A reply after a stop takes the task up again, in the same process or a resumed one.
                stopped = false;
            } else if line == STOP_LINE {
                (stopped, ever_stopped) = (true, true);
            } else if line.contains("\"result\"")
                && let Some(totals) = process_totals(&line)
            {
                // A log written before resume markers shows a new process by its total starting again.
                if current.as_ref().is_some_and(|previous| totals.cost_usd < previous.cost_usd) {
                    ended.extend(current.take());
                }
                last_failed = Some(totals.failed);
                current = Some(totals);
            }
        }
        if last_failed.is_none() && !ever_stopped {
            return Ok(None);
        }
        let outcome = match (stopped, last_failed) {
            (true, _) => TaskOutcome::Stopped,
            (false, Some(true)) => TaskOutcome::Failed,
            _ => TaskOutcome::Finished,
        };
        let processes: Vec<ProcessTotals> = ended.into_iter().chain(current).collect();
        Ok(Some(TaskCost {
            started_on,
            command: command_of(&meta.line),
            outcome,
            cost_usd: processes.iter().map(|process| process.cost_usd).sum(),
            models: models_by_id(processes.into_iter().flat_map(|process| process.models)),
        }))
    }
}

/// The slash command a task line starts with, if it starts with one. A path such as `/Users/x/cv.pdf` is not a command.
fn command_of(line: &str) -> Option<String> {
    let word: String = visible_chars(line.split_whitespace().next()?).collect();
    let name = word.strip_prefix('/')?;
    let shaped = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'));
    shaped.then_some(word)
}

/// One entry per model id, in the order each first appears, with its figures added up.
fn models_by_id(models: impl Iterator<Item = ModelCost>) -> Vec<ModelCost> {
    let mut summed: Vec<ModelCost> = Vec::new();
    for model in models {
        match summed.iter_mut().find(|known| known.model == model.model) {
            Some(known) => {
                known.tokens_in += model.tokens_in;
                known.tokens_out += model.tokens_out;
                known.cache_read += model.cache_read;
                known.cost_usd += model.cost_usd;
            }
            None => summed.push(model),
        }
    }
    summed
}

impl UsageLogs for RunLogUsage {
    /// Every task in the folder, with no limit on how many.
    fn tasks(&mut self) -> Vec<TaskCost> {
        let paths = log_paths(&self.dir);
        let present: HashSet<&PathBuf> = paths.iter().collect();
        self.cache.retain(|path, _| present.contains(path));
        let mut tasks = Vec::new();
        for path in paths {
            let Ok(metadata) = fs::metadata(&path) else { continue };
            let (len, modified) = (metadata.len(), metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            let task = match self.cache.get(&path) {
                Some((known_len, known_modified, task)) if *known_len == len && *known_modified == modified => task.clone(),
                _ => {
                    // An unreadable file is skipped, and tried again next time.
                    let Ok(task) = self.read(&path) else { continue };
                    self.cache.insert(path, (len, modified, task.clone()));
                    task
                }
            };
            tasks.extend(task);
        }
        tasks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str, line: &str) -> RunRequest {
        RunRequest { run_id: id.into(), line: line.into(), model: None, budget_usd: 2.0, started_at: "2026-10-31 10:00".into(), resume_session: None }
    }

    #[test]
    fn replays_a_run_for_its_own_workspace_only() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Path::new("/w");
        let mut log = RunLog::open(dir.path(), workspace, &request("a", "/rank")).unwrap();
        log.append(r#"{"type":"system","subtype":"init","session_id":"s","model":"m"}"#);
        log.append(r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done"}]}}"#);
        log.append(r#"{"type":"result","subtype":"success","total_cost_usd":0.5,"duration_ms":3}"#);
        drop(log);
        RunLog::open(dir.path(), workspace, &request("a", "again")).unwrap();

        let runs = RunLogs::new(dir.path().to_owned(), workspace).recent(10);

        assert_eq!(runs[0].session_id.as_deref(), Some("s"));
        assert_eq!(runs[0].status, RunStatus::Closed);
        assert_eq!(runs[0].cost_usd, 0.5);
        assert_eq!(runs[0].activity.len(), 3);
        assert!(RunLogs::new(dir.path().to_owned(), Path::new("/other")).recent(10).is_empty());
    }

    fn result(cost: f64) -> String {
        format!(r#"{{"type":"result","subtype":"success","total_cost_usd":{cost},"duration_ms":3}}"#)
    }

    fn result_with(cost: f64, models: &[(&str, u64, f64)]) -> String {
        let usage: Vec<String> = models
            .iter()
            .map(|(model, tokens, cost)| {
                format!(r#""{model}":{{"inputTokens":{tokens},"outputTokens":2,"cacheReadInputTokens":3,"cacheCreationInputTokens":4,"costUSD":{cost}}}"#)
            })
            .collect();
        format!(r#"{{"type":"result","subtype":"success","total_cost_usd":{cost},"modelUsage":{{{}}}}}"#, usage.join(","))
    }

    /// Writes a log as the gateway does: `RunLog::open` once per process, then the stream.
    fn write(dir: &Path, workspace: &str, id: &str, processes: &[&[&str]]) {
        for (at, lines) in processes.iter().enumerate() {
            let line = if at == 0 { "/rank today" } else { "go on" };
            let mut log = RunLog::open(dir, Path::new(workspace), &request(id, line)).unwrap();
            for line in *lines {
                match *line {
                    "stop" => log.note_stop(),
                    "reply" => log.append_user("go on"),
                    line => log.append(line),
                }
            }
        }
    }

    fn tasks(dir: &Path) -> Vec<TaskCost> {
        RunLogUsage::new(dir.to_owned(), Path::new("/w")).tasks()
    }

    #[test]
    fn two_results_in_one_process_count_once() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "/w", "a", &[&[&result(0.2), &result(0.5)]]);

        let tasks = tasks(dir.path());

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].cost_usd, 0.5);
        assert_eq!(tasks[0].started_on, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
        assert_eq!(tasks[0].command.as_deref(), Some("/rank"));
        assert_eq!(tasks[0].outcome, TaskOutcome::Finished);
    }

    #[test]
    fn a_resume_adds_the_processes_together() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "/w", "a", &[&[&result(0.5)], &[&result(0.75)]]);
        assert_eq!(tasks(dir.path())[0].cost_usd, 1.25);
    }

    #[test]
    fn an_old_log_starts_a_new_process_where_the_cost_drops() {
        let dir = tempfile::tempdir().unwrap();
        let old = [META_LINE, &result(0.5), r#"{"type":"jobdesk_user","text":"go on"}"#, &result(0.25), &result(0.5)].join("\n");
        fs::write(dir.path().join("a.jsonl"), old).unwrap();
        assert_eq!(tasks(dir.path())[0].cost_usd, 1.0);
    }

    #[test]
    fn a_stop_counts_until_a_resume_finishes() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "/w", "a", &[&["stop"]]);
        assert_eq!(tasks(dir.path())[0].outcome, TaskOutcome::Stopped);
        assert_eq!(tasks(dir.path())[0].cost_usd, 0.0);

        write(dir.path(), "/w", "b", &[&[&result(0.5), "stop", &result(0.5)], &[&result(0.25)]]);
        let resumed = tasks(dir.path()).pop().unwrap();
        assert_eq!(resumed.outcome, TaskOutcome::Finished);
        assert_eq!(resumed.cost_usd, 0.75);
    }

    #[test]
    fn a_reply_after_a_stop_in_the_same_process_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let stopped = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"total_cost_usd":0.25}"#;
        write(dir.path(), "/w", "a", &[&["stop", stopped, "reply", &result(0.5)]]);

        let task = &tasks(dir.path())[0];

        assert_eq!(task.outcome, TaskOutcome::Finished);
        assert_eq!(task.cost_usd, 0.5);
        assert_eq!(RunLogs::new(dir.path().to_owned(), Path::new("/w")).recent(1)[0].status, RunStatus::Closed);
    }

    #[test]
    fn a_line_that_is_not_utf8_ends_the_log_there() {
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = [META_LINE, &result(0.5)].join("\n").into_bytes();
        bytes.extend(b"\n\xff\xfe\n");
        bytes.extend(result(0.75).into_bytes());
        fs::write(dir.path().join("a.jsonl"), bytes).unwrap();
        assert_eq!(tasks(dir.path())[0].cost_usd, 0.5);
    }

    #[test]
    fn an_error_result_fails_the_task() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "/w", "a", &[&[r#"{"type":"result","subtype":"error_max_budget_usd","is_error":true,"total_cost_usd":2.0}"#]]);
        assert_eq!(tasks(dir.path())[0].outcome, TaskOutcome::Failed);
    }

    #[test]
    fn a_task_with_no_result_and_another_workspace_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "/w", "a", &[&[r#"{"type":"system","subtype":"init","session_id":"s","model":"m"}"#]]);
        write(dir.path(), "/other", "b", &[&[&result(0.5)]]);
        fs::write(dir.path().join("c.jsonl"), "not json").unwrap();
        assert!(tasks(dir.path()).is_empty());
    }

    #[test]
    fn a_typed_task_has_no_command() {
        assert_eq!(command_of("  /apply\u{200b} https://x.example"), Some("/apply".into()));
        assert_eq!(command_of("/ai-job-search:rank_jobs-2 now"), Some("/ai-job-search:rank_jobs-2".into()));
        assert_eq!(command_of("Tell me a story"), None);
        assert_eq!(command_of("/Users/x/cv.pdf summarise"), None);
        assert_eq!(command_of("/ alone"), None);
        assert_eq!(command_of("/rank!"), None);
        assert_eq!(command_of(""), None);
    }

    #[test]
    fn the_cache_reads_only_a_changed_file_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.jsonl");
        fs::write(&path, [META_LINE, &result(0.5)].join("\n")).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let mut usage = RunLogUsage::new(dir.path().to_owned(), Path::new("/w"));
        assert_eq!(usage.tasks()[0].cost_usd, 0.5);

        // Same size and time: the cached figure stands, so the file was not read.
        fs::write(&path, [META_LINE, &result(0.7)].join("\n")).unwrap();
        File::options().write(true).open(&path).unwrap().set_modified(modified).unwrap();
        assert_eq!(usage.tasks()[0].cost_usd, 0.5);

        File::options().write(true).open(&path).unwrap().set_modified(modified + std::time::Duration::from_secs(1)).unwrap();
        assert_eq!(usage.tasks()[0].cost_usd, 0.7);
    }

    #[test]
    fn models_add_up_per_id() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "/w", "a", &[&[&result_with(0.5, &[("opus", 10, 0.375), ("haiku", 1, 0.125)])], &[&result_with(0.25, &[("opus", 20, 0.25)])]]);

        let models = &tasks(dir.path())[0].models;

        let opus = models.iter().find(|model| model.model == "opus").unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(*opus, ModelCost { model: "opus".into(), tokens_in: 44, tokens_out: 4, cache_read: 6, cost_usd: 0.625 });
    }

    #[test]
    fn a_stopped_log_replays_as_stopped() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "/w", "a", &[&["stop", r#"{"type":"result","subtype":"error_during_execution","is_error":true,"total_cost_usd":0.1}"#]]);
        write(dir.path(), "/w", "b", &[&[r#"{"type":"system","subtype":"init","session_id":"s","model":"m"}"#, "stop"]]);

        let runs = RunLogs::new(dir.path().to_owned(), Path::new("/w")).recent(10);

        assert_eq!(runs.iter().map(|run| run.status.clone()).collect::<Vec<_>>(), [RunStatus::Stopped, RunStatus::Stopped]);
    }

    const META_LINE: &str = r#"{"type":"jobdesk_meta","workspace":"/w","id":"a","line":"/rank","model":null,"budget_usd":2.0,"started_at":"2026-10-31 10:00"}"#;

    #[test]
    fn missing_folder_means_no_history() {
        assert!(RunLogs::new(PathBuf::from("/nonexistent/x"), Path::new("/w")).recent(5).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn logs_are_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        RunLog::open(dir.path(), Path::new("/w"), &request("a", "/rank")).unwrap();
        let mode = fs::metadata(dir.path().join("a.jsonl")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
