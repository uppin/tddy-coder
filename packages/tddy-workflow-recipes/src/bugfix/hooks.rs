//! Hooks for the bug-fix workflow (analyze → reproduce with agent output + questions).

use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc;

use super::analyze::{
    apply_analyze_submit_to_changeset, reproduce_system_prompt, seed_bugfix_changeset_if_missing,
    system_prompt,
};
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
            "analyze" => {
                log::info!("[bugfix hooks] before_task analyze: set system prompt and state");
                context.set_sync("system_prompt", system_prompt());
                if let Some(ref session_dir) = context.get_sync::<PathBuf>("session_dir") {
                    let feature_input: String =
                        context.get_sync("feature_input").unwrap_or_default();
                    let repo_path = context
                        .get_sync::<PathBuf>("output_dir")
                        .map(|p| p.display().to_string());
                    seed_bugfix_changeset_if_missing(
                        session_dir.as_path(),
                        feature_input,
                        repo_path,
                    );
                    set_analyzing_state(session_dir.as_path());
                }
            }
            "reproduce" => {
                log::info!("[bugfix hooks] before_task reproduce: set system prompt");
                context.set_sync("system_prompt", reproduce_system_prompt());
                if let Some(ref session_dir) = context.get_sync::<PathBuf>("session_dir") {
                    merge_analyze_summary_into_prompt(context, session_dir.as_path());
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
