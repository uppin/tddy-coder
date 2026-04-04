//! TDD **interview** step — elicitation before **plan** (grill-me–aligned handoff).
//!
//! Persists user/agent text to a relay file under `.workflow/` so **plan** can reload it after
//! hooks clear `answers` following the interview task.
//!
//! **Terminology:** Same relay pattern as [`grill_me`](crate::grill_me) (e.g. persisted answers
//! under the session tree); see `grill_me::hooks` for the grill-phase counterpart.

use std::fs;
use std::path::{Path, PathBuf};

use tddy_core::workflow::context::Context;

/// Relay path under the session dir (same pattern as `.workflow/grill_ask_answers.txt`).
pub const INTERVIEW_HANDOFF_RELATIVE: &str = ".workflow/tdd_interview_handoff.txt";

pub fn interview_handoff_path(session_dir: &Path) -> PathBuf {
    session_dir.join(INTERVIEW_HANDOFF_RELATIVE)
}

pub fn system_prompt() -> String {
    log::debug!(
        target: "tddy_workflow_recipes::tdd::interview",
        "system_prompt: building interview system prompt"
    );
    r#"You are running the TDD workflow **interview** phase — elicit focused clarification about the feature before a PRD/plan is written.

Before planning completes, you **must** surface the optional **demo** phase: use **tddy-tools ask** so the user decides (no silent default) whether to run the **demo** goal after **green**, and how (e.g. script-based vs manual, scope). Persist those answers for the workflow graph: the session/context key **run_optional_step_x** controls the post-green branch, and **demo_options** records how the demo should be done. Prefer **tddy-tools persist-changeset-workflow** (or follow-up plan steps) to write `workflow.run_optional_step_x`, `workflow.demo_options`, and `workflow.tool_schema_id` into **changeset.yaml** so resume and graph routing match this interview.

Prefer **tddy-tools ask** for interactive questions when appropriate. Do not write PRD.md in this phase."#
        .to_string()
}

pub fn build_interview_user_prompt(feature_input: &str) -> String {
    log::debug!(
        target: "tddy_workflow_recipes::tdd::interview",
        "build_interview_user_prompt: feature_input_len={}",
        feature_input.len()
    );
    format!(
        "Clarify requirements for the following feature before planning. Include a **demo** yes/no and **demo_options** (how to run the demo) via **tddy-tools ask**, and persist **run_optional_step_x** / **demo_options** into **changeset.yaml** so routing after **green** matches the user's choice:\n\n{}",
        feature_input.trim()
    )
}

/// Build the follow-up prompt after clarification answers (same marker as [`super::planning::build_followup_prompt`] so backends recognize the turn).
pub fn build_followup_prompt(feature_input: &str, answers: &str) -> String {
    format!(
        r#"Here are the user's answers to your questions:

{answers}

Continue the interview for: {feature}"#,
        answers = answers.trim(),
        feature = feature_input.trim(),
    )
}

/// Write relay file so **plan** can recover content after `after_task` clears `answers` / `prompt`.
pub fn persist_interview_handoff_for_plan(
    session_dir: &Path,
    handoff_text: &str,
) -> std::io::Result<()> {
    let path = interview_handoff_path(session_dir);
    log::info!(
        target: "tddy_workflow_recipes::tdd::interview",
        "persist_interview_handoff_for_plan: writing {} bytes to {:?}",
        handoff_text.len(),
        path
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, handoff_text)
}

/// Load relay into context for [`PlanTask`](super::PlanTask) (`answers` / follow-up prompt path).
pub fn apply_staged_interview_handoff_to_plan_context(
    session_dir: &Path,
    context: &Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = interview_handoff_path(session_dir);
    if !path.exists() {
        log::debug!(
            target: "tddy_workflow_recipes::tdd::interview",
            "apply_staged_interview_handoff_to_plan_context: no relay file at {:?}",
            path
        );
        return Ok(());
    }
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("read interview handoff relay {}: {}", path.display(), e))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        log::debug!(
            target: "tddy_workflow_recipes::tdd::interview",
            "apply_staged_interview_handoff_to_plan_context: relay file empty at {:?}",
            path
        );
        return Ok(());
    }
    log::info!(
        target: "tddy_workflow_recipes::tdd::interview",
        "apply_staged_interview_handoff_to_plan_context: staging {} bytes into context answers",
        trimmed.len()
    );
    context.set_sync("answers", trimmed.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_followup_prompt_includes_stub_answer_marker() {
        let p = build_followup_prompt("feat", "a1\na2");
        assert!(
            p.to_uppercase().contains("HERE ARE THE USER'S ANSWERS"),
            "StubBackend and planning follow-ups use this substring after uppercasing; got: {p:?}"
        );
    }
}
