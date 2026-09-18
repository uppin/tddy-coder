//! The `evaluate-changes` phase's parser: the `Evaluate*` report types and
//! `parse_evaluate_response`.

use tddy_core::ParseError;

/// Build result entry from evaluate-changes output.
#[derive(Debug, Clone)]
pub struct EvaluateBuildResult {
    pub package: String,
    pub status: String,
    pub notes: Option<String>,
}

/// An issue found during evaluation.
#[derive(Debug, Clone)]
pub struct EvaluateIssue {
    pub severity: String,
    pub category: String,
    pub file: String,
    pub line: Option<u32>,
    pub description: String,
    pub suggestion: Option<String>,
}

/// Changeset sync status from evaluate-changes output.
#[derive(Debug, Clone)]
pub struct EvaluateChangesetSync {
    pub status: String,
    pub items_updated: u32,
    pub items_added: u32,
}

/// File analyzed entry from evaluate-changes output.
#[derive(Debug, Clone)]
pub struct EvaluateFileAnalyzed {
    pub file: String,
    pub lines_changed: Option<u32>,
    pub changeset_item: Option<String>,
}

/// Test impact summary from evaluate-changes output.
#[derive(Debug, Clone)]
pub struct EvaluateTestImpact {
    pub tests_affected: u32,
    pub new_tests_needed: u32,
}

#[derive(serde::Deserialize)]
struct EvaluateBuildResultDe {
    pub(crate) package: String,
    pub(crate) status: String,
    pub(crate) notes: Option<String>,
}

#[derive(serde::Deserialize)]
struct EvaluateIssueDe {
    pub(crate) severity: String,
    pub(crate) category: String,
    pub(crate) file: String,
    pub(crate) line: Option<u32>,
    pub(crate) description: String,
    pub(crate) suggestion: Option<String>,
}

#[derive(serde::Deserialize)]
struct EvaluateChangesetSyncDe {
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) items_updated: u32,
    #[serde(default)]
    pub(crate) items_added: u32,
}

#[derive(serde::Deserialize)]
struct EvaluateFileAnalyzedDe {
    pub(crate) file: String,
    pub(crate) lines_changed: Option<u32>,
    pub(crate) changeset_item: Option<String>,
}

#[derive(serde::Deserialize)]
struct EvaluateTestImpactDe {
    pub(crate) tests_affected: u32,
    pub(crate) new_tests_needed: u32,
}

// ── evaluate-changes output types ────────────────────────────────────────────

/// A changed file entry in an evaluate-changes report.
#[derive(Debug, Clone)]
pub struct EvaluateChangedFile {
    pub path: String,
    pub change_type: String,
    pub lines_added: i64,
    pub lines_removed: i64,
}

/// An affected test entry in an evaluate-changes report.
#[derive(Debug, Clone)]
pub struct EvaluateAffectedTest {
    pub path: String,
    pub status: String,
    pub description: String,
}

/// Parsed output from the evaluate-changes goal.
#[derive(Debug, Clone)]
pub struct EvaluateOutput {
    pub summary: String,
    pub risk_level: String,
    pub build_results: Vec<EvaluateBuildResult>,
    pub issues: Vec<EvaluateIssue>,
    pub changeset_sync: Option<EvaluateChangesetSync>,
    pub files_analyzed: Vec<EvaluateFileAnalyzed>,
    pub test_impact: Option<EvaluateTestImpact>,
    pub changed_files: Vec<EvaluateChangedFile>,
    pub affected_tests: Vec<EvaluateAffectedTest>,
    pub validity_assessment: String,
}

#[derive(serde::Deserialize)]
struct StructuredEvaluate {
    pub(crate) goal: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) risk_level: Option<String>,
    #[serde(default)]
    pub(crate) build_results: Option<Vec<EvaluateBuildResultDe>>,
    #[serde(default)]
    pub(crate) issues: Option<Vec<EvaluateIssueDe>>,
    #[serde(default)]
    pub(crate) changeset_sync: Option<EvaluateChangesetSyncDe>,
    #[serde(default)]
    pub(crate) files_analyzed: Option<Vec<EvaluateFileAnalyzedDe>>,
    #[serde(default)]
    pub(crate) test_impact: Option<EvaluateTestImpactDe>,
    #[serde(default)]
    pub(crate) changed_files: Option<Vec<EvaluateChangedFileDe>>,
    #[serde(default)]
    pub(crate) affected_tests: Option<Vec<EvaluateAffectedTestDe>>,
    #[serde(default)]
    pub(crate) validity_assessment: Option<String>,
}

#[derive(serde::Deserialize)]
struct EvaluateChangedFileDe {
    pub(crate) path: String,
    pub(crate) change_type: String,
    #[serde(default)]
    pub(crate) lines_added: i64,
    #[serde(default)]
    pub(crate) lines_removed: i64,
}

#[derive(serde::Deserialize)]
struct EvaluateAffectedTestDe {
    pub(crate) path: String,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) description: String,
}

/// Parse LLM evaluate-changes response. JSON must come from tddy-tools submit.
pub fn parse_evaluate_response(s: &str) -> Result<EvaluateOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredEvaluate = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("evaluate-changes") {
        return Err(ParseError::Malformed(format!(
            "goal is not evaluate-changes, got: {:?}",
            parsed.goal
        )));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| "No summary provided.".to_string());
    let risk_level = parsed
        .risk_level
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let build_results = parsed
        .build_results
        .unwrap_or_default()
        .into_iter()
        .map(|b| EvaluateBuildResult {
            package: b.package,
            status: b.status,
            notes: b.notes,
        })
        .collect();
    let issues = parsed
        .issues
        .unwrap_or_default()
        .into_iter()
        .map(|i| EvaluateIssue {
            severity: i.severity,
            category: i.category,
            file: i.file,
            line: i.line,
            description: i.description,
            suggestion: i.suggestion,
        })
        .collect();
    let changeset_sync = parsed.changeset_sync.map(|c| EvaluateChangesetSync {
        status: c.status,
        items_updated: c.items_updated,
        items_added: c.items_added,
    });
    let files_analyzed = parsed
        .files_analyzed
        .unwrap_or_default()
        .into_iter()
        .map(|f| EvaluateFileAnalyzed {
            file: f.file,
            lines_changed: f.lines_changed,
            changeset_item: f.changeset_item,
        })
        .collect();
    let test_impact = parsed.test_impact.map(|t| EvaluateTestImpact {
        tests_affected: t.tests_affected,
        new_tests_needed: t.new_tests_needed,
    });
    let changed_files: Vec<_> = parsed
        .changed_files
        .unwrap_or_default()
        .into_iter()
        .map(|c| EvaluateChangedFile {
            path: c.path,
            change_type: c.change_type,
            lines_added: c.lines_added,
            lines_removed: c.lines_removed,
        })
        .collect();
    let affected_tests: Vec<_> = parsed
        .affected_tests
        .unwrap_or_default()
        .into_iter()
        .map(|a| EvaluateAffectedTest {
            path: a.path,
            status: a.status,
            description: a.description,
        })
        .collect();
    let validity_assessment = parsed
        .validity_assessment
        .filter(|s| !s.is_empty())
        .unwrap_or_default();

    log::debug!(
        "[tddy-core] parse_evaluate_response: parsed {} changed_files, {} affected_tests",
        changed_files.len(),
        affected_tests.len()
    );

    Ok(EvaluateOutput {
        summary,
        risk_level,
        build_results,
        issues,
        changeset_sync,
        files_analyzed,
        test_impact,
        changed_files,
        affected_tests,
        validity_assessment,
    })
}
