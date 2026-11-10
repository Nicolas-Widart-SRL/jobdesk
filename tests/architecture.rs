//! Enforces the layer rules from ARCHITECTURE.md that the module system cannot.

use std::fs;
use std::path::{Path, PathBuf};

const FEATURES: [&str; 11] = ["discovery", "dossier", "insights", "intray", "portals", "profile", "runs", "settings", "tracking", "updates", "workspace"];
const IMPURE: [&str; 12] = ["egui", "eframe", "std::fs", "notify", "serde", "csv::", "rfd", "Palette", "crate::ui_kit", "keyring", "ureq", "minisign"];
const APPEARANCE: [&str; 15] = [
    "Color32",
    "Palette",
    "Stroke",
    "Shadow",
    "CornerRadius",
    "RichText",
    "Button::new",
    "ui.button",
    ".fill(",
    ".corner_radius(",
    ".color(",
    ".size(",
    ".family(",
    "CursorIcon",
    "on_hover_cursor",
];

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

/// Production code only: unit tests may build fixtures from any layer.
fn production_code(path: &Path) -> String {
    production_source(&fs::read_to_string(path).unwrap()).to_owned()
}

fn production_source(text: &str) -> &str {
    text.split("#[cfg(test)]").next().unwrap_or_default()
}

/// Drops the whitespace before each `.`, so a method chain that rustfmt broke after its receiver matches like one line.
fn joined_chains(code: &str) -> String {
    let mut joined = String::with_capacity(code.len());
    for c in code.chars() {
        if c == '.' {
            joined.truncate(joined.trim_end().len());
        }
        joined.push(c);
    }
    joined
}

/// `crate::feature::layer` references, with `use crate::feature::{...}` groups kept coarse.
fn crate_paths(code: &str) -> Vec<(String, String)> {
    code.match_indices("crate::")
        .map(|(at, marker)| {
            let mut parts = code[at + marker.len()..].split("::");
            let module = parts.next().unwrap_or_default();
            let layer = parts.next().unwrap_or_default();
            let word = |s: &str| s.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect::<String>();
            (word(module), word(layer))
        })
        .collect()
}

fn allowed(feature: &str, layer: &str, module: &str, target: &str) -> bool {
    let own = module == feature;
    match (layer, module) {
        (_, "shared_kernel") => true,
        (_, "ui_kit") => layer == "ui",
        ("domain", _) => own && target == "domain",
        ("application", _) => (own && target == "domain") || target == "application",
        ("infrastructure", _) => own && matches!(target, "domain" | "infrastructure"),
        ("ui", _) => own && matches!(target, "application" | "ui"),
        _ => false,
    }
}

fn layer_of(path: &Path, feature_root: &Path) -> Option<String> {
    let relative = path.strip_prefix(feature_root).ok()?;
    let first = relative.components().next()?.as_os_str().to_str()?;
    let layer = first.trim_end_matches(".rs");
    (layer != "mod").then(|| layer.to_owned())
}

#[test]
fn features_respect_layer_rules() {
    let mut violations = Vec::new();
    for feature in FEATURES {
        let root = Path::new("src").join(feature);
        let mut files = Vec::new();
        rust_files(&root, &mut files);
        for file in files {
            let Some(layer) = layer_of(&file, &root) else { continue };
            let code = production_code(&file);
            if code.contains("super::") {
                violations.push(format!("{}: use crate:: paths so layers stay checkable", file.display()));
            }
            for (module, target) in crate_paths(&code) {
                if !allowed(feature, &layer, &module, &target) {
                    violations.push(format!("{}: {layer} must not use crate::{module}::{target}", file.display()));
                }
            }
            if matches!(layer.as_str(), "domain" | "application") {
                for word in IMPURE {
                    if code.contains(word) {
                        violations.push(format!("{}: {layer} must not touch {word}", file.display()));
                    }
                }
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

/// The appearance words a source file uses outside its test module, split chains included.
fn appearance_hits(source: &str) -> Vec<&'static str> {
    let code = joined_chains(production_source(source));
    APPEARANCE.into_iter().filter(|word| code.contains(word)).collect()
}

/// `ui_kit` owns every colour, stroke and type style. Screens only arrange its widgets.
#[test]
fn appearance_lives_in_ui_kit() {
    let mut files = Vec::new();
    for feature in FEATURES.iter().chain(["shell"].iter()) {
        rust_files(&Path::new("src").join(feature), &mut files);
    }
    let mut violations = Vec::new();
    for file in files {
        for word in appearance_hits(&fs::read_to_string(&file).unwrap()) {
            violations.push(format!("{}: {word} belongs in ui_kit", file.display()));
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn appearance_check_sees_chains_that_rustfmt_split() {
    let split = "fn show(ui: &mut Ui) {\n    ui\n        .button(\"x\");\n}\n";
    assert_eq!(appearance_hits(split), ["ui.button"]);
    let in_tests = format!("fn show() {{}}\n#[cfg(test)]\nmod tests {{\n{split}}}\n");
    assert!(appearance_hits(&in_tests).is_empty());
}

#[test]
fn domain_and_infrastructure_stay_private() {
    for feature in FEATURES {
        let declarations = fs::read_to_string(Path::new("src").join(feature).join("mod.rs")).unwrap();
        for layer in ["domain", "infrastructure"] {
            assert!(!declarations.contains(&format!("pub mod {layer}")), "{feature}::{layer} must be private");
        }
    }
}

/// Every place `needle` shows in the files, as `(path, byte offset)`.
fn occurrences(files: &[(PathBuf, String)], needle: &str) -> Vec<(PathBuf, usize)> {
    files.iter().flat_map(|(file, text)| text.match_indices(needle).map(|(at, _)| (file.clone(), at))).collect()
}

fn read_all(root: &str) -> Vec<(PathBuf, String)> {
    let mut paths = Vec::new();
    rust_files(Path::new(root), &mut paths);
    paths.into_iter().map(|path| (path.clone(), fs::read_to_string(path).unwrap())).collect()
}

/// The words are put together here, so this file does not match its own search.
fn keychain_call() -> String {
    ["system", "_keychain("].concat()
}

/// What is wrong with how the sources reach the system keychain: its crate is named in one adapter, and it is built in
/// `settings/mod.rs` and, at most, in the body of `fn start(` in the shell. The body ends at the first closing brace
/// indented four spaces. An end that is not found is a violation, so the check never fails open.
fn keychain_violations(sources: &[(PathBuf, String)]) -> Vec<String> {
    // A checkout with CRLF line ends reads like one with LF, so the boundary below finds its end.
    let sources: Vec<(PathBuf, String)> = sources.iter().map(|(file, text)| (file.clone(), text.replace("\r\n", "\n"))).collect();
    let sources = sources.as_slice();
    let mut violations = Vec::new();
    let crate_path = ["key", "ring::"].concat();
    for (file, _) in occurrences(sources, &crate_path) {
        if file != Path::new("src/settings/infrastructure/system_keychain.rs") {
            violations.push(format!("{}: {crate_path} belongs in the system keychain adapter", file.display()));
        }
    }
    let app = Path::new("src/shell/app.rs");
    let body = sources.iter().find(|(file, _)| file == app).and_then(|(_, text)| {
        // The definition line, so a mention of `fn start(` in a comment or a string does not move the range.
        let at = text.find("\n    fn start(")? + 1;
        Some(at..at + text[at..].find("\n    }\n")?)
    });
    for (file, at) in occurrences(sources, &keychain_call()) {
        let allowed = file == Path::new("src/settings/mod.rs") || (file == app && body.as_ref().is_some_and(|body| body.contains(&at)));
        if !allowed {
            violations.push(format!("{}: may not call the system keychain here", file.display()));
        }
    }
    violations
}

#[test]
fn the_system_keychain_is_reached_from_one_place() {
    let violations = keychain_violations(&read_all("src"));
    assert!(violations.is_empty(), "{}", violations.join("\n"));
    assert!(occurrences(&read_all("tests"), &keychain_call()).is_empty(), "tests must never touch the real keychain");
}

/// The files whose code outside tests starts more processes than it calls `no_window` for. On Windows a console program
/// opens a console window unless `no_window` starts it. The definition of `no_window` is no call. Explorer is a window
/// program, and the login-shell call runs on Unix only.
fn console_window_violations(sources: &[(PathBuf, String)]) -> Vec<String> {
    let exempt = [Path::new("src/settings/infrastructure/folder_reveal.rs"), Path::new("src/shared_kernel/host_path.rs")];
    sources
        .iter()
        .filter(|(file, _)| !exempt.contains(&file.as_path()))
        .filter_map(|(file, text)| {
            let code = production_source(text);
            let starts = code.matches("Command::new(").count();
            let calls = code.matches("no_window(").count() - code.matches("fn no_window(").count();
            (starts > calls).then(|| format!("{}: {starts} process starts but {calls} no_window calls", file.display()))
        })
        .collect()
}

#[test]
fn every_process_starts_without_a_console_window() {
    let violations = console_window_violations(&read_all("src"));
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

fn source(path: &str, text: &str) -> (PathBuf, String) {
    (PathBuf::from(path), text.to_owned())
}

#[test]
fn the_console_window_check_needs_a_call_for_each_start() {
    let file = |text: &str| source("src/runs/infrastructure/gateway.rs", text);
    let start = "let mut command = Command::new(program);\n";
    let call = "no_window(&mut command);\n";
    assert!(console_window_violations(&[file(&format!("{start}{call}"))]).is_empty());
    assert!(console_window_violations(&[file("let x = 1;\n")]).is_empty(), "no start needs no call");
    assert_eq!(console_window_violations(&[file(start)]).len(), 1, "a start without a call is rejected");
    assert_eq!(console_window_violations(&[file(&format!("{start}{call}{start}"))]).len(), 1, "two starts need two calls");
    let definition = format!("pub fn no_window(command: &mut Command) {{}}\n{start}");
    assert_eq!(console_window_violations(&[file(&definition)]).len(), 1, "the definition is no call");
    assert!(console_window_violations(&[file(&format!("fn a() {{}}\n#[cfg(test)]\nmod tests {{\n{start}}}\n"))]).is_empty(), "tests may start any");
    assert!(console_window_violations(&[source("src/settings/infrastructure/folder_reveal.rs", start)]).is_empty());
    assert!(console_window_violations(&[source("src/shared_kernel/host_path.rs", start)]).is_empty());
}

#[test]
fn the_keychain_check_sees_a_call_outside_its_one_place() {
    let call = keychain_call();
    let app = |text: &str| source("src/shell/app.rs", text);
    let inside = format!("impl App {{\n    fn start() {{\n        let keys = {call});\n    }}\n\n    fn other() {{}}\n}}\n");
    let outside = format!("impl App {{\n    fn start() {{\n        let _ = 1;\n    }}\n\n    fn other() {{\n        let keys = {call});\n    }}\n}}\n");
    assert!(keychain_violations(&[app(&inside)]).is_empty());
    assert!(keychain_violations(&[source("src/settings/mod.rs", &format!("pub fn {call}) {{}}"))]).is_empty());
    assert!(keychain_violations(&[app(&inside.replace('\n', "\r\n"))]).is_empty(), "CRLF line ends find the end of the body");
    assert_eq!(keychain_violations(&[app(&outside.replace('\n', "\r\n"))]).len(), 1, "and still reject a call outside it");
    assert_eq!(keychain_violations(&[app(&outside)]).len(), 1, "a call in another function is rejected");
    assert_eq!(keychain_violations(&[source("src/shell/other.rs", &inside)]).len(), 1, "a call in another file is rejected");
    assert_eq!(keychain_violations(&[source("src/settings/ui/mod.rs", &inside)]).len(), 1);
    let mention = format!(
        "/// Built by `fn start(` below.\nimpl App {{\n    fn other() {{\n        let keys = {call});\n    }}\n\n    fn start() {{\n        let _ = 1;\n    }}\n}}\n"
    );
    assert_eq!(keychain_violations(&[app(&mention)]).len(), 1, "a mention of `fn start(` earlier does not widen the body");
    let no_start = format!("fn other() {{\n    let keys = {call});\n}}\n");
    assert_eq!(keychain_violations(&[app(&no_start)]).len(), 1, "no `fn start(` allows nothing");
    let unended = format!("impl App {{\n    fn start() {{\n        let keys = {call});\n");
    assert_eq!(keychain_violations(&[app(&unended)]).len(), 1, "a body whose end is not found allows nothing");
    let adapter = format!("use {}Entry;", ["key", "ring::"].concat());
    assert_eq!(keychain_violations(&[source("src/shell/app.rs", &adapter)]).len(), 1);
    assert!(keychain_violations(&[source("src/settings/infrastructure/system_keychain.rs", &adapter)]).is_empty());
}
