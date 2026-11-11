use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::shared_kernel::host_path::{find_executable, resolved_path, tool_program};
use crate::shared_kernel::process_tree::no_window;
use crate::workspace::domain::{ToolProber, ToolSpec};

const TIMEOUT: Duration = Duration::from_secs(10);

pub struct ProcessProber;

impl ToolProber for ProcessProber {
    fn probe(&self, tool: &ToolSpec) -> Option<String> {
        version(resolved_path(), tool)
    }
}

impl ProcessProber {
    /// All `program` prints when run with `args` from the folder where the tool check finds the tool `beside`, so both
    /// belong to one install. `None` when either is missing, or it fails or takes longer than `TIMEOUT`.
    pub fn beside(&self, beside: &str, program: &str, args: &[&str]) -> Option<String> {
        output_beside(resolved_path(), beside, program, args)
    }
}

/// The first line `tool` prints when run with its arguments and `path` as its `PATH`, from the file
/// `host_path::tool_program` finds for it there: on its output, else on its errors.
fn version(path: &str, tool: &ToolSpec) -> Option<String> {
    let output = run(tool_program(path, tool.name)?, tool.args, None, path)?;
    let text = if output.stdout.is_empty() { output.stderr } else { output.stdout };
    Some(String::from_utf8_lossy(&text).lines().next().unwrap_or_default().trim().to_owned())
}

/// All `program` prints when run with `args`, looked for beside the file `beside` is on `path` and run with that
/// `PATH`. A `beside` that links elsewhere, as a TeX Live's `lualatex` in a folder of links does, is looked for beside
/// the file it links to first, then beside the link. `program` runs from the root folder, so no file in Jobdesk's own
/// current folder is found. `None` when either is missing, or it fails or takes longer than `TIMEOUT`.
fn output_beside(path: &str, beside: &str, program: &str, args: &[&str]) -> Option<String> {
    let found = find_executable(path, beside)?;
    let folders = [fs::canonicalize(&found).ok(), Some(found)];
    let program = folders.into_iter().flatten().filter_map(|file| Some(file.parent()?.join(program))).find(|program| program.is_file())?;
    let output = run(program, args, Some(Path::new("/")), path)?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs `program` with `args` and `path` as its `PATH`, in `folder` when one is given, with nothing to read. Gives what
/// it printed when it succeeds within `TIMEOUT`.
fn run(program: PathBuf, args: &[&str], folder: Option<&'static Path>, path: &str) -> Option<Output> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let path = path.to_owned();
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let mut command = Command::new(program);
        command.args(args).env("PATH", path).stdin(Stdio::null());
        if let Some(folder) = folder {
            command.current_dir(folder);
        }
        no_window(&mut command);
        let _ = sent.send(command.output());
    });
    let output = received.recv_timeout(TIMEOUT).ok()?.ok()?;
    output.status.success().then_some(output)
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::*;

    /// Writes a shell script `name` into `folder` that runs `body`, and makes it runnable.
    fn script(folder: &Path, name: &str, body: &str) {
        fs::create_dir_all(folder).unwrap();
        let file = folder.join(name);
        fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// `folders` joined as a `PATH`.
    fn path_of(folders: &[&Path]) -> String {
        folders.iter().map(|folder| folder.to_str().unwrap()).collect::<Vec<_>>().join(":")
    }

    #[test]
    fn the_program_beside_the_engine_runs_not_one_found_earlier_on_the_path() {
        let root = tempfile::tempdir().unwrap();
        let (earlier, tex) = (root.path().join("earlier"), root.path().join("tex"));
        script(&earlier, "kpsewhich", "echo /earlier/moderncv.cls");
        script(&tex, "lualatex", "exit 0");
        script(&tex, "kpsewhich", "echo \"/tex/$1\"");
        let path = path_of(&[&earlier, &tex]);
        assert_eq!(output_beside(&path, "lualatex", "kpsewhich", &["moderncv.cls"]).as_deref(), Some("/tex/moderncv.cls\n"));
        assert_eq!(output_beside(&path, "xelatex", "kpsewhich", &[]), None, "no engine");
        assert_eq!(output_beside(&path_of(&[&tex]), "lualatex", "mktexlsr", &[]), None, "nothing to run beside it");
    }

    /// The stub cases are in `host_path`'s tests, which can name their own `/usr/bin`.
    #[cfg(target_os = "macos")]
    #[test]
    fn on_macos_the_tool_check_runs_the_python3_a_task_runs() {
        let root = tempfile::tempdir().unwrap();
        let (old, new, plain, unrunnable) = (root.path().join("old"), root.path().join("new"), root.path().join("plain"), root.path().join("unrunnable"));
        script(&old, "python3", "echo 'Python 3.9.6'");
        script(&new, "python3", "echo 'Python 3.12.4'");
        script(&plain, "python", "echo 'Python 3.12.7'");
        script(&unrunnable, "python3", "echo 'Python 3.13.0'");
        fs::set_permissions(unrunnable.join("python3"), fs::Permissions::from_mode(0o644)).unwrap();
        let python = ToolSpec { name: "python3", args: &["--version"], purpose: "", required: true };
        assert_eq!(version(&path_of(&[&old, &new]), &python).as_deref(), Some("Python 3.9.6"), "the later one is not reached");
        assert_eq!(version(&path_of(&[&new, &old]), &python).as_deref(), Some("Python 3.12.4"));
        assert_eq!(version(&path_of(&[&unrunnable, &new]), &python).as_deref(), Some("Python 3.12.4"), "a file it may not run is passed, as a shell does");
        assert_eq!(version(&path_of(&[&plain]), &python), None, "ai-job-search's tools call python3, never python");
    }

    #[test]
    fn the_program_runs_from_the_root_folder() {
        let root = tempfile::tempdir().unwrap();
        let tex = root.path().join("tex");
        script(&tex, "lualatex", "exit 0");
        script(&tex, "kpsewhich", "pwd");
        assert_eq!(output_beside(&path_of(&[&tex]), "lualatex", "kpsewhich", &[]).as_deref(), Some("/\n"));
    }

    #[test]
    fn a_program_that_fails_gives_nothing() {
        let root = tempfile::tempdir().unwrap();
        let tex = root.path().join("tex");
        script(&tex, "lualatex", "exit 0");
        script(&tex, "kpsewhich", "echo /tex/moderncv.cls\nexit 1");
        assert_eq!(output_beside(&path_of(&[&tex]), "lualatex", "kpsewhich", &[]), None);
    }

    #[test]
    fn an_engine_linked_from_elsewhere_finds_the_program_beside_its_target_first_then_beside_the_link() {
        let root = tempfile::tempdir().unwrap();
        let (links, texlive) = (root.path().join("links"), root.path().join("texlive").join("bin"));
        script(&texlive, "lualatex", "exit 0");
        script(&texlive, "kpsewhich", "echo texlive");
        fs::create_dir_all(&links).unwrap();
        symlink(texlive.join("lualatex"), links.join("lualatex")).unwrap();
        let path = path_of(&[&links]);
        assert_eq!(output_beside(&path, "lualatex", "kpsewhich", &[]).as_deref(), Some("texlive\n"), "only the engine is linked");
        script(&links, "kpsewhich", "echo links");
        assert_eq!(output_beside(&path, "lualatex", "kpsewhich", &[]).as_deref(), Some("texlive\n"), "the target's own comes first");
        fs::remove_file(texlive.join("kpsewhich")).unwrap();
        assert_eq!(output_beside(&path, "lualatex", "kpsewhich", &[]).as_deref(), Some("links\n"), "then the one beside the link");
    }
}
