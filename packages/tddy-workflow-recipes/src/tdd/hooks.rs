//! TddWorkflowHooks — file I/O and event emission for the TDD workflow.
//!
//! Implements RunnerHooks for the graph-flow path. Writes artifacts from context
//! in after_task, reads artifacts into context in before_task.

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
use tddy_core::workflow::{clear_sinks, set_sinks};

use crate::SessionArtifactManifest;
use tddy_core::workflow::task::TaskResult;

use super::hooks_common;

mod before;

mod after;

/// Hooks for the TDD workflow. Handles file I/O. Event emission for TUI when event_tx is set.
pub struct TddWorkflowHooks {
    recipe: Arc<dyn WorkflowRecipe>,
    manifest: Arc<dyn SessionArtifactManifest>,
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl TddWorkflowHooks {
    /// Create hooks for CLI path (file I/O only, no events).
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

    /// Create hooks with event emission for TUI (GoalStarted, StateChange).
    pub fn with_event_tx(
        recipe: Arc<dyn WorkflowRecipe>,
        manifest: Arc<dyn SessionArtifactManifest>,
        event_tx: mpsc::Sender<WorkflowEvent>,
    ) -> Self {
        Self {
            recipe,
            manifest,
            event_tx: Some(event_tx),
        }
    }

    pub fn with_event_tx_optional(
        recipe: Arc<dyn WorkflowRecipe>,
        manifest: Arc<dyn SessionArtifactManifest>,
        event_tx: Option<mpsc::Sender<WorkflowEvent>>,
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
                            "progress_sink SessionStarted",
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

impl RunnerHooks for TddWorkflowHooks {
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
        log::debug!("[tddy-core] state: → {}", task_id);
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
        }
        if task_id == "interview" {
            before::before_interview(context)?;
        } else if task_id == "plan" {
            before::before_plan_with_interview(context)?;
        } else {
            let session_dir: Option<PathBuf> = context
                .get_sync("session_dir")
                .or_else(|| context.get_sync("output_dir"));
            let session_dir = match session_dir {
                Some(p) => p,
                None => return Ok(()),
            };

            match task_id {
                "acceptance-tests" => {
                    hooks_common::ensure_worktree_for_session(
                        &session_dir,
                        context,
                        self.event_tx.as_ref(),
                        "[tddy-core] acceptance-tests",
                    )?;
                    before::before_acceptance_tests(
                        &session_dir,
                        context,
                        self.recipe.as_ref(),
                        self.manifest.as_ref(),
                    )?;
                }
                "red" => before::before_red(
                    &session_dir,
                    context,
                    self.recipe.as_ref(),
                    self.manifest.as_ref(),
                )?,
                "green" => hooks_common::before_green(
                    &session_dir,
                    context,
                    self.recipe.as_ref(),
                    self.manifest.as_ref(),
                    "tddy_workflow_recipes::tdd::hooks",
                )?,
                "demo" => before::before_demo(&session_dir, context, self.manifest.as_ref())?,
                "evaluate" => before::before_evaluate(
                    &session_dir,
                    context,
                    self.recipe.as_ref(),
                    self.manifest.as_ref(),
                )?,
                "validate" => {
                    before::before_validate(&session_dir, context, self.manifest.as_ref())?
                }
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
                    "interview" => Some("Interviewing"),
                    "plan" => Some("Planning"),
                    "acceptance-tests" => Some("AcceptanceTesting"),
                    "red" => Some("RedTesting"),
                    "green" => Some("GreenImplementing"),
                    "demo" => Some("DemoRunning"),
                    "evaluate" => Some("Evaluating"),
                    "validate" => Some("Validating"),
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
        result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let interview_handoff_snapshot: Option<String> = if task_id == "interview" {
            context
                .get_sync::<String>("prompt")
                .or_else(|| context.get_sync::<String>("answers"))
        } else {
            None
        };
        let session_dir: Option<PathBuf> = context
            .get_sync("session_dir")
            .or_else(|| context.get_sync("output_dir"));
        if let (Some(ref tx), Some(ref dir)) = (&self.event_tx, session_dir) {
            let current = read_changeset(dir)
                .ok()
                .map(|c| c.state.current)
                .unwrap_or_else(|| WorkflowState::new("Init"));
            let (from, to) = match task_id {
                "interview" => ("Interviewing", "Interviewed"),
                "plan" => ("Planning", "Planned"),
                "acceptance-tests" => ("AcceptanceTesting", "AcceptanceTestsReady"),
                "red" => ("RedTesting", "RedTestsReady"),
                "green" => ("GreenImplementing", "GreenComplete"),
                "demo" => ("DemoRunning", "DemoComplete"),
                "evaluate" => ("Evaluating", "Evaluated"),
                "validate" => ("Validating", "ValidateComplete"),
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
            "interview" => {
                let session_dir: PathBuf = context
                    .get_sync("session_dir")
                    .or_else(|| context.get_sync("output_dir"))
                    .ok_or("interview after_task requires session_dir or output_dir in context")?;
                after::after_interview(&session_dir, result, interview_handoff_snapshot)?;
            }
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
            "acceptance-tests" | "red" | "green" | "evaluate" => {
                let session_dir: PathBuf = context
                    .get_sync("session_dir")
                    .or_else(|| context.get_sync("output_dir"))
                    .ok_or("after_task requires session_dir or output_dir in context")?;
                let output: String = context
                    .get_sync("output")
                    .ok_or("after_task requires output in context")?;
                match task_id {
                    "acceptance-tests" => {
                        after::after_acceptance_tests(&session_dir, &output, context)?
                    }
                    "red" => after::after_red(&session_dir, &output, context)?,
                    "green" => after::after_green(&session_dir, &output)?,
                    "evaluate" => after::after_evaluate(&session_dir, &output)?,
                    _ => {}
                }
            }
            "validate" | "refactor" | "update-docs" | "demo" => {
                let output: Option<String> = context.get_sync("output");
                let session_dir: Option<PathBuf> = context
                    .get_sync("session_dir")
                    .or_else(|| context.get_sync("output_dir"));
                if let (Some(ref output), Some(ref session_dir)) = (output, session_dir) {
                    match task_id {
                        "validate" => after::after_validate(session_dir, output)?,
                        "refactor" => after::after_refactor(session_dir, output)?,
                        "update-docs" => after::after_update_docs(session_dir, output)?,
                        "demo" => after::after_demo(session_dir)?,
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
        let dominated_by_plan = task_id == self.recipe.plan_refinement_goal().as_str();
        if !dominated_by_plan {
            return None;
        }
        let session_dir: PathBuf = context
            .get_sync("session_dir")
            .or_else(|| context.get_sync("output_dir"))?;
        let basename = self.manifest.primary_document_basename()?;
        let prd_path = tddy_workflow::resolve_existing_session_artifact(&session_dir, &basename)?;
        log::debug!(
            "[tdd hooks] elicitation DocumentApproval reading {:?}",
            prd_path
        );
        let prd_content = std::fs::read_to_string(&prd_path).ok()?;
        Some(ElicitationEvent::DocumentApproval {
            content: prd_content,
        })
    }

    fn on_error(&self, _task_id: &str, context: &Context, error: &(dyn Error + Send + Sync)) {
        log::error!("[tddy-core] workflow task failed: {}", error);
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
                "[tdd hooks] on_error: could not persist Failed state: {} (session_dir={})",
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
