//! Hooks for the bug-fix workflow (interview → analyze → reproduce with agent output + questions).

use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc;

use super::analyze::{
    apply_analyze_submit_to_changeset, reproduce_system_prompt, seed_bugfix_changeset_if_missing,
    system_prompt,
};
use super::interview;
use crate::tdd::hooks_common;
use tddy_core::backend::AgentOutputSink;
use tddy_core::changeset::{read_changeset, update_state, write_changeset};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::task::TaskResult;

/// Hooks for [`super::BugfixRecipe`]. Emits goal/state events and provides an agent output sink.
#[derive(Debug)]
pub struct BugfixWorkflowHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl BugfixWorkflowHooks {
    pub fn new(event_tx: Option<mpsc::Sender<WorkflowEvent>>) -> Self {
        Self { event_tx }
    }
}

fn set_analyzing_state(session_dir: &std::path::Path) {
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("Analyzing"));
        if let Err(e) = write_changeset(session_dir, &cs) {
            log::debug!("[bugfix hooks] could not write Analyzing state: {}", e);
        } else {
            log::debug!("[bugfix hooks] changeset state -> Analyzing");
        }
    }
}

fn set_interviewing_state(session_dir: &std::path::Path) {
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, WorkflowState::new("Interviewing"));
        if let Err(e) = write_changeset(session_dir, &cs) {
            log::debug!("[bugfix hooks] could not write Interviewing state: {}", e);
        } else {
            log::debug!("[bugfix hooks] changeset state -> Interviewing");
        }
    }
}

/// Prepend optional analyze summary (from `changeset.artifacts["analyze_summary"]`) to the
/// reproduce prompt so the backend sees triage context.
fn merge_analyze_summary_into_prompt(context: &Context, session_dir: &std::path::Path) {
    let Ok(cs) = read_changeset(session_dir) else {
        return;
    };
    let Some(summary) = cs.artifacts.get("analyze_summary") else {
        return;
    };
    if summary.trim().is_empty() {
        return;
    }
    let base = context
        .get_sync::<String>("prompt")
        .or_else(|| context.get_sync("feature_input"))
        .unwrap_or_default();
    let merged = if base.trim().is_empty() {
        format!("**Analysis summary:**\n{}", summary.trim())
    } else {
        format!(
            "**Analysis summary:**\n{}\n\n---\n\n{}",
            summary.trim(),
            base.trim()
        )
    };
    context.set_sync("prompt", merged);
    log::debug!("[bugfix hooks] merged analyze summary into reproduce prompt");
}

/// `before_task` body for **`interview`** (TDD-shaped: prompts + optional session/changeset seed).
fn before_bugfix_interview(context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    log::info!("[bugfix hooks] before_bugfix_interview: set prompts (bugfix interview module)");
    let feature_input: String = context.get_sync("feature_input").unwrap_or_default();
    if let Some(answers) = context.get_sync::<String>("answers") {
        if !answers.trim().is_empty() {
            log::debug!(
                "[bugfix hooks] before_bugfix_interview: follow-up prompt from answers (len={})",
                answers.len()
            );
            context.set_sync(
                "prompt",
                interview::build_followup_prompt(&feature_input, &answers),
            );
            context.remove_sync("answers");
        } else {
            context.set_sync(
                "prompt",
                interview::build_interview_user_prompt(&feature_input),
            );
        }
    } else {
        context.set_sync(
            "prompt",
            interview::build_interview_user_prompt(&feature_input),
        );
    }
    context.set_sync("system_prompt", interview::system_prompt());
    if let Some(ref session_dir) = context.get_sync::<PathBuf>("session_dir") {
        let repo_path = context
            .get_sync::<PathBuf>("output_dir")
            .map(|p| p.display().to_string());
        seed_bugfix_changeset_if_missing(session_dir.as_path(), feature_input, repo_path);
        set_interviewing_state(session_dir.as_path());
    }
    Ok(())
}

/// `before_task` body for **`analyze`**: merge interview relay, triage system prompt, changeset seed/state.
fn before_bugfix_analyze(context: &Context) -> Result<(), Box<dyn Error + Send + Sync>> {
    log::info!(
        "[bugfix hooks] before_bugfix_analyze: merge interview relay, set system prompt and state"
    );
    if let Some(ref session_dir) = context.get_sync::<PathBuf>("session_dir") {
        interview::apply_bugfix_interview_handoff_to_analyze_context(
            session_dir.as_path(),
            context,
        )?;
    }
    context.set_sync("system_prompt", system_prompt());
    if let Some(ref session_dir) = context.get_sync::<PathBuf>("session_dir") {
        let feature_input: String = context.get_sync("feature_input").unwrap_or_default();
        let repo_path = context
            .get_sync::<PathBuf>("output_dir")
            .map(|p| p.display().to_string());
        seed_bugfix_changeset_if_missing(session_dir.as_path(), feature_input, repo_path);
        set_analyzing_state(session_dir.as_path());
    }
    Ok(())
}

impl RunnerHooks for BugfixWorkflowHooks {
    fn agent_output_sink(&self) -> Option<AgentOutputSink> {
        self.event_tx.as_ref().map(|tx| {
            let tx = tx.clone();
            AgentOutputSink::new(move |s: &str| {
                let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
            })
        })
    }

    fn before_task(
        &self,
        task_id: &str,
        context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::debug!("[bugfix hooks] before_task: {}", task_id);
        match task_id {
            "interview" => before_bugfix_interview(context)?,
            "analyze" => before_bugfix_analyze(context)?,
            "reproduce" => {
                log::info!("[bugfix hooks] before_task reproduce: set system prompt");
                context.set_sync("system_prompt", reproduce_system_prompt());
                if let Some(ref session_dir) = context.get_sync::<PathBuf>("session_dir") {
                    merge_analyze_summary_into_prompt(context, session_dir.as_path());
                    // Align with TDD `acceptance-tests`: create session worktree after triage names exist
                    // in `changeset.yaml` (from `analyze` submit), so reproduce runs against isolated tree.
                    hooks_common::ensure_worktree_for_session(
                        session_dir.as_path(),
                        context,
                        self.event_tx.as_ref(),
                        "[bugfix hooks] reproduce",
                    )?;
                }
            }
            _ => {}
        }
        if let Some(answers) = context.get_sync::<String>("answers") {
            if !answers.trim().is_empty() {
                log::debug!(
                    "[bugfix hooks] transferring answers to prompt (len={})",
                    answers.len()
                );
                let prompt_with_answers = format!(
                    "Here are the user's answers to clarification questions:\n{}",
                    answers
                );
                context.set_sync("prompt", &prompt_with_answers);
                context.remove_sync("answers");
            }
        }
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
            let _ = tx.send(WorkflowEvent::StateChange {
                from: String::new(),
                to: task_id.to_string(),
            });
        }
        Ok(())
    }

    fn after_task(
        &self,
        task_id: &str,
        context: &Context,
        result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if task_id == "interview" {
            log::info!("[bugfix hooks] after_task interview: persist handoff + Interviewed state");
            if let Some(session_dir) = context.get_sync::<PathBuf>("session_dir") {
                interview::persist_bugfix_interview_handoff_for_analyze(
                    session_dir.as_path(),
                    &result.response,
                )?;
                if let Ok(mut cs) = read_changeset(session_dir.as_path()) {
                    update_state(&mut cs, WorkflowState::new("Interviewed"));
                    let _ = write_changeset(session_dir.as_path(), &cs);
                }
            } else {
                log::debug!("[bugfix hooks] after_task interview: no session_dir in context");
            }
        }
        if task_id == "analyze" {
            log::info!("[bugfix hooks] after_task analyze: persisting submit to changeset");
            if let Some(session_dir) = context.get_sync::<PathBuf>("session_dir") {
                apply_analyze_submit_to_changeset(session_dir.as_path(), result)?;
            } else {
                log::debug!("[bugfix hooks] after_task analyze: no session_dir in context");
            }
        }
        Ok(())
    }

    fn on_error(&self, _task_id: &str, _context: &Context, _error: &(dyn Error + Send + Sync)) {}
}
