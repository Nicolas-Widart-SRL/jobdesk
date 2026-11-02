//! Enforces the layer rules from ARCHITECTURE.md that the module system cannot.

use std::fs;
use std::path::{Path, PathBuf};

const FEATURES: [&str; 8] = ["discovery", "dossier", "insights", "intray", "runs", "settings", "tracking", "workspace"];
const IMPURE: [&str; 9] = ["egui", "eframe", "std::fs", "notify", "serde", "csv::", "rfd", "Palette", "crate::ui_kit"];
const APPEARANCE: [&str; 15] = [
    "Color32", "Palette", "Stroke", "Shadow", "CornerRadius", "RichText", "Button::new", "ui.button", ".fill(", ".corner_radius(", ".color(", ".size(", ".family(", "CursorIcon", "on_hover_cursor",
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
    let text = fs::read_to_string(path).unwrap();
    text.split("#[cfg(test)]").next().unwrap_or_default().to_owned()
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

/// `ui_kit` owns every colour, stroke and type style. Screens only arrange its widgets.
#[test]
fn appearance_lives_in_ui_kit() {
    let mut files = Vec::new();
    for feature in FEATURES.iter().chain(["shell"].iter()) {
        rust_files(&Path::new("src").join(feature), &mut files);
    }
    let mut violations = Vec::new();
    for file in files {
        let code = production_code(&file);
        for word in APPEARANCE {
            if code.contains(word) {
                violations.push(format!("{}: {word} belongs in ui_kit", file.display()));
            }
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
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
