//! Bugfix **interview** step — elicitation before **analyze** (TDD-aligned relay + workflow fields).
//!
//! Persists agent/interview text to **`.workflow/bugfix_interview_handoff.txt`** so **analyze** can
//! merge it after hooks clear **`answers`** (same relay pattern as [`crate::tdd::interview`]).

use std::fs;
use std::path::{Path, PathBuf};

use tddy_core::backend::{ClarificationQuestion, QuestionOption};
use tddy_core::workflow::context::Context;
use tddy_core::GoalId;

/// Relay path under the session dir (PRD / acceptance: `.workflow/bugfix_interview_handoff.txt`).
pub const BUGFIX_INTERVIEW_HANDOFF_RELATIVE: &str = ".workflow/bugfix_interview_handoff.txt";

#[must_use]
pub fn handoff_path(session_dir: &Path) -> PathBuf {
    session_dir.join(BUGFIX_INTERVIEW_HANDOFF_RELATIVE)
}

/// System prompt for bugfix **interview** — socket-backed **`tddy-tools ask`**, demo routing, **`changeset.yaml`** **`workflow`**.
#[must_use]
pub fn system_prompt() -> String {
    log::debug!(
        target: "tddy_workflow_recipes::bugfix::interview",
        "system_prompt: building bugfix interview system prompt"
    );
    r#"You are running the bugfix workflow **interview** phase — elicit focused clarification about the bug report before **analyze** triage and **reproduce**.

Before **analyze** completes, you **must** surface whether to run an optional **demo** after the fix path (post-**reproduce** / future demo step) and **how** (options). Persist those answers for workflow routing: **`run_optional_step_x`** controls the optional demo branch, **`demo_options`** records how the demo should be done, and **`tool_schema_id`** (when applicable) ties the block to the **`changeset-workflow`** JSON Schema. Prefer **`tddy-tools persist-changeset-workflow`** (or documented follow-up steps) to write **`workflow.run_optional_step_x`**, **`workflow.demo_options`**, and **`workflow.tool_schema_id`** into **`changeset.yaml`** so resume and **`merge_persisted_workflow_into_context`** match operator intent.

## Elicitation (mandatory)

1. Every interactive clarification **must** be asked through **`tddy-tools ask`** so **tddy-tui** (or the web-attached terminal) can show questions and collect answers. The session sets **`TDDY_SOCKET`** for the agent process; run the CLI from a shell (e.g. **Bash**) so the tool connects to the host.

   **Command shape** (escape JSON for your shell):

   ```text
   tddy-tools ask --data '{"questions":[{"header":"Short title","question":"Full question text?","options":[{"label":"Option A","description":"What A means"},{"label":"Option B","description":"What B means"}],"multiSelect":false}]}'
   ```

   JSON rules: top-level **`questions`** array; each item has **`header`**, **`question`**, **`options`** (**`label`**, **`description`**), **`multiSelect`**. Add **`allowOther`** when free-text is needed.

2. **Do not** satisfy interview elicitation by only writing questions in your assistant message (markdown lists, numbered Q&A, or pasted JSON). That **does not** invoke **`tddy-tools ask`** and **will not** appear as elicitation in the TUI.

3. Ask in small batches; avoid unnecessary questions.

## Not in this phase

**Do not** run full **analyze** triage (branch/worktree **`tddy-tools submit`**) here. When interview clarification for this goal is complete, stop issuing **`tddy-tools ask`** for this interview turn so the workflow can proceed to **analyze**."#
        .to_string()
}

/// User-facing interview prompt for the bug report text.
#[must_use]
pub fn build_interview_user_prompt(feature_input: &str) -> String {
    log::debug!(
        target: "tddy_workflow_recipes::bugfix::interview",
        "build_interview_user_prompt: feature_input_len={}",
        feature_input.len()
    );
    format!(
        "Clarify this bug report before **analyze** triage. Include a **demo** yes/no (optional post-fix demo) and **demo_options** (how to run it) via **tddy-tools ask**, and persist **run_optional_step_x** / **demo_options** / **tool_schema_id** into **changeset.yaml** using **tddy-tools persist-changeset-workflow** so downstream goals see the same **`workflow`** block:\n\n{}",
        feature_input.trim()
    )
}

/// Build the follow-up prompt after clarification answers (same marker pattern as TDD interview).
#[must_use]
pub fn build_followup_prompt(feature_input: &str, answers: &str) -> String {
    format!(
        r#"Here are the user's answers to your questions:

{answers}

Continue the interview for: {feature}"#,
        answers = answers.trim(),
        feature = feature_input.trim(),
    )
}

/// Write relay file so **analyze** can recover content after `after_task` clears **`answers`** / **`prompt`**.
pub fn persist_bugfix_interview_handoff_for_analyze(
    session_dir: &Path,
    handoff_text: &str,
) -> std::io::Result<()> {
    let path = handoff_path(session_dir);
    log::info!(
        target: "tddy_workflow_recipes::bugfix::interview",
        "persist_bugfix_interview_handoff_for_analyze: writing {} bytes to {:?}",
        handoff_text.len(),
        path
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, handoff_text)
}

/// Merge relay file into **`prompt`** (and visible **analyze** context) before the **analyze** system prompt runs.
pub fn apply_bugfix_interview_handoff_to_analyze_context(
    session_dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = handoff_path(session_dir);
    if !path.exists() {
        log::debug!(
            target: "tddy_workflow_recipes::bugfix::interview",
            "apply_bugfix_interview_handoff_to_analyze_context: no relay at {:?}",
            path
        );
        return Ok(());
    }
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("read bugfix interview handoff {}: {}", path.display(), e))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        log::debug!(
            target: "tddy_workflow_recipes::bugfix::interview",
            "apply_bugfix_interview_handoff_to_analyze_context: empty relay at {:?}",
            path
        );
        return Ok(());
    }
    let base = context
        .get_sync::<String>("prompt")
        .or_else(|| context.get_sync("feature_input"))
        .unwrap_or_default();
    let merged = if base.trim().is_empty() {
        format!("**Bug clarification (interview relay):**\n{}", trimmed)
    } else {
        format!(
            "**Bug clarification (interview relay):**\n{}\n\n---\n\n{}",
            trimmed,
            base.trim()
        )
    };
    log::info!(
        target: "tddy_workflow_recipes::bugfix::interview",
        "apply_bugfix_interview_handoff_to_analyze_context: merged relay ({} bytes) into analyze prompt (base len {})",
        trimmed.len(),
        base.len()
    );
    context.set_sync("prompt", merged);
    Ok(())
}

/// True when staged agent **`output`** looks like **markdown-numbered questions** (weak proxy for
/// “model asked in prose instead of **`tddy-tools ask`**”).
///
/// Requires **at least two** `N.` lines whose remainder contains **`?`**, so plain numbered steps
/// (e.g. `1. npm install` / `2. cargo build`) do not trigger host recovery.
fn prose_numbered_clarification_probe(text: &str) -> bool {
    let mut numbered_question_lines = 0usize;
    for line in text.lines() {
        let s = line.trim_start();
        if let Some(dot) = s.find('.') {
            let prefix = &s[..dot];
            if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let after = s.get(dot + 1..).map(str::trim_start).unwrap_or("");
            if after.contains('?') {
                numbered_question_lines += 1;
            }
        }
    }
    let hit = numbered_question_lines >= 2;
    log::debug!(
        target: "tddy_workflow_recipes::bugfix::interview",
        "prose_numbered_clarification_probe: numbered_question_lines={} hit={}",
        numbered_question_lines,
        hit
    );
    hit
}

/// After a no-submit **interview** turn, surface a host **`tddy-tools ask`**-shaped batch when prose elicitation was detected.
///
/// Full **TDD**-style merge of recovery answers into **`changeset.yaml`** **`workflow`** (beyond this gate) is not implemented here; agents still use **`tddy-tools persist-changeset-workflow`** per interview prompts.
///
/// Aligns with **`BackendInvokeTask`** staging **`output`** on **`Context`** before
/// [`tddy_core::workflow::recipe::WorkflowRecipe::host_clarification_gate_after_no_submit_turn`].
#[must_use]
pub fn host_gate_bugfix_interview_recovery_after_no_submit(
    goal_id: &GoalId,
    context: &Context,
) -> Option<Vec<ClarificationQuestion>> {
    if goal_id.as_str() != "interview" {
        return None;
    }
    let output = context.get_sync::<String>("output").unwrap_or_default();
    if !prose_numbered_clarification_probe(&output) {
        log::debug!(
            target: "tddy_workflow_recipes::bugfix::interview",
            "host_gate_bugfix_interview_recovery_after_no_submit: no prose probe match for interview"
        );
        return None;
    }
    log::info!(
        target: "tddy_workflow_recipes::bugfix::interview",
        "host_gate_bugfix_interview_recovery_after_no_submit: surfacing recovery ClarificationQuestion batch for bugfix interview"
    );
    Some(vec![ClarificationQuestion {
        header: "Bugfix interview".to_string(),
        question: "The prior turn listed clarification in plain text instead of invoking **tddy-tools ask** through **TDDY_SOCKET**. Use **tddy-tools ask** for the next batch so answers flow through the host relay; then pick how to proceed."
            .to_string(),
        options: vec![
            QuestionOption {
                label: "I'll use tddy-tools ask next".to_string(),
                description: "Continue interview after asking via the socket-backed tool.".to_string(),
            },
            QuestionOption {
                label: "Continue without recovery".to_string(),
                description: "Only if answers were already captured through ask or persisted elsewhere.".to_string(),
            },
        ],
        multi_select: false,
        allow_other: false,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_probe_detects_numbered_questions() {
        let s = "1. What is the expected behavior?\n2. Which version fails?";
        assert!(prose_numbered_clarification_probe(s));
        assert!(!prose_numbered_clarification_probe("no questions here"));
    }

    #[test]
    fn prose_probe_rejects_numbered_non_questions() {
        let steps = "1. Clone the repo\n2. Run the failing test\n3. Capture logs";
        assert!(
            !prose_numbered_clarification_probe(steps),
            "numbered how-to steps without `?` must not trigger interview recovery"
        );
    }

    #[test]
    fn host_gate_skips_plain_numbered_steps() {
        let ctx = Context::new();
        ctx.set_sync("output", "1. Setup\n2. Build");
        assert!(host_gate_bugfix_interview_recovery_after_no_submit(
            &GoalId::new("interview"),
            &ctx
        )
        .is_none());
    }
}
