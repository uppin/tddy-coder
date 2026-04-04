//! Structured `analyze` goal: branch/worktree naming (bugfix pipeline).
//!
//! Parse `tddy-tools submit --goal analyze` JSON and merge into [`Changeset`].

use std::path::Path;

use tddy_core::changeset::{read_changeset, update_state, write_changeset, Changeset};
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::task::TaskResult;

use crate::parser::{parse_analyze_response, AnalyzeOutput};

/// System prompt for the bugfix **analyze** step (structured submit, optional `tddy-tools ask`).
pub fn system_prompt() -> String {
    r#"You are a bug triage assistant. Read the user's bug report and derive a short changeset display name (optional), a git branch name, and a worktree directory name — aligned with TDD plan naming semantics.

If you need clarification before submitting, call:
  tddy-tools ask --data '{"questions":[{"header":"<section>","question":"<text>","options":[{"label":"<choice>","description":"<desc>"}],"multiSelect":false}]}'
The call will block until the user answers. The response contains the user's answers.

When ready, submit structured output by calling:
  tddy-tools submit --goal analyze --data-stdin << 'EOF'
<your JSON output>
EOF

Use --data-stdin and a heredoc. Do NOT use --data with inline JSON. Do NOT use Write, cat, or python to build the JSON first — only Bash(tddy-tools *) is auto-approved; other Bash commands require permission.

Run `tddy-tools get-schema analyze` to see the exact JSON shape. Required fields: goal (must be "analyze"), branch_suggestion, worktree_suggestion. Optional: name (changeset display name), summary (short text to merge into the downstream reproduce step context).

**branch_suggestion**: Git branch name (e.g. "bugfix/login-crash", "fix/session-leak").
**worktree_suggestion**: Directory name for `git worktree add` (e.g. "bugfix-login-crash").
**name** (optional): Human-readable title for the changeset.
**summary** (optional): One or two sentences capturing the analyzed issue for reproduce.

**CRITICAL**: You MUST call `tddy-tools submit` with valid JSON. If you do not submit, the workflow cannot continue."#
        .to_string()
}

/// System prompt for the **reproduce** step (demo-friendly; no structured submit required).
pub fn reproduce_system_prompt() -> String {
    r#"You are helping reproduce and fix a bug. Use the bug report and any prior analysis context.

Investigate, add or run tests to reproduce the failure, and document findings in fix-plan.md as appropriate for this project.

This step does not require `tddy-tools submit` unless the workflow configuration says otherwise."#
        .to_string()
}

/// Apply a completed `analyze` task result to `changeset.yaml` under `session_dir`.
pub fn apply_analyze_submit_to_changeset(
    session_dir: &Path,
    task_result: &TaskResult,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::info!(
        "[bugfix analyze] apply_analyze_submit_to_changeset task_id={} response_len={}",
        task_result.task_id,
        task_result.response.len()
    );
    let raw = task_result.response.trim();
    if raw.is_empty() {
        log::debug!("[bugfix analyze] empty task response; cannot persist analyze submit");
        return Err(
            "analyze submit: empty task result (expected JSON from tddy-tools submit)".into(),
        );
    }
    let parsed: AnalyzeOutput = parse_analyze_response(raw)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
    log::debug!(
        "[bugfix analyze] parsed branch_suggestion={:?} worktree_suggestion={:?} name={:?}",
        parsed.branch_suggestion,
        parsed.worktree_suggestion,
        parsed.name
    );
    let mut cs: Changeset =
        read_changeset(session_dir).map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
            format!("read changeset: {}", e).into()
        })?;
    cs.branch_suggestion = Some(parsed.branch_suggestion);
    cs.worktree_suggestion = Some(parsed.worktree_suggestion);
    if let Some(n) = parsed.name {
        cs.name = Some(n);
    }
    if let Some(ref s) = parsed.summary {
        cs.artifacts
            .insert("analyze_summary".to_string(), s.clone());
    }
    update_state(&mut cs, WorkflowState::new("Reproducing"));
    write_changeset(session_dir, &cs)?;
    log::info!(
        "[bugfix analyze] persisted analyze output and set workflow state to Reproducing under {:?}",
        session_dir
    );
    Ok(())
}

/// Seed `changeset.yaml` when missing so the analyze step can read/write `changeset.yaml`
/// (same lifecycle idea as TDD [`crate::tdd::hooks::before_plan`]).
pub(crate) fn seed_bugfix_changeset_if_missing(
    session_dir: &Path,
    feature_input: String,
    repo_path: Option<String>,
) {
    if read_changeset(session_dir).is_ok() {
        return;
    }
    let init_cs = Changeset {
        initial_prompt: Some(feature_input),
        repo_path,
        recipe: Some("bugfix".to_string()),
        ..Changeset::default()
    };
    if let Err(e) = write_changeset(session_dir, &init_cs) {
        log::debug!(
            "[bugfix analyze] seed_bugfix_changeset_if_missing write failed: {}",
            e
        );
    } else {
        log::info!(
            "[bugfix analyze] seeded changeset.yaml (session_dir={:?})",
            session_dir
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tddy_core::workflow::task::{NextAction, TaskResult};
    use uuid::Uuid;

    fn temp_session_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{}-{}-{}",
            name,
            std::process::id(),
            Uuid::now_v7()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        write_changeset(&dir, &Changeset::default()).expect("seed");
        dir
    }

    #[test]
    fn apply_analyze_submit_errors_on_empty_response() {
        let dir = temp_session_dir("bugfix-empty-resp");
        let tr = TaskResult {
            response: String::new(),
            next_action: NextAction::Continue,
            task_id: "analyze".into(),
            status_message: None,
        };
        let err = apply_analyze_submit_to_changeset(&dir, &tr).unwrap_err();
        assert!(
            err.to_string().contains("empty"),
            "expected empty response error, got {:?}",
            err
        );
    }

    #[test]
    fn apply_analyze_submit_errors_on_invalid_json() {
        let dir = temp_session_dir("bugfix-bad-json");
        let tr = TaskResult {
            response: "not json".into(),
            next_action: NextAction::Continue,
            task_id: "analyze".into(),
            status_message: None,
        };
        assert!(apply_analyze_submit_to_changeset(&dir, &tr).is_err());
    }

    #[test]
    fn apply_analyze_submit_persists_summary_artifact() {
        let dir = temp_session_dir("bugfix-summary-art");
        let tr = TaskResult {
            response: r#"{"goal":"analyze","branch_suggestion":"b","worktree_suggestion":"w","summary":"One-line triage"}"#
                .into(),
            next_action: NextAction::Continue,
            task_id: "analyze".into(),
            status_message: None,
        };
        apply_analyze_submit_to_changeset(&dir, &tr).expect("apply");
        let cs = read_changeset(&dir).expect("read");
        assert_eq!(
            cs.artifacts.get("analyze_summary").map(String::as_str),
            Some("One-line triage")
        );
    }
}
