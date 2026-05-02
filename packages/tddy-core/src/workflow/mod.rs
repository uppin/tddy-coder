//! Workflow state machine for tddy-coder.

pub mod ids;
pub mod recipe;

pub mod action_cache;

mod agent_output;

pub use agent_output::{
    clear_sinks, get_agent_sink, get_progress_sink, set_agent_sink, set_progress_sink, set_sinks,
};
pub mod context;
pub mod engine;
pub mod goal_conditions;
pub mod graph;
pub mod hooks;
pub mod runner;
pub mod session;
pub mod task;

use crate::error::WorkflowError;
use std::path::{Path, PathBuf};

// Removed: Workflow struct, WorkflowState, and all goal methods (plan, acceptance_tests, red, etc.).
// Execution now uses WorkflowEngine + FlowRunner.

/// Options for invoking any workflow goal (backend-agnostic). Recipes map goal ids to behavior;
/// callers pass the same struct for every step.
#[derive(Debug)]
pub struct GoalOptions {
    pub model: Option<String>,
    pub agent_output: bool,
    pub agent_output_sink: Option<crate::backend::AgentOutputSink>,
    pub conversation_output_path: Option<PathBuf>,
    pub inherit_stdin: bool,
    pub allowed_tools_extras: Option<Vec<String>>,
    pub debug: bool,
}

impl Default for GoalOptions {
    fn default() -> Self {
        Self {
            model: None,
            agent_output: false,
            agent_output_sink: None,
            conversation_output_path: None,
            inherit_stdin: true,
            allowed_tools_extras: None,
            debug: false,
        }
    }
}

// ── Session directory relocation helpers (R1, R2, R4) ───────────────────────

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
#[allow(dead_code)] // Used by relocate_session_dir; will be used when PlanTask implements plan_dir_suggestion
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
fn relocate_session_dir(
    staging: &Path,
    suggestion: &str,
    dir_name: &str,
    output_dir: &Path,
) -> Result<PathBuf, WorkflowError> {
    // R3: Reject empty / whitespace-only suggestions
    let suggestion = suggestion.trim();
    if suggestion.is_empty() {
        log::debug!("[relocate_session_dir] empty suggestion → falling back to staging");
        return Ok(staging.to_path_buf());
    }

    // R3: Reject absolute paths
    if std::path::Path::new(suggestion).is_absolute() {
        log::debug!(
            "[relocate_session_dir] absolute path rejected: {}",
            suggestion
        );
        return Ok(staging.to_path_buf());
    }

    // R3: Reject paths containing `..`
    if suggestion.contains("..") {
        log::debug!(
            "[relocate_session_dir] dotdot path rejected: {}",
            suggestion
        );
        return Ok(staging.to_path_buf());
    }

    // R2: Find the git root relative to the output directory
    let git_root = find_git_root(output_dir);
    log::debug!("[relocate_session_dir] git_root={:?}", git_root);

    // Build the target: git_root / suggestion (stripped trailing slash) / dir_name
    let target = git_root
        .join(suggestion.trim_end_matches('/'))
        .join(dir_name);
    log::debug!(
        "[relocate_session_dir] staging={:?} target={:?}",
        staging,
        target
    );

    // R3: If the suggestion resolves to the same path as staging → no-op
    if target == staging {
        log::debug!("[relocate_session_dir] target == staging → no-op");
        return Ok(staging.to_path_buf());
    }

    // R3: If target already exists → error with a clear message
    if target.exists() {
        return Err(WorkflowError::WriteFailed(format!(
            "relocate_session_dir: target directory already exists: {}",
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
            "[relocate_session_dir] rename failed (cross-device?), falling back to copy+delete"
        );
        copy_dir_recursive(staging, &target)
            .map_err(|e| WorkflowError::WriteFailed(format!("copy staging dir: {}", e)))?;
        std::fs::remove_dir_all(staging).map_err(|e| {
            WorkflowError::WriteFailed(format!("remove staging dir after copy: {}", e))
        })?;
    }

    log::debug!(
        "[relocate_session_dir] relocated {:?} → {:?}",
        staging,
        target
    );
    Ok(target)
}

/// Build the context header string listing absolute paths to existing `.md` artifacts
/// in the session directory, and optionally `repo_dir` (the current working directory for the agent).
///
/// `artifact_basenames` is typically from [`WorkflowRecipe::context_header_session_artifact_filenames`].
/// Returns an empty string when both the session dir and `repo_dir` yield no content.
pub fn build_context_header(
    session_dir: Option<&Path>,
    repo_dir: Option<&Path>,
    artifact_basenames: &[&str],
) -> String {
    let mut lines: Vec<String> = Vec::new();

    if let Some(rd) = repo_dir {
        let canonical = std::fs::canonicalize(rd).unwrap_or_else(|_| rd.to_path_buf());
        lines.push(format!("repo_dir: {}", canonical.display()));
    }

    if let Some(dir) = session_dir {
        log::debug!("[build_context_header] scanning {:?} for artifacts", dir);
        for artifact in artifact_basenames {
            let under_artifacts = dir.join("artifacts").join(artifact);
            let at_root = dir.join(artifact);
            let path = if under_artifacts.exists() {
                log::debug!(
                    "[build_context_header] using {:?} (under session artifacts/)",
                    under_artifacts
                );
                under_artifacts
            } else if at_root.exists() {
                log::debug!(
                    "[build_context_header] using {:?} (legacy session root)",
                    at_root
                );
                at_root
            } else {
                continue;
            };
            let canonical = std::fs::canonicalize(&path).unwrap_or(path);
            log::debug!(
                "[build_context_header] found artifact: {}",
                canonical.display()
            );
            lines.push(format!("{}: {}", artifact, canonical.display()));
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
    session_dir: Option<&Path>,
    repo_dir: Option<&Path>,
    artifact_basenames: &[&str],
) -> String {
    let header = build_context_header(session_dir, repo_dir, artifact_basenames);
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
        fs::write(staging.join("SessionDoc.md"), "# Doc").unwrap();

        let result = relocate_session_dir(&staging, "docs/plans/", dir_name, &output_dir)
            .expect("valid suggestion should succeed");

        let expected = root.join("docs/plans").join(dir_name);
        assert_eq!(
            result, expected,
            "final path should be at suggested location"
        );
        assert!(expected.exists(), "target directory should exist");
        assert!(
            expected.join("SessionDoc.md").exists(),
            "SessionDoc.md should be present at target"
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

        let result = relocate_session_dir(&staging, "/tmp/evil", dir_name, &output_dir)
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

        let result = relocate_session_dir(&staging, "../../outside", dir_name, &output_dir)
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

        let result = relocate_session_dir(&staging, "   ", dir_name, &output_dir)
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

        let result = relocate_session_dir(&staging, "output/", dir_name, &output_dir)
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

    /// Test-only basenames (production passes manifest-derived basenames from the workflow-recipes layer).
    const CTX_TEST_PRIMARY_DOC: &[&str] = &["SessionDoc.md"];
    const CTX_TEST_PRIMARY_DOC_AND_AT: &[&str] = &["SessionDoc.md", "acceptance-tests.md"];
    const CTX_TEST_REFACTOR: &[&str] = &["refactoring-plan.md"];

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
        fs::write(dir.join("SessionDoc.md"), "# Doc").unwrap();

        let header = build_context_header(Some(&dir), None, CTX_TEST_PRIMARY_DOC);

        assert!(
            header.starts_with("**CRITICAL FOR CONTEXT AND SUMMARY**\n"),
            "header must start with the marker line, got: {:?}",
            &header[..header.floor_char_boundary(200)]
        );
        assert!(
            header.contains("SessionDoc.md:"),
            "header must list SessionDoc.md"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC3: missing artifacts are silently omitted ───────────────────────────

    #[test]
    fn test_context_header_omits_missing_files() {
        let dir = temp_dir("omits-missing");
        fs::write(dir.join("SessionDoc.md"), "# Doc").unwrap();
        // acceptance-tests.md is NOT created

        let header = build_context_header(Some(&dir), None, CTX_TEST_PRIMARY_DOC_AND_AT);

        assert!(
            header.contains("SessionDoc.md:"),
            "should list SessionDoc.md"
        );
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

        let header = build_context_header(Some(&dir), None, CTX_TEST_PRIMARY_DOC);

        assert!(
            header.is_empty(),
            "header must be empty when no md files exist, got: {:?}",
            header
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // ── AC2: None session_dir → no header ────────────────────────────────────

    #[test]
    fn test_context_header_empty_for_none_session_dir() {
        let header = build_context_header(None, None, CTX_TEST_PRIMARY_DOC);

        assert!(
            header.is_empty(),
            "header must be empty when session_dir is None"
        );
    }

    // ── AC4: paths are absolute ───────────────────────────────────────────────

    #[test]
    fn test_context_header_uses_absolute_paths() {
        let dir = temp_dir("abs-paths");
        fs::write(dir.join("SessionDoc.md"), "# Doc").unwrap();

        let header = build_context_header(Some(&dir), None, CTX_TEST_PRIMARY_DOC);

        let doc_line = header
            .lines()
            .find(|l| l.starts_with("SessionDoc.md:"))
            .expect("header must contain a SessionDoc.md line");
        let path_str = doc_line.trim_start_matches("SessionDoc.md:").trim();

        assert!(
            std::path::Path::new(path_str).is_absolute(),
            "SessionDoc.md path must be absolute, got: {}",
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
        fs::write(dir.join("SessionDoc.md"), "# Doc").unwrap();

        let original = "Do the task.".to_string();
        let result =
            prepend_context_header(original.clone(), Some(&dir), None, CTX_TEST_PRIMARY_DOC);

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
        fs::write(dir.join("SessionDoc.md"), "# Doc").unwrap();

        let result =
            prepend_context_header("Task.".to_string(), Some(&dir), None, CTX_TEST_PRIMARY_DOC);

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

    // ── AC (R9): context header lists refactoring-plan.md when in the basename list ─

    #[test]
    fn test_context_header_includes_refactoring_plan() {
        let dir = temp_dir("includes-refactoring-plan");
        fs::write(
            dir.join("refactoring-plan.md"),
            "# Refactoring Plan\n## Tasks\n- Rename",
        )
        .unwrap();

        let header = build_context_header(Some(&dir), None, CTX_TEST_REFACTOR);

        assert!(
            header.contains("refactoring-plan.md:"),
            "artifact list must include refactoring-plan.md so it appears in context headers, \
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
        let result =
            prepend_context_header(original.clone(), Some(&dir), None, CTX_TEST_PRIMARY_DOC);

        assert_eq!(
            result, original,
            "prompt must be unchanged when no header is needed"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
