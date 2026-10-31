use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::runs::domain::{Run, RunHistory, RunRequest};
use crate::runs::infrastructure::stream_json::parse_line;

const META: &str = "jobdesk_meta";
const USER: &str = "jobdesk_user";

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
            log.append_user(&request.line);
        }
        Ok(log)
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
            match serde_json::from_str::<UserLine>(&line) {
                Ok(user) if user.r#type == USER => run.note_reply(&user.text),
                _ => parse_line(&line).into_iter().for_each(|event| run.apply(event)),
            }
        }
        run.settle_after_restore();
        Some(run)
    }
}

impl RunHistory for RunLogs {
    fn recent(&self, limit: usize) -> Vec<Run> {
        let Ok(entries) = fs::read_dir(&self.dir) else { return Vec::new() };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|ext| ext == "jsonl")).collect();
        paths.sort();
        paths.iter().rev().filter_map(|path| self.replay(path)).take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::domain::RunStatus;

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
