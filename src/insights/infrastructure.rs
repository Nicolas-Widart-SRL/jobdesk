use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::insights::domain::{SkillGap, SkillGapReport, SkillGapSource, StudyItem};
use crate::shared_kernel::LoadError;
use crate::shared_kernel::dates::iso_dates_in;

const SOURCE: &str = "upskill";

/// Aggregate reports are `report-YYYY-MM-DD.md`; targeted ones add a company and role.
pub struct UpskillReports {
    folder: PathBuf,
}

impl UpskillReports {
    pub fn in_workspace(root: &Path) -> Self {
        Self { folder: root.join(SOURCE) }
    }
}

impl SkillGapSource for UpskillReports {
    fn latest(&self) -> Result<Option<SkillGapReport>, LoadError> {
        let entries = match fs::read_dir(&self.folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(LoadError::new(SOURCE, error)),
        };
        let mut names: Vec<String> = entries.flatten().filter_map(|entry| entry.file_name().into_string().ok()).filter(|name| is_aggregate(name)).collect();
        names.sort();
        let Some(name) = names.pop() else { return Ok(None) };
        let text = fs::read_to_string(self.folder.join(&name)).map_err(|error| LoadError::new(format!("{SOURCE}/{name}"), error))?;
        Ok(Some(parse_report(name, &text)))
    }
}

fn is_aggregate(name: &str) -> bool {
    name.strip_prefix("report-").and_then(|rest| rest.strip_suffix(".md")).is_some_and(|date| date.len() == 10 && iso_dates_in(date).len() == 1)
}

fn parse_report(file_name: String, text: &str) -> SkillGapReport {
    let mut report = SkillGapReport { dated: iso_dates_in(&file_name).first().copied(), file_name, raw: text.to_owned(), ..Default::default() };
    let mut section = "";
    let mut bullets_into_new = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(heading) = line.strip_prefix("## ") {
            section = heading.trim();
        } else if let Some(mode) = line.strip_prefix("**Mode:**") {
            report.mode = mode.trim().to_owned();
        } else if section == "Since Last Report" {
            if line.starts_with("**Gaps closed") {
                bullets_into_new = false;
            } else if line.starts_with("**New gaps") {
                bullets_into_new = true;
            } else if let Some(item) = line.strip_prefix("- ") {
                let target = if bullets_into_new { &mut report.new_gaps } else { &mut report.gaps_closed };
                target.push(item.to_owned());
            }
        } else if section == "Gap Heatmap" {
            if let [priority, skill, kind, source, ..] = table_cells(line).as_slice() {
                report.gaps.push(SkillGap { priority: priority.clone(), skill: skill.clone(), kind: kind.clone(), source: source.clone() });
            }
        } else if section == "Suggested Study Order" {
            if let [_, topic, kind, time, note, ..] = table_cells(line).as_slice() {
                report.study_order.push(StudyItem { topic: topic.clone(), kind: kind.clone(), time: time.clone(), note: note.clone() });
            } else if line.contains("Total estimated time") {
                report.total_time = Some(line.trim_matches('*').trim().to_owned());
            }
        }
    }
    report
}

/// Data cells of a Markdown table row. Header and rule rows give nothing.
fn table_cells(line: &str) -> Vec<String> {
    let Some(inner) = line.strip_prefix('|').and_then(|rest| rest.strip_suffix('|')) else {
        return Vec::new();
    };
    let cells: Vec<String> = inner.split('|').map(|cell| cell.trim().to_owned()).collect();
    let is_rule = cells.iter().all(|cell| cell.chars().all(|c| matches!(c, '-' | ':')));
    let is_header = matches!(cells.first().map(String::as_str), Some("Priority" | "#"));
    if is_rule || is_header { Vec::new() } else { cells }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = "# Upskill Report — 2026-10-28\n**Mode:** Aggregate (9 jobs analysed: 4 tracked, 5 ranked)\n\n---\n\n## Since Last Report\n**Gaps closed** (skills added to profile since 2026-09-01):\n- Terraform\n\n**New gaps** (from jobs tracked since 2026-09-01):\n- TOGAF\n\n---\n\n## Gap Heatmap\n\n| Priority | Skill / Area | Type | Gap Source |\n|----------|-------------|------|------------|\n| Critical | Azure | Hard | 6 jobs (4 recorded gaps, 2 inferred), score 3.4 |\n| High | Security | Domain | LLM synthesis |\n\n---\n\n## Suggested Study Order\n\n| # | Topic | Type | Est. Time | Note |\n|---|---|---|---|---|\n| 1 | Azure | Hard | 20h | First |\n\n**Total estimated time: ~20h**\n";

    #[test]
    fn reads_every_section() {
        let report = parse_report("report-2026-10-28.md".into(), REPORT);
        assert!(report.mode.starts_with("Aggregate"));
        assert_eq!(report.gaps.len(), 2);
        assert_eq!(report.gaps[0].skill, "Azure");
        assert_eq!(report.study_order[0].time, "20h");
        assert_eq!(report.total_time.as_deref(), Some("Total estimated time: ~20h"));
        assert_eq!((report.gaps_closed.len(), report.new_gaps.len()), (1, 1));
    }

    #[test]
    fn unreadable_report_keeps_raw_text() {
        let report = parse_report("report-2026-10-28.md".into(), "Free prose");
        assert!(report.gaps.is_empty());
        assert_eq!(report.raw, "Free prose");
    }

    #[test]
    fn picks_latest_aggregate_and_skips_targeted() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(SOURCE);
        fs::create_dir(&folder).unwrap();
        for name in ["report-2026-09-01.md", "report-2026-10-01.md", "report-2026-11-01-acme-lead.md"] {
            fs::write(folder.join(name), "x").unwrap();
        }
        let latest = UpskillReports::in_workspace(dir.path()).latest().unwrap().unwrap();
        assert_eq!(latest.file_name, "report-2026-10-01.md");
    }
}
