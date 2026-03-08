//! Lower-level unit tests for the validate-changes goal internals.
//!
//! These tests exercise individual code paths:
//! - `workflow::validate::build_prompt`
//! - `workflow::validate::system_prompt`
//! - `output::write_validation_report`
//! - `permission::validate_allowlist`
//! - `changeset::next_goal_for_state` (validate-changes not in auto-sequence)

use tddy_core::{
    next_goal_for_state, validate_allowlist, write_validation_report, ValidateBuildResult,
    ValidateChangesetSync, ValidateFileAnalyzed, ValidateIssue, ValidateOutput, ValidateTestImpact,
};

fn make_validate_output(summary: &str, risk_level: &str) -> ValidateOutput {
    ValidateOutput {
        summary: summary.to_string(),
        risk_level: risk_level.to_string(),
        build_results: vec![ValidateBuildResult {
            package: "my-crate".to_string(),
            status: "pass".to_string(),
            notes: None,
        }],
        issues: vec![ValidateIssue {
            severity: "warning".to_string(),
            category: "code_quality".to_string(),
            file: "src/main.rs".to_string(),
            line: Some(5),
            description: "Magic number 42".to_string(),
            suggestion: Some("Use a constant".to_string()),
        }],
        changeset_sync: Some(ValidateChangesetSync {
            status: "synced".to_string(),
            items_updated: 1,
            items_added: 0,
        }),
        files_analyzed: vec![ValidateFileAnalyzed {
            file: "src/main.rs".to_string(),
            lines_changed: Some(10),
            changeset_item: None,
        }],
        test_impact: Some(ValidateTestImpact {
            tests_affected: 2,
            new_tests_needed: 0,
        }),
    }
}

/// write_validation_report() creates validation-report.md in the working directory.
#[test]
fn write_validation_report_creates_file_in_working_dir() {
    let dir = std::env::temp_dir().join("tddy-validate-unit-writer");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");

    let output = make_validate_output("Analyzed 1 file.", "low");
    let result = write_validation_report(&dir, &output);

    assert!(
        result.is_ok(),
        "write_validation_report should succeed, got: {:?}",
        result
    );
    assert!(
        dir.join("validation-report.md").exists(),
        "validation-report.md should be created"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// write_validation_report() includes summary text in the written file.
#[test]
fn write_validation_report_includes_summary_in_output() {
    let dir = std::env::temp_dir().join("tddy-validate-unit-summary");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");

    let output = make_validate_output("Analyzed 5 files with risk.", "high");
    let _ = write_validation_report(&dir, &output);

    let content = std::fs::read_to_string(dir.join("validation-report.md"))
        .expect("should be able to read validation-report.md");
    assert!(
        content.contains("Analyzed 5 files"),
        "report should include summary text, got: {}",
        content
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// write_validation_report() includes the risk level in the written file.
#[test]
fn write_validation_report_includes_risk_level_in_output() {
    let dir = std::env::temp_dir().join("tddy-validate-unit-risk");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");

    let output = make_validate_output("Summary.", "critical");
    let _ = write_validation_report(&dir, &output);

    let content = std::fs::read_to_string(dir.join("validation-report.md"))
        .expect("should be able to read validation-report.md");
    assert!(
        content.contains("critical"),
        "report should include risk level, got: {}",
        content
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// write_validation_report() includes issue descriptions in the written file.
#[test]
fn write_validation_report_includes_issues_in_output() {
    let dir = std::env::temp_dir().join("tddy-validate-unit-issues");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");

    let output = make_validate_output("Summary.", "low");
    let _ = write_validation_report(&dir, &output);

    let content = std::fs::read_to_string(dir.join("validation-report.md"))
        .expect("should be able to read validation-report.md");
    assert!(
        content.contains("Magic number 42") || content.contains("warning"),
        "report should include issue details, got: {}",
        content
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// write_validation_report() includes build_results in the written file.
#[test]
fn write_validation_report_includes_build_results_in_output() {
    let dir = std::env::temp_dir().join("tddy-validate-unit-build");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");

    let output = make_validate_output("Summary.", "low");
    let _ = write_validation_report(&dir, &output);

    let content = std::fs::read_to_string(dir.join("validation-report.md"))
        .expect("should be able to read validation-report.md");
    assert!(
        content.contains("my-crate") || content.contains("pass"),
        "report should include build result info, got: {}",
        content
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Allowlist and changeset tests ───────────────────────────────────────────

/// validate_allowlist() must include Bash(git log *) per PRD requirement 6.
#[test]
fn validate_allowlist_includes_git_log_entry() {
    let allowlist = validate_allowlist();
    assert!(
        allowlist.iter().any(|t| t.contains("git log")),
        "validate_allowlist must include a Bash(git log *) entry per PRD requirement 6 — git log is needed to inspect recent commit history. Got: {:?}",
        allowlist
    );
}

/// validate_allowlist() must include Bash(find *) per PRD requirement 6.
#[test]
fn validate_allowlist_includes_find_entry() {
    let allowlist = validate_allowlist();
    assert!(
        allowlist.iter().any(|t| t.contains("find")),
        "validate_allowlist must include a Bash(find *) entry per PRD requirement 6 — find is needed to locate files in the working directory. Got: {:?}",
        allowlist
    );
}

/// next_goal_for_state("Validated") must return None — validate-changes is not in auto-sequencing.
#[test]
fn next_goal_for_validated_state_returns_none() {
    let result = next_goal_for_state("Validated");
    assert_eq!(
        result,
        None,
        "next_goal_for_state(\"Validated\") must return None — validate-changes is not part of the auto-sequence (Init→plan→acceptance-tests→red→green). Got: {:?}",
        result
    );
}

/// next_goal_for_state("Validating") must return None — validate-changes is not in auto-sequencing.
#[test]
fn next_goal_for_validating_state_returns_none() {
    let result = next_goal_for_state("Validating");
    assert_eq!(
        result,
        None,
        "next_goal_for_state(\"Validating\") must return None — validate-changes is not part of the auto-sequence. Got: {:?}",
        result
    );
}
