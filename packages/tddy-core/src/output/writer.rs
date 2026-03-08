//! Write planning artifacts to the filesystem.

use crate::error::WorkflowError;
use crate::output::{
    AcceptanceTestsOutput, DemoPlan, EvaluateOutput, GreenOutput, PlanningOutput, RedOutput,
    ValidateOutput,
};
use std::fs;
use std::path::Path;

/// Inject a "Related Documents" section with relative links to peer .md files.
pub fn inject_cross_references(content: &str, plan_dir: &Path, self_name: &str) -> String {
    let mut peers: Vec<String> = fs::read_dir(plan_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if name.ends_with(".md") && name != self_name {
                Some(format!("[{}](./{})", name, name))
            } else {
                None
            }
        })
        .collect();
    peers.sort();
    if peers.is_empty() {
        return content.to_string();
    }
    let mut out = content.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n## Related Documents\n\n");
    for p in &peers {
        out.push_str(&format!("- {}\n", p));
    }
    out
}

/// Generate a directory name from the feature description: YYYY-MM-DD-<slug>.
pub fn slugify_directory_name(feature: &str) -> String {
    let date = format_date_today();
    let slug = slugify(feature, 50);
    format!("{}-{}", date, slug)
}

fn format_date_today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn slugify(s: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(s.len().min(max_len));
    let mut prev_space = false;
    for c in s.chars().take(max_len) {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_lowercase().next().unwrap_or(c));
            prev_space = false;
        } else if (c.is_whitespace() || c == '-' || c == '_') && !prev_space && !out.is_empty() {
            out.push('-');
            prev_space = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Write the session ID to `.session` in the output directory.
pub fn write_session_file(output_dir: &Path, session_id: &str) -> Result<(), WorkflowError> {
    let session_path = output_dir.join(".session");
    fs::write(&session_path, session_id).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Read the session ID from `.session` in the plan directory.
pub fn read_session_file(plan_dir: &Path) -> Result<String, WorkflowError> {
    let session_path = plan_dir.join(".session");
    fs::read_to_string(&session_path).map_err(|e| WorkflowError::SessionMissing(format!("{}", e)))
}

/// Write the implementation session ID to `.impl-session` in the plan directory.
/// Used by the red goal so the green goal can resume the same session.
pub fn write_impl_session_file(plan_dir: &Path, session_id: &str) -> Result<(), WorkflowError> {
    let session_path = plan_dir.join(".impl-session");
    fs::write(&session_path, session_id).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Read the implementation session ID from `.impl-session` in the plan directory.
pub fn read_impl_session_file(plan_dir: &Path) -> Result<String, WorkflowError> {
    let session_path = plan_dir.join(".impl-session");
    fs::read_to_string(&session_path).map_err(|e| WorkflowError::SessionMissing(format!("{}", e)))
}

/// Write PRD.md and TODO.md to the given directory.
/// Injects cross-references to peer documents.
pub fn write_artifacts(output_dir: &Path, planning: &PlanningOutput) -> Result<(), WorkflowError> {
    fs::create_dir_all(output_dir).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

    // Write TODO first so it exists when we inject cross-refs into PRD
    let todo_path = output_dir.join("TODO.md");
    let todo_content = inject_cross_references(&planning.todo, output_dir, "TODO.md");
    fs::write(&todo_path, todo_content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

    let prd_path = output_dir.join("PRD.md");
    let prd_content = inject_cross_references(&planning.prd, output_dir, "PRD.md");
    fs::write(&prd_path, prd_content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

    if let Some(ref demo) = planning.demo_plan {
        write_demo_plan_file(output_dir, demo)?;
    }

    Ok(())
}

/// Write demo-plan.md to the plan directory.
pub fn write_demo_plan_file(plan_dir: &Path, demo: &DemoPlan) -> Result<(), WorkflowError> {
    let mut out = format!(
        "# Demo Plan\n\n## Type\n{}\n\n## Setup\n\n{}\n\n## Steps\n\n",
        demo.demo_type, demo.setup_instructions
    );
    for (i, step) in demo.steps.iter().enumerate() {
        out.push_str(&format!(
            "### Step {}\n\n- **Description**: {}\n- **Action**: {}\n- **Expected**: {}\n\n",
            i + 1,
            step.description,
            step.command_or_action,
            step.expected_result
        ));
    }
    out.push_str(&format!("## Verification\n\n{}\n", demo.verification));
    let content = inject_cross_references(&out, plan_dir, "demo-plan.md");
    let path = plan_dir.join("demo-plan.md");
    fs::write(&path, content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Write demo-results.md to the plan directory.
pub fn write_demo_results_file(
    plan_dir: &Path,
    summary: &str,
    steps_completed: u32,
) -> Result<(), WorkflowError> {
    let content = format!(
        "# Demo Results\n\n## Summary\n\n{}\n\n## Steps Completed\n\n{}\n",
        summary, steps_completed
    );
    let path = plan_dir.join("demo-results.md");
    fs::write(&path, content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Write acceptance-tests.md to the plan directory.
pub fn write_acceptance_tests_file(
    plan_dir: &Path,
    output: &AcceptanceTestsOutput,
) -> Result<(), WorkflowError> {
    let md_path = plan_dir.join("acceptance-tests.md");
    let content = inject_cross_references(&output.to_markdown(), plan_dir, "acceptance-tests.md");
    fs::write(&md_path, content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Write red-output.md to the plan directory.
pub fn write_red_output_file(plan_dir: &Path, output: &RedOutput) -> Result<(), WorkflowError> {
    let md_path = plan_dir.join("red-output.md");
    let content = inject_cross_references(&output.to_markdown(), plan_dir, "red-output.md");
    fs::write(&md_path, content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Write progress.md to the plan directory. Unfilled checkboxes for failed tests and skeletons.
/// Next goal uses this to mark items as done, skipped, or failed.
pub fn write_progress_file(plan_dir: &Path, output: &RedOutput) -> Result<(), WorkflowError> {
    let md_path = plan_dir.join("progress.md");
    let content = output.to_progress_markdown();
    fs::write(&md_path, content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Update progress.md in the plan directory with green goal results.
/// Overwrites with updated checkboxes: [x] for passing, [!] for failing.
pub fn update_progress_file(plan_dir: &Path, output: &GreenOutput) -> Result<(), WorkflowError> {
    let md_path = plan_dir.join("progress.md");
    let content = output.to_updated_progress_markdown();
    fs::write(&md_path, content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Update acceptance-tests.md in the plan directory with green goal results.
/// Replaces "failing" with "passing" for tests that now pass.
pub fn update_acceptance_tests_file(
    plan_dir: &Path,
    output: &GreenOutput,
) -> Result<(), WorkflowError> {
    let md_path = plan_dir.join("acceptance-tests.md");
    if !md_path.exists() {
        return Ok(());
    }
    let content =
        fs::read_to_string(&md_path).map_err(|e| WorkflowError::PlanDirInvalid(e.to_string()))?;
    let updated = output.update_acceptance_tests_content(&content);
    fs::write(&md_path, updated).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Write `validation-report.md` to `working_dir` from a `ValidateOutput`.
///
/// The report contains the summary, risk level, issues, and build results.
pub fn write_validation_report(
    working_dir: &Path,
    output: &ValidateOutput,
) -> Result<(), WorkflowError> {
    let content = output_to_validation_report_md(output);
    let report_path = working_dir.join("validation-report.md");
    fs::write(&report_path, content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

fn output_to_validation_report_md(output: &ValidateOutput) -> String {
    let mut md = String::from("# Validation Report\n\n");
    md.push_str("## Summary\n\n");
    md.push_str(&output.summary);
    md.push_str("\n\n## Risk Level\n\n");
    md.push_str(&output.risk_level);
    md.push_str("\n\n");

    if !output.build_results.is_empty() {
        md.push_str("## Build Results\n\n");
        for b in &output.build_results {
            md.push_str(&format!("- **{}**: {}\n", b.package, b.status));
            if let Some(ref notes) = b.notes {
                if !notes.is_empty() {
                    md.push_str(&format!("  - {}\n", notes));
                }
            }
        }
        md.push('\n');
    }

    if !output.issues.is_empty() {
        md.push_str("## Issues\n\n");
        for i in &output.issues {
            let loc = i
                .line
                .map(|l| format!("{}:{}", i.file, l))
                .unwrap_or_else(|| i.file.clone());
            md.push_str(&format!(
                "- **{}** [{}] {}: {}\n",
                i.severity, i.category, loc, i.description
            ));
            if let Some(ref s) = i.suggestion {
                md.push_str(&format!("  - Suggestion: {}\n", s));
            }
        }
        md.push('\n');
    }

    if let Some(ref sync) = output.changeset_sync {
        md.push_str("## Changeset Sync\n\n");
        md.push_str(&format!(
            "- Status: {}\n- Items updated: {}\n- Items added: {}\n\n",
            sync.status, sync.items_updated, sync.items_added
        ));
    }

    if let Some(ref impact) = output.test_impact {
        md.push_str("## Test Impact\n\n");
        md.push_str(&format!(
            "- Tests affected: {}\n- New tests needed: {}\n",
            impact.tests_affected, impact.new_tests_needed
        ));
    }

    md
}

/// Write evaluation-report.md to plan_dir (not working_dir).
/// Called by the evaluate-changes workflow after a successful backend invocation.
pub fn write_evaluation_report(
    plan_dir: &Path,
    output: &EvaluateOutput,
) -> Result<(), WorkflowError> {
    eprintln!(
        r#"{{"tddy":{{"marker_id":"M015","scope":"output::writer::write_evaluation_report","data":{{}}}}}}"#
    );

    let mut md = String::new();
    md.push_str("# Evaluation Report\n\n");

    md.push_str("## Summary\n\n");
    md.push_str(&output.summary);
    md.push_str("\n\n");

    md.push_str("## Risk Level\n\n");
    md.push_str(&output.risk_level);
    md.push_str("\n\n");

    if !output.changed_files.is_empty() {
        md.push_str("## Changed Files\n\n");
        for f in &output.changed_files {
            md.push_str(&format!(
                "- {} ({}, +{}/−{})\n",
                f.path, f.change_type, f.lines_added, f.lines_removed
            ));
        }
        md.push('\n');
    }

    if !output.affected_tests.is_empty() {
        md.push_str("## Affected Tests\n\n");
        for t in &output.affected_tests {
            md.push_str(&format!("- {}: {}\n", t.path, t.status));
            if !t.description.is_empty() {
                md.push_str(&format!("  {}\n", t.description));
            }
        }
        md.push('\n');
    }

    if !output.validity_assessment.is_empty() {
        md.push_str("## Validity Assessment\n\n");
        md.push_str(&output.validity_assessment);
        md.push_str("\n\n");
    }

    if !output.build_results.is_empty() {
        md.push_str("## Build Results\n\n");
        for b in &output.build_results {
            let notes = b.notes.as_deref().unwrap_or("");
            md.push_str(&format!(
                "- {}: {}{}\n",
                b.package,
                b.status,
                if notes.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", notes)
                }
            ));
        }
        md.push('\n');
    }

    if !output.issues.is_empty() {
        md.push_str("## Issues\n\n");
        for i in &output.issues {
            let loc = i.line.map(|l| format!(":{}", l)).unwrap_or_default();
            md.push_str(&format!(
                "- [{}/{}] {}{}: {}\n",
                i.severity, i.category, i.file, loc, i.description
            ));
            if let Some(ref sug) = i.suggestion {
                md.push_str(&format!("  Suggestion: {}\n", sug));
            }
        }
        md.push('\n');
    }

    eprintln!(
        "[tddy-core] write_evaluation_report: writing {} bytes to {}",
        md.len(),
        plan_dir.join("evaluation-report.md").display()
    );

    fs::write(plan_dir.join("evaluation-report.md"), md)
        .map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

    Ok(())
}
