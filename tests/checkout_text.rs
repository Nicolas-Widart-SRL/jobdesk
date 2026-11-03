//! A checkout saved on Windows reads as the same checkout: CRLF line endings and a UTF-8 byte order mark change nothing the
//! app shows. The CRLF and BOM copies are built here from the LF sample, so the repository never holds them.

use std::fs;
use std::path::Path;

use chrono::NaiveDate;
use jobdesk::profile::application::topic_counts;
use jobdesk::shell::Snapshot;

#[derive(Clone, Copy)]
enum Text {
    Lf,
    Crlf,
    Bom,
}

fn copy_tree(from: &Path, to: &Path, kind: Text) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let (path, target) = (entry.path(), to.join(entry.file_name()));
        let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_owned();
        if path.is_dir() {
            copy_tree(&path, &target, kind);
            continue;
        }
        if extension == "pdf" {
            fs::copy(&path, &target).unwrap();
            continue;
        }
        let lf = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
        let text = match kind {
            Text::Lf => lf,
            Text::Crlf => lf.replace('\n', "\r\n"),
            Text::Bom if matches!(extension.as_str(), "md" | "csv") => format!("\u{feff}{lf}"),
            Text::Bom => lf,
        };
        fs::write(&target, text).unwrap();
    }
}

/// What the screens read from a snapshot of the sample copied as `kind`. Line endings in the texts become LF and the
/// temporary folder becomes `<root>`, so only the parse is compared.
fn load(kind: Text) -> Vec<(&'static str, String)> {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("ai-job-search");
    copy_tree(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample_workspace"), &root, kind);
    // A real `CLAUDE.md` in this repository would steer the agents that work on it.
    fs::rename(root.join("CLAUDE.profile.md"), root.join("CLAUDE.md")).unwrap();
    let snapshot = Snapshot::load(&root, NaiveDate::from_ymd_opt(2026, 10, 31).unwrap());
    assert!(snapshot.issues.is_empty(), "{:?}", snapshot.issues);
    assert!(
        topic_counts(&snapshot.profile).filled > 0 && snapshot.board.total_seen > 0 && snapshot.pipeline.cards().count() > 0,
        "the sample has something to compare"
    );
    let mut archives: Vec<_> = snapshot.archives.iter().collect();
    archives.sort_by_key(|(folder, _)| *folder);
    let root = format!("{root:?}").trim_matches('"').to_owned();
    [
        ("topics", format!("{:?}", snapshot.profile)),
        ("topic counts", format!("{:?}", topic_counts(&snapshot.profile))),
        ("jobs and lanes", format!("{:?}", snapshot.board)),
        ("terms", format!("{:?}", snapshot.terms)),
        ("applications", format!("{:?}", snapshot.pipeline)),
        ("insights", format!("{:?} {:?}", snapshot.insights, snapshot.band)),
        ("skill gaps report", format!("{:?}", snapshot.skill_gaps)),
        ("application reports", format!("{archives:?}")),
        ("workspace", format!("{:?}", snapshot.workspace)),
    ]
    .into_iter()
    .map(|(part, shown)| (part, shown.replace("\\r\\n", "\\n").replace(&root, "<root>")))
    .collect()
}

fn assert_reads_as_lf(kind: Text) {
    let differ: Vec<String> = load(Text::Lf)
        .into_iter()
        .zip(load(kind))
        .filter(|((_, want), (_, got))| want != got)
        .map(|((part, want), (_, got))| {
            let at = want.chars().zip(got.chars()).take_while(|(a, b)| a == b).count();
            let near = |text: &str| text.chars().skip(at.saturating_sub(100)).take(200).collect::<String>();
            format!("{part} differ\n  LF: {}\n got: {}", near(&want), near(&got))
        })
        .collect();
    assert!(differ.is_empty(), "{}", differ.join("\n"));
}

#[test]
fn a_crlf_checkout_reads_as_the_lf_one() {
    assert_reads_as_lf(Text::Crlf);
}

#[test]
fn a_checkout_with_byte_order_marks_reads_as_the_lf_one() {
    assert_reads_as_lf(Text::Bom);
}
