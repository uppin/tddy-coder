//! Hooks for the `tdd-small` workflow: merged red, single post-green submit, shared green/refactor/docs.

use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use tddy_core::backend::{AgentOutputSink, ProgressSink};
use tddy_core::changeset::{read_changeset, update_state, write_changeset, SessionEntry};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::stream::ProgressEvent as StreamProgressEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::graph::ElicitationEvent;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::recipe::WorkflowRecipe;
use tddy_core::workflow::task::TaskResult;
use tddy_core::workflow::{clear_sinks, set_sinks};

use crate::tdd::hooks_common;
use crate::SessionArtifactManifest;

mod before;

mod after;

/// Hooks for the `tdd-small` workflow.
pub struct TddSmallWorkflowHooks {
    recipe: Arc<dyn WorkflowRecipe>,
    manifest: Arc<dyn SessionArtifactManifest>,
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl TddSmallWorkflowHooks {
    /// CLI path: file I/O only (no events).
    pub fn new(
        recipe: Arc<dyn WorkflowRecipe>,
        manifest: Arc<dyn SessionArtifactManifest>,
    ) -> Self {
        Self {
            recipe,
            manifest,
            event_tx: None,
        }
    }

    /// Hooks with optional TUI event channel.
    pub fn with_event_tx_optional(
        recipe: Arc<dyn WorkflowRecipe>,
        manifest: Arc<dyn SessionArtifactManifest>,
        event_tx: Option<tddy_core::workflow::recipe::WorkflowEventSender>,
    ) -> Self {
        Self {
            recipe,
            manifest,
            event_tx,
        }
    }

    fn agent_output_sink_impl(&self) -> Option<AgentOutputSink> {
        self.event_tx.as_ref().map(|tx| {
            let tx = tx.clone();
            AgentOutputSink::new(move |s: &str| {
                let _ = tx.send(WorkflowEvent::AgentOutput(s.to_string()));
            })
        })
    }

    fn progress_sink_impl(&self, context: &Context) -> Option<ProgressSink> {
        let session_dir: Option<PathBuf> = context
            .get_sync("session_dir")
            .or_else(|| context.get_sync("output_dir"));
        let task_id: Option<String> = context.get_sync("current_task_id");
        let backend_name: String = context
            .get_sync("backend_name")
            .unwrap_or_else(|| "claude".to_string());
        let event_tx = self.event_tx.clone();

        let recipe_for_progress = self.recipe.clone();
        Some(ProgressSink::new(move |ev: &StreamProgressEvent| {
            if let StreamProgressEvent::SessionStarted { session_id } = ev {
                if let Some(ref dir) = session_dir {
                    if let Ok(mut cs) = read_changeset(dir) {
                        let already_exists = cs.sessions.iter().any(|s| s.id == *session_id);
                        if !already_exists {
                            let tag = match task_id.as_deref() {
                                Some("red") => "impl".to_string(),
                                Some(t) => t.to_string(),
                                None => recipe_for_progress.start_goal().to_string(),
                            };
                            let now = chrono::Utc::now().to_rfc3339();
                            cs.sessions.push(SessionEntry {
                                id: session_id.clone(),
                                agent: backend_name.clone(),
                                tag,
                                created_at: now,
                                system_prompt_file: None,
                            });
                        }
                        cs.state.session_id = Some(session_id.clone());
                        hooks_common::write_changeset_logged(
                            dir,
                            &cs,
                            "tdd_small progress_sink SessionStarted",
                        );
                    }
                }
            }
            if let Some(ref tx) = event_tx {
                let _ = tx.send(WorkflowEvent::Progress(ev.clone()));
            }
        }))
    }
}

impl RunnerHooks for TddSmallWorkflowHooks {
    fn on_enter_task(&self, _task_id: &str, context: &Context) {
        set_sinks(
            self.agent_output_sink_impl(),
            self.progress_sink_impl(context),
        );
    }

    fn on_exit_task(&self, _task_id: &str, _context: &Context) {
        clear_sinks();
    }

    fn before_task(
        &self,
        task_id: &str,
        context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        context.set_sync("current_task_id", task_id.to_string());
        log::debug!("[tdd-small hooks] before_task task_id={}", task_id);
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
        }
        if task_id == "plan" {
            hooks_common::before_plan(context)?;
        } else {
            let session_dir: Option<PathBuf> = context
                .get_sync("session_dir")
                .or_else(|| context.get_sync("output_dir"));
            let session_dir = match session_dir {
                Some(p) => p,
                None => return Ok(()),
            };

            match task_id {
                "red" => {
                    hooks_common::ensure_worktree_for_session(
                        &session_dir,
                        context,
                        self.event_tx.as_ref(),
                        "[tdd-small hooks] merged red",
                    )?;
                    before::before_merged_red(
                        &session_dir,
                        context,
                        self.recipe.as_ref(),
                        self.manifest.as_ref(),
                    )?;
                }
                "green" => hooks_common::before_green(
                    &session_dir,
                    context,
                    self.recipe.as_ref(),
                    self.manifest.as_ref(),
                    "tddy_workflow_recipes::tdd_small::hooks",
                )?,
                "post-green-review" => before::before_post_green_review(
                    &session_dir,
                    context,
                    self.recipe.as_ref(),
                    self.manifest.as_ref(),
                )?,
                "refactor" => {
                    before::before_refactor(&session_dir, context, self.manifest.as_ref())?
                }
                "update-docs" => {
                    before::before_update_docs(self.manifest.as_ref(), &session_dir, context)?
                }
                _ => {}
            }
        }
        let is_resuming = context.get_sync::<String>("answers").is_some();
        if !is_resuming {
            if let Some(ref tx) = self.event_tx {
                let session_dir_for_state: Option<PathBuf> = context
                    .get_sync("session_dir")
                    .or_else(|| context.get_sync("output_dir"));
                let from = session_dir_for_state
                    .as_ref()
                    .and_then(|sd| read_changeset(sd).ok())
                    .map(|c| c.state.current)
                    .unwrap_or_else(|| WorkflowState::new("Init"));
                let to_transitional = match task_id {
                    "plan" => Some("Planning"),
                    "red" => Some("RedTesting"),
                    "green" => Some("GreenImplementing"),
                    "post-green-review" => Some("Evaluating"),
                    "refactor" => Some("Refactoring"),
                    "update-docs" => Some("UpdatingDocs"),
                    _ => None,
                };
                if let Some(to) = to_transitional {
                    let _ = tx.send(WorkflowEvent::StateChange {
                        from: from.to_string(),
                        to: to.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    fn after_task(
        &self,
        task_id: &str,
        context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let session_dir: Option<PathBuf> = context
            .get_sync("session_dir")
            .or_else(|| context.get_sync("output_dir"));
        if let (Some(ref tx), Some(ref dir)) = (&self.event_tx, session_dir) {
            let current = read_changeset(dir)
                .ok()
                .map(|c| c.state.current)
                .unwrap_or_else(|| WorkflowState::new("Init"));
            let (from, to) = match task_id {
                "plan" => ("Planning", "Planned"),
                "red" => ("RedTesting", "RedTestsReady"),
                "green" => ("GreenImplementing", "GreenComplete"),
                "post-green-review" => ("Evaluating", "ValidateComplete"),
                "refactor" => ("Refactoring", "RefactorComplete"),
                "update-docs" => ("UpdatingDocs", "DocsUpdated"),
                _ => (current.as_str(), current.as_str()),
            };
            if to != from {
                let _ = tx.send(WorkflowEvent::StateChange {
                    from: from.to_string(),
                    to: to.to_string(),
                });
                if let Some(next_goal) = self.recipe.next_goal_for_state(&WorkflowState::new(to)) {
                    let _ = tx.send(WorkflowEvent::GoalStarted(next_goal.to_string()));
                }
            }
        }
        context.remove_sync("answers");
        context.remove_sync("is_resume");
        match task_id {
            "plan" => {
                let session_dir: PathBuf = context
                    .get_sync("session_dir")
                    .ok_or("plan after_task requires session_dir in context (set by PlanTask)")?;
                after::after_plan(
                    self.recipe.as_ref(),
                    self.manifest.as_ref(),
                    &session_dir,
                    context,
                )?;
            }
            "red" | "green" | "post-green-review" => {
                let session_dir: PathBuf = context
                    .get_sync("session_dir")
                    .or_else(|| context.get_sync("output_dir"))
                    .ok_or("after_task requires session_dir or output_dir in context")?;
                let output: String = context
                    .get_sync("output")
                    .ok_or("after_task requires output in context")?;
                match task_id {
                    "red" => after::after_red(&session_dir, &output, context)?,
                    "green" => after::after_green(&session_dir, &output)?,
                    "post-green-review" => after::after_post_green_review(&session_dir, &output)?,
                    _ => {}
                }
            }
            "refactor" | "update-docs" => {
                let output: Option<String> = context.get_sync("output");
                let session_dir: Option<PathBuf> = context
                    .get_sync("session_dir")
                    .or_else(|| context.get_sync("output_dir"));
                if let (Some(ref output), Some(ref session_dir)) = (output, session_dir) {
                    match task_id {
                        "refactor" => after::after_refactor(session_dir, output)?,
                        "update-docs" => after::after_update_docs(session_dir, output)?,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn elicitation_after_task(
        &self,
        task_id: &str,
        context: &Context,
        _result: &TaskResult,
    ) -> Option<ElicitationEvent> {
        if task_id != self.recipe.start_goal().as_str() {
            return None;
        }
        let session_dir: PathBuf = context
            .get_sync("session_dir")
            .or_else(|| context.get_sync("output_dir"))?;
        let basename = self.manifest.primary_document_basename()?;
        let prd_path = tddy_workflow::resolve_existing_session_artifact(&session_dir, &basename)?;
        log::debug!(
            "[tdd-small hooks] elicitation DocumentApproval reading {:?}",
            prd_path
        );
        let prd_content = std::fs::read_to_string(&prd_path).ok()?;
        Some(ElicitationEvent::DocumentApproval {
            content: prd_content,
        })
    }

    fn on_error(&self, _task_id: &str, context: &Context, error: &(dyn Error + Send + Sync)) {
        log::error!("[tdd-small hooks] workflow task failed: {}", error);
        let session_dir: Option<PathBuf> = context
            .get_sync("session_dir")
            .or_else(|| context.get_sync("output_dir"));
        let Some(ref dir) = session_dir else {
            return;
        };
        let Ok(mut cs) = read_changeset(dir) else {
            return;
        };
        let from = cs.state.current.to_string();
        update_state(&mut cs, WorkflowState::new("Failed"));
        if let Err(e) = write_changeset(dir, &cs) {
            log::warn!(
                "[tdd-small hooks] on_error: could not persist Failed state: {} (session_dir={})",
                e,
                dir.display()
            );
            return;
        }
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::StateChange {
                from,
                to: "Failed".to_string(),
            });
        }
    }
}
