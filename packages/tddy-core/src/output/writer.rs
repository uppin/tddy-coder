//! Write planning artifacts to the filesystem.

use crate::error::WorkflowError;
use crate::output::{AcceptanceTestsOutput, PlanningOutput, RedOutput};
use std::fs;
use std::path::Path;

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

/// Write PRD.md and TODO.md to the given directory.
pub fn write_artifacts(output_dir: &Path, planning: &PlanningOutput) -> Result<(), WorkflowError> {
    fs::create_dir_all(output_dir).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

    let prd_path = output_dir.join("PRD.md");
    fs::write(&prd_path, &planning.prd).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

    let todo_path = output_dir.join("TODO.md");
    fs::write(&todo_path, &planning.todo).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

    Ok(())
}

/// Write acceptance-tests.md to the plan directory.
pub fn write_acceptance_tests_file(
    plan_dir: &Path,
    output: &AcceptanceTestsOutput,
) -> Result<(), WorkflowError> {
    let md_path = plan_dir.join("acceptance-tests.md");
    let content = output.to_markdown();
    fs::write(&md_path, content).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;
    Ok(())
}

/// Write red-output.md to the plan directory.
pub fn write_red_output_file(plan_dir: &Path, output: &RedOutput) -> Result<(), WorkflowError> {
    let md_path = plan_dir.join("red-output.md");
    let content = output.to_markdown();
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
