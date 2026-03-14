//! Workflow state machine for tddy-coder.

mod acceptance_tests;
mod agent_output;
pub mod context;
pub mod engine;
mod evaluate;
pub mod graph;
mod green;
pub mod hooks;
mod planning;
mod red;
mod refactor;
pub mod runner;
pub mod session;
pub mod steps;
pub mod task;
pub mod tdd_graph;
pub mod tdd_hooks;
mod update_docs;
mod validate_subagents;

use crate::error::WorkflowError;
use std::path::{Path, PathBuf};

// Removed: Workflow struct, WorkflowState, and all goal methods (plan, acceptance_tests, red, etc.).
// Execution now uses WorkflowEngine + FlowRunner. Options structs retained for API compatibility.

/// Options for the plan step.
#[derive(Debug, Default)]
pub struct PlanOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the acceptance-tests step.
#[derive(Debug, Default)]
pub struct AcceptanceTestsOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the red step.
#[derive(Debug, Default)]
pub struct RedOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the green step.
#[derive(Debug)]
pub struct GreenOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

impl Default for GreenOptions {
    fn default() -> Self {
        Self {
            model: None,
            agent_output: true,
            agent_output_sink: None,
            conversation_output_path: None,
            inherit_stdin: true,
            allowed_tools_extras: None,
            debug: false,
        }
    }
}

/// Options for the standalone demo step.
#[derive(Debug, Default)]
pub struct DemoOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the evaluate-changes step.
#[derive(Debug, Default)]
pub struct EvaluateOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the update-docs step.
#[derive(Debug, Default)]
pub struct UpdateDocsOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the refactor step.
#[derive(Debug, Default)]
pub struct RefactorOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

/// Options for the validate step (subagent-based).
#[derive(Debug, Default)]
pub struct ValidateOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

// ── Plan directory relocation helpers (R1, R2, R4) ───────────────────────────

/// Walk up from `dir` looking for a `.git` directory.
/// Falls back to `dir`'s parent if none found (or to `dir` itself if it has no parent).
#[allow(dead_code)] // Used by relocation_tests; will be used when PlanTask implements plan_dir_suggestion
pub fn find_git_root(dir: &Path) -> PathBuf {
    let mut current = dir.to_path_buf();
    loop {
        if current.join(".git").exists() {
            log::debug!("[find_git_root] found .git at {:?}", current);
            return current;
        }
        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            }
            _ => break,
        }
    }
    // R2 fallback: return dir's immediate parent (or dir itself if no parent)
    log::debug!(
        "[find_git_root] no .git found, falling back to parent of {:?}",
        dir
    );
    dir.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dir.to_path_buf())
}

/// Recursively copy `src` directory to `dst`. Used for cross-device moves (R4).
#[allow(dead_code)] // Used by relocate_plan_dir; will be used when PlanTask implements plan_dir_suggestion
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Relocate the plan directory from the staging location to the path suggested by the agent.
///
/// # Arguments
/// * `staging` – current (staging) path of the plan directory
/// * `suggestion` – raw `plan_dir_suggestion` string from the agent
/// * `dir_name` – the bare directory name (e.g. `"2026-03-08-my-feature"`)
/// * `output_dir` – the original output directory (used to find the git root)
///
/// Returns the final path.  On any invalid suggestion the function falls back to `staging`
/// and returns `Ok(staging.to_path_buf())` — it never returns `Err` for validation failures.
#[allow(dead_code)] // Used by relocation_tests; will be used when PlanTask implements plan_dir_suggestion
fn relocate_plan_dir(
    staging: &Path,
    suggestion: &str,
    dir_name: &str,
    output_dir: &Path,
) -> Result<PathBuf, WorkflowError> {
    // R3: Reject empty / whitespace-only suggestions
    let suggestion = suggestion.trim();
    if suggestion.is_empty() {
        log::debug!("[relocate_plan_dir] empty suggestion → falling back to staging");
        return Ok(staging.to_path_buf());
    }

    // R3: Reject absolute paths
    if std::path::Path::new(suggestion).is_absolute() {
        log::debug!("[relocate_plan_dir] absolute path rejected: {}", suggestion);
        return Ok(staging.to_path_buf());
    }

    // R3: Reject paths containing `..`
    if suggestion.contains("..") {
        log::debug!("[relocate_plan_dir] dotdot path rejected: {}", suggestion);
        return Ok(staging.to_path_buf());
    }

    // R2: Find the git root relative to the output directory
    let git_root = find_git_root(output_dir);
    log::debug!("[relocate_plan_dir] git_root={:?}", git_root);

    // Build the target: git_root / suggestion (stripped trailing slash) / dir_name
    let target = git_root
        .join(suggestion.trim_end_matches('/'))
        .join(dir_name);
    log::debug!(
        "[relocate_plan_dir] staging={:?} target={:?}",
        staging,
        target
    );

    // R3: If the suggestion resolves to the same path as staging → no-op
    if target == staging {
        log::debug!("[relocate_plan_dir] target == staging → no-op");
        return Ok(staging.to_path_buf());
    }

    // R3: If target already exists → error with a clear message
    if target.exists() {
        return Err(WorkflowError::WriteFailed(format!(
            "relocate_plan_dir: target directory already exists: {}",
            target.display()
        )));
    }

    // Create parent directories for the target
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| WorkflowError::WriteFailed(format!("create target parent dirs: {}", e)))?;
    }

    // R4: Try a fast rename first; on cross-device failure fall back to copy+delete
    if std::fs::rename(staging, &target).is_err() {
        log::debug!(
            "[relocate_plan_dir] rename failed (cross-device?), falling back to copy+delete"
        );
        copy_dir_recursive(staging, &target)
            .map_err(|e| WorkflowError::WriteFailed(format!("copy staging dir: {}", e)))?;
        std::fs::remove_dir_all(staging).map_err(|e| {
            WorkflowError::WriteFailed(format!("remove staging dir after copy: {}", e))
        })?;
    }

    log::debug!("[relocate_plan_dir] relocated {:?} → {:?}", staging, target);
    Ok(target)
}

/// Known artifact filenames to include in the context header.
const KNOWN_ARTIFACTS: &[&str] = &[
    "PRD.md",
    "acceptance-tests.md",
    "progress.md",
    "evaluation-report.md",
    "demo-plan.md",
    "validate-tests-report.md",
    "validate-prod-ready-report.md",
    "analyze-clean-code-report.md",
    "refactoring-plan.md",
];

/// Build the context header string listing absolute paths to existing `.md` artifacts
/// in `plan_dir`, and optionally `repo_dir` (the current working directory for the agent).
/// Returns an empty string when both `plan_dir` and `repo_dir` yield no content.
pub fn build_context_header(plan_dir: Option<&Path>, repo_dir: Option<&Path>) -> String {
    let mut lines: Vec<String> = Vec::new();

    if let Some(rd) = repo_dir {
        let canonical = std::fs::canonicalize(rd).unwrap_or_else(|_| rd.to_path_buf());
        lines.push(format!("repo_dir: {}", canonical.display()));
    }

    if let Some(dir) = plan_dir {
        log::debug!("[build_context_header] scanning {:?} for artifacts", dir);
        for artifact in KNOWN_ARTIFACTS {
            let path = dir.join(artifact);
            if path.exists() {
                let canonical = std::fs::canonicalize(&path).unwrap_or(path);
                log::debug!(
                    "[build_context_header] found artifact: {}",
                    canonical.display()
                );
                lines.push(format!("{}: {}", artifact, canonical.display()));
            }
        }
    }

    if lines.is_empty() {
        log::debug!("[build_context_header] no content — empty header");
        return String::new();
    }

    let mut header = String::from("**CRITICAL FOR CONTEXT AND SUMMARY**\n");
    for line in &lines {
        header.push_str(line);
        header.push('\n');
    }
    log::debug!(
        "[build_context_header] built header with {} artifact(s)",
        lines.len()
    );
    header
}

/// Prepend the context header to `prompt`. When the header is empty, returns `prompt` unchanged.
pub fn prepend_context_header(
    prompt: String,
    plan_dir: Option<&Path>,
    repo_dir: Option<&Path>,
) -> String {
    let header = build_context_header(plan_dir, repo_dir);
    if header.is_empty() {
        log::debug!("[prepend_context_header] no header — prompt unchanged");
        return prompt;
    }
    log::debug!("[prepend_context_header] prepending context header to prompt");
    format!(
        "<context-reminder>\n{}</context-reminder>\n\n{}",
        header, prompt
    )
}

#[cfg(test)]
mod relocation_tests {
    use super::*;
    use std::fs;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tddy-wr-{}", label));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ── R4: valid suggestion moves directory ─────────────────────────────────

    #[test]
    fn test_relocate_valid_suggestion() {
        let root = temp_dir("relocate-valid");
        fs::create_dir_all(root.join(".git")).unwrap();

        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let dir_name = "2026-03-08-my-feature";
        let staging = output_dir.join(dir_name);
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("PRD.md"), "# PRD").unwrap();

        let result = relocate_plan_dir(&staging, "docs/plans/", dir_name, &output_dir)
            .expect("valid suggestion should succeed");

        let expected = root.join("docs/plans").join(dir_name);
        assert_eq!(
            result, expected,
            "final path should be at suggested location"
        );
        assert!(expected.exists(), "target directory should exist");
        assert!(
            expected.join("PRD.md").exists(),
            "PRD.md should be present at target"
        );

        let _ = fs::remove_dir_all(&root);
    }

    // ── R3: absolute-path suggestion falls back silently ──────────────────────

    #[test]
    fn test_relocate_invalid_absolute_path() {
        let root = temp_dir("relocate-absolute");
        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let dir_name = "2026-03-08-my-feature";
        let staging = output_dir.join(dir_name);
        fs::create_dir_all(&staging).unwrap();

        let result = relocate_plan_dir(&staging, "/tmp/evil", dir_name, &output_dir)
            .expect("absolute path should fall back, not error");

        assert_eq!(result, staging, "should fall back to staging path");

        let _ = fs::remove_dir_all(&root);
    }

    // ── R3: path traversal (dotdot) rejected ─────────────────────────────────

    #[test]
    fn test_relocate_dotdot_rejected() {
        let root = temp_dir("relocate-dotdot");
        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let dir_name = "2026-03-08-my-feature";
        let staging = output_dir.join(dir_name);
        fs::create_dir_all(&staging).unwrap();

        let result = relocate_plan_dir(&staging, "../../outside", dir_name, &output_dir)
            .expect("dotdot path should fall back, not error");

        assert_eq!(result, staging, "dotdot path should fall back to staging");

        let _ = fs::remove_dir_all(&root);
    }

    // ── R3: empty / whitespace suggestion falls back ──────────────────────────

    #[test]
    fn test_relocate_empty_suggestion() {
        let root = temp_dir("relocate-empty");
        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let dir_name = "2026-03-08-my-feature";
        let staging = output_dir.join(dir_name);
        fs::create_dir_all(&staging).unwrap();

        let result = relocate_plan_dir(&staging, "   ", dir_name, &output_dir)
            .expect("whitespace suggestion should fall back, not error");

        assert_eq!(
            result, staging,
            "whitespace-only suggestion should fall back"
        );

        let _ = fs::remove_dir_all(&root);
    }

    // ── R3: suggestion resolves to same path → no-op ──────────────────────────

    #[test]
    fn test_relocate_same_path_noop() {
        let root = temp_dir("relocate-same");
        // No .git here → find_git_root falls back to output_dir.parent() == root.
        // Suggestion "output/" → root / "output" / dir_name == staging → noop.
        let output_dir = root.join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let dir_name = "2026-03-08-my-feature";
        let staging = output_dir.join(dir_name);
        fs::create_dir_all(&staging).unwrap();

        let result = relocate_plan_dir(&staging, "output/", dir_name, &output_dir)
            .expect("same-path suggestion should be a noop, not error");

        assert_eq!(result, staging, "same-path should return staging unchanged");
        assert!(
            staging.is_dir(),
            "staging directory should still exist as a real dir"
        );

        let _ = fs::remove_dir_all(&root);
    }

    // ── R2: find_git_root locates .git ancestor ───────────────────────────────

    #[test]
    fn test_find_git_root_finds_dot_git() {
        let root = temp_dir("git-root-find");
        fs::create_dir_all(root.join(".git")).unwrap();
        let nested = root.join("a/b/c");
        fs::create_dir_all(&nested).unwrap();

        let found = find_git_root(&nested);

        assert_eq!(found, root, "should return the ancestor that contains .git");

        let _ = fs::remove_dir_all(&root);
    }

    // ── R2: find_git_root falls back to parent when no .git found ─────────────

    #[test]
    fn test_find_git_root_fallback() {
        let root = temp_dir("git-root-fallback");
        // `root` has no .git; temp dirs on supported platforms are outside any
        // git repo, so walking up from `nested` will not find one.
        let nested = root.join("a");
        fs::create_dir_all(&nested).unwrap();

        let found = find_git_root(&nested);

        // Must not return `nested` itself — always walks at least one level up.
        assert_ne!(found, nested, "must not return the input directory itself");
        assert!(found.is_absolute(), "result must be an absolute path");
        assert!(found.is_dir(), "result must be an existing directory");

        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod context_header_tests {
    use super::*;
    use std::fs;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tddy-ch-{}", label));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ── AC1: header lists existing .md files ─────────────────────────────────

    #[test]
    fn test_context_header_lists_existing_md_files() {
        let dir = temp_dir("lists-existing");
        fs::write(dir.join("PRD.md"), "# PRD").unwrap();

        let header = build_context_header(Some(&dir), None);

        assert!(
            header.starts_with("**CRITICAL FOR CONTEXT AND SUMMARY**\n"),
            "header must start with the marker line, got: {:?}",
            &header[..header.len().min(200)]
        );
        assert!(header.contains("PRD.md:"), "header must list PRD.md");

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC3: missing artifacts are silently omitted ───────────────────────────

    #[test]
    fn test_context_header_omits_missing_files() {
        let dir = temp_dir("omits-missing");
        fs::write(dir.join("PRD.md"), "# PRD").unwrap();
        // acceptance-tests.md is NOT created

        let header = build_context_header(Some(&dir), None);

        assert!(header.contains("PRD.md:"), "should list PRD.md");
        assert!(
            !header.contains("acceptance-tests.md:"),
            "must not list missing acceptance-tests.md"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC2: empty plan directory → no header ────────────────────────────────

    #[test]
    fn test_context_header_empty_for_no_md_files() {
        let dir = temp_dir("empty-dir");
        // No .md files

        let header = build_context_header(Some(&dir), None);

        assert!(
            header.is_empty(),
            "header must be empty when no md files exist, got: {:?}",
            header
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC2: None plan_dir → no header ───────────────────────────────────────

    #[test]
    fn test_context_header_empty_for_none_plan_dir() {
        let header = build_context_header(None, None);

        assert!(
            header.is_empty(),
            "header must be empty when plan_dir is None"
        );
    }

    // ── AC4: paths are absolute ───────────────────────────────────────────────

    #[test]
    fn test_context_header_uses_absolute_paths() {
        let dir = temp_dir("abs-paths");
        fs::write(dir.join("PRD.md"), "# PRD").unwrap();

        let header = build_context_header(Some(&dir), None);

        let prd_line = header
            .lines()
            .find(|l| l.starts_with("PRD.md:"))
            .expect("header must contain a PRD.md line");
        let path_str = prd_line.trim_start_matches("PRD.md:").trim();

        assert!(
            std::path::Path::new(path_str).is_absolute(),
            "PRD.md path must be absolute, got: {}",
            path_str
        );
        assert!(
            std::path::Path::new(path_str).exists(),
            "listed path must exist on disk: {}",
            path_str
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC6: original prompt appears after header ─────────────────────────────

    #[test]
    fn test_prepend_adds_header_before_prompt() {
        let dir = temp_dir("prepend-adds");
        fs::write(dir.join("PRD.md"), "# PRD").unwrap();

        let original = "Do the task.".to_string();
        let result = prepend_context_header(original.clone(), Some(&dir), None);

        assert!(
            result.starts_with("<context-reminder>"),
            "result must start with context-reminder tag"
        );
        assert!(
            result.contains("**CRITICAL FOR CONTEXT AND SUMMARY**"),
            "result must contain header marker inside context-reminder"
        );
        assert!(
            result.contains("</context-reminder>"),
            "result must contain closing context-reminder tag"
        );

        let close_tag = "</context-reminder>";
        let close_pos = result.find(close_tag).expect("must find closing tag");
        let after_tag = &result[close_pos + close_tag.len()..];
        let body = after_tag.trim_start_matches('\n');
        assert_eq!(
            body, original,
            "original prompt must appear verbatim after the context-reminder block"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── context-reminder tags wrap the header block ────────────────────────────

    #[test]
    fn test_prepend_wraps_header_in_context_reminder_tags() {
        let dir = temp_dir("wrap-tags");
        fs::write(dir.join("PRD.md"), "# PRD").unwrap();

        let result = prepend_context_header("Task.".to_string(), Some(&dir), None);

        assert!(
            result.starts_with("<context-reminder>\n"),
            "header block must start with <context-reminder> and newline"
        );
        let inner_start = "<context-reminder>\n".len();
        let inner = &result[inner_start..];
        assert!(
            inner.starts_with("**CRITICAL FOR CONTEXT AND SUMMARY**"),
            "first line inside tags must be the marker"
        );
        assert!(
            result.contains("\n</context-reminder>\n"),
            "closing tag must be followed by newline before prompt body"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC (R9): KNOWN_ARTIFACTS includes refactoring-plan.md ───────────────

    #[test]
    fn test_context_header_includes_refactoring_plan() {
        let dir = temp_dir("includes-refactoring-plan");
        fs::write(
            dir.join("refactoring-plan.md"),
            "# Refactoring Plan\n## Tasks\n- Rename",
        )
        .unwrap();

        let header = build_context_header(Some(&dir), None);

        assert!(
            header.contains("refactoring-plan.md:"),
            "KNOWN_ARTIFACTS must include refactoring-plan.md so it appears in context headers, \
             got header: {:?}",
            header
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC7: no-op when header is empty ──────────────────────────────────────

    #[test]
    fn test_prepend_returns_original_when_no_header() {
        let dir = temp_dir("prepend-noop");
        // No .md files → build_context_header returns ""

        let original = "Do the task.".to_string();
        let result = prepend_context_header(original.clone(), Some(&dir), None);

        assert_eq!(
            result, original,
            "prompt must be unchanged when no header is needed"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
