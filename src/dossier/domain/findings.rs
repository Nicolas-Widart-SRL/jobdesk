/// `/apply` reports these only in its run output, not in any file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunFindings {
    pub evaluation: Option<String>,
    pub keywords: Vec<KeywordRow>,
    pub checklist: Vec<CheckItem>,
    /// Shown as written when no checklist line could be read.
    pub verification_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeywordRow {
    pub keyword: String,
    pub priority: String,
    pub status: String,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckResult {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckItem {
    pub label: String,
    pub result: CheckResult,
}

/// Reads Claude's messages, oldest first. Later messages win, as they follow revisions.
pub fn extract_findings(messages: &[String]) -> RunFindings {
    let mut findings = RunFindings::default();
    for message in messages {
        let lower = message.to_lowercase();
        if lower.contains("skills match") || lower.contains("overall fit") {
            findings.evaluation = Some(message.clone());
        }
        let keywords = keyword_rows(message);
        if !keywords.is_empty() {
            findings.keywords = keywords;
        }
        if lower.contains("verification") {
            let checklist = check_items(message);
            if checklist.is_empty() {
                findings.verification_text = Some(message.clone());
            } else {
                findings.checklist = checklist;
                findings.verification_text = None;
            }
        }
    }
    findings
}

fn cells(line: &str) -> Option<Vec<String>> {
    let inner = line.trim().strip_prefix('|')?.strip_suffix('|')?;
    Some(inner.split('|').map(|cell| cell.trim().trim_matches('*').trim().to_owned()).collect())
}

fn is_rule(row: &[String]) -> bool {
    row.iter().all(|cell| !cell.is_empty() && cell.chars().all(|c| matches!(c, '-' | ':')))
}

fn keyword_rows(message: &str) -> Vec<KeywordRow> {
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in message.lines() {
        let Some(row) = cells(line) else {
            in_table = false;
            continue;
        };
        if row.first().is_some_and(|cell| cell.eq_ignore_ascii_case("keyword")) {
            in_table = true;
        } else if in_table && !is_rule(&row) {
            if let [keyword, priority, status, rest @ ..] = row.as_slice() {
                rows.push(KeywordRow { keyword: keyword.clone(), priority: priority.clone(), status: status.clone(), note: rest.join(" ") });
            }
        }
    }
    rows
}

fn result_of(text: &str) -> Option<CheckResult> {
    let lower = text.to_lowercase();
    if lower.contains("pass/fail") {
        return None;
    }
    if lower.contains("[ ]") || lower.contains('❌') || lower.contains("fail") {
        Some(CheckResult::Fail)
    } else if lower.contains("[x]") || lower.contains('✅') || lower.contains("pass") {
        Some(CheckResult::Pass)
    } else {
        None
    }
}

fn check_items(message: &str) -> Vec<CheckItem> {
    message
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if let Some(row) = cells(line) {
                let [label, verdict, ..] = row.as_slice() else { return None };
                return (!is_rule(&row)).then(|| result_of(verdict)).flatten().map(|result| CheckItem { label: label.clone(), result });
            }
            let bullet = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "))?;
            let result = result_of(bullet)?;
            Some(CheckItem { label: label_of(bullet), result })
        })
        .collect()
}

fn label_of(bullet: &str) -> String {
    let mut label = bullet.to_owned();
    for marker in ["[x]", "[X]", "[ ]", "✅", "❌", "**PASS**", "**FAIL**", "PASS", "FAIL", "Pass", "Fail"] {
        label = label.replace(marker, "");
    }
    label.trim_matches(|c: char| c.is_whitespace() || matches!(c, ':' | '-' | '—' | '*')).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_documented_keyword_table() {
        let message = "Keywords:\n\n| Keyword | Priority | Status | Note |\n|---------|----------|--------|------|\n| Kafka | required | covered | Skills |\n| TOGAF | preferred | missing (gap) | not on profile |\n\nDone.\n| Other | table |";
        let findings = extract_findings(&[message.to_owned()]);
        assert_eq!(findings.keywords.len(), 2);
        assert_eq!(findings.keywords[1].status, "missing (gap)");
    }

    #[test]
    fn reads_checklists_in_several_shapes() {
        let message = "## Verification Checklist\n- [x] Claims match\n- [ ] Named contact\n- ✅ One page\n- Spelling: PASS\n- Report pass/fail for each item\n| Item | Result |\n|---|---|\n| Fonts match | Pass |\n";
        let items = extract_findings(&[message.to_owned()]).checklist;
        let found: Vec<_> = items.iter().map(|item| (item.label.as_str(), item.result)).collect();
        assert_eq!(found, [
            ("Claims match", CheckResult::Pass),
            ("Named contact", CheckResult::Fail),
            ("One page", CheckResult::Pass),
            ("Spelling", CheckResult::Pass),
            ("Fonts match", CheckResult::Pass),
        ]);
    }

    #[test]
    fn unreadable_verification_stays_as_text() {
        let findings = extract_findings(&["Verification went well overall.".to_owned()]);
        assert!(findings.checklist.is_empty());
        assert!(findings.verification_text.is_some());
    }

    #[test]
    fn the_latest_evaluation_wins() {
        let findings = extract_findings(&["Skills match: old".to_owned(), "Unrelated".to_owned(), "Overall fit: new".to_owned()]);
        assert_eq!(findings.evaluation.as_deref(), Some("Overall fit: new"));
    }
}
