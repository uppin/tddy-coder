//! Write planning artifacts to the filesystem.

use crate::error::WorkflowError;
use crate::output::PlanningOutput;
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

/// Write PRD.md and TODO.md to the given directory.
pub fn write_artifacts(output_dir: &Path, planning: &PlanningOutput) -> Result<(), WorkflowError> {
    fs::create_dir_all(output_dir).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

    let prd_path = output_dir.join("PRD.md");
    fs::write(&prd_path, &planning.prd).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

    let todo_path = output_dir.join("TODO.md");
    fs::write(&todo_path, &planning.todo).map_err(|e| WorkflowError::WriteFailed(e.to_string()))?;

    Ok(())
}
