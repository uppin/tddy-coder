use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc;

use tddy_core::backend::AgentOutputSink;
use tddy_core::changeset::{read_changeset, update_state, write_changeset};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::task::TaskResult;

use super::prompt;
use super::{validate_stack_plan, StackPlanOutput, STACK_PLAN_BASENAME, PR_STACK_PLAN_MD_BASENAME};

pub struct PlanPrStackHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl PlanPrStackHooks {
    pub fn new(event_tx: Option<mpsc::Sender<WorkflowEvent>>) -> Self {
        Self { event_tx }
    }
}

fn session_dir_from_context(context: &Context) -> Option<PathBuf> {
    context
        .get_sync::<PathBuf>("session_dir")
        .or_else(|| context.get_sync::<PathBuf>("output_dir"))
}

fn set_changeset_state(session_dir: &std::path::Path, state: WorkflowState) {
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, state);
        if let Err(e) = write_changeset(session_dir, &cs) {
            log::warn!("[plan-pr-stack hooks] could not persist state: {e}");
        }
    }
}

fn generate_pr_stack_plan_md(plan: &StackPlanOutput) -> String {
    let mut md = String::from("# PR Stack Plan\n\n");
    for pr in &plan.prs {
        md.push_str(&format!("## {} — {}\n\n", pr.node_id, pr.title));
        if !pr.description.trim().is_empty() {
            md.push_str(&pr.description);
            md.push_str("\n\n");
        }
        if let Some(ref branch) = pr.branch_suggestion {
            md.push_str(&format!("**Branch:** `{branch}`\n\n"));
        }
        if pr.parents.is_empty() {
            md.push_str("**Dependencies:** (root — off stack base)\n\n");
        } else {
            md.push_str(&format!("**Dependencies:** {}\n\n", pr.parents.join(", ")));
        }
        if let Some(ref recipe) = pr.child_recipe {
            md.push_str(&format!("**Recipe:** {recipe}\n\n"));
        }
    }
    md
}

impl RunnerHooks for PlanPrStackHooks {
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
        log::debug!("[plan-pr-stack hooks] before_task: {task_id}");
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
        }
        let session_dir = session_dir_from_context(context);

        match task_id {
            "analyze-stack" => {
                context.set_sync("system_prompt", prompt::analyze_stack_system_prompt());
                let feature_input: String =
                    context.get_sync("feature_input").unwrap_or_default();
                let answers: Option<String> = context.get_sync("answers");
                let user_prompt = if let Some(a) = answers.filter(|s| !s.trim().is_empty()) {
                    format!(
                        "{}\n\n## Clarification\n\n{a}",
                        prompt::analyze_stack_user_prompt(&feature_input)
                    )
                } else {
                    prompt::analyze_stack_user_prompt(&feature_input)
                };
                context.set_sync("prompt", user_prompt);
                if let Some(ref dir) = session_dir {
                    set_changeset_state(dir, WorkflowState::new("AnalyzeStack"));
                }
            }
            "write-stack-plan" => {
                context.set_sync("system_prompt", prompt::write_stack_plan_system_prompt());
                let feature_input: String =
                    context.get_sync("feature_input").unwrap_or_default();
                let analysis_output: String = context.get_sync("output").unwrap_or_default();
                let answers: Option<String> = context.get_sync("answers");
                let user_prompt = prompt::write_stack_plan_user_prompt(
                    &feature_input,
                    &analysis_output,
                    answers.as_deref(),
                );
                context.set_sync("prompt", user_prompt);
                if let Some(ref dir) = session_dir {
                    set_changeset_state(dir, WorkflowState::new("WriteStackPlan"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn after_task(
        &self,
        task_id: &str,
        context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let session_dir = session_dir_from_context(context);

        match task_id {
            "analyze-stack" => {
                if let Some(ref dir) = session_dir {
                    set_changeset_state(dir, WorkflowState::new("WriteStackPlan"));
                }
            }
            "write-stack-plan" => {
                let dir = session_dir
                    .ok_or("write-stack-plan after_task requires session_dir in context")?;

                let output: String = context
                    .get_sync("output")
                    .ok_or("write-stack-plan after_task requires output in context")?;

                let plan: StackPlanOutput = serde_yaml::from_str(&output)
                    .map_err(|e| format!("failed to parse stack-plan YAML: {e}"))?;

                validate_stack_plan(&plan)
                    .map_err(|e| format!("stack plan validation failed: {e}"))?;

                // Write stack-plan.yaml (re-serialized for canonical form).
                let yaml = serde_yaml::to_string(&plan)
                    .map_err(|e| format!("failed to serialize stack-plan: {e}"))?;
                std::fs::write(dir.join(STACK_PLAN_BASENAME), &yaml)
                    .map_err(|e| format!("write {STACK_PLAN_BASENAME}: {e}"))?;

                // Write human-readable pr-stack-plan.md.
                let md = generate_pr_stack_plan_md(&plan);
                std::fs::write(dir.join(PR_STACK_PLAN_MD_BASENAME), &md)
                    .map_err(|e| format!("write {PR_STACK_PLAN_MD_BASENAME}: {e}"))?;

                set_changeset_state(&dir, WorkflowState::new("StackPlanned"));
            }
            _ => {}
        }
        Ok(())
    }

    fn on_error(
        &self,
        task_id: &str,
        context: &Context,
        error: &(dyn Error + Send + Sync),
    ) {
        log::warn!("[plan-pr-stack hooks] on_error task={task_id} err={error}");
        let Some(dir) = session_dir_from_context(context) else { return };
        set_changeset_state(&dir, WorkflowState::new("Failed"));
    }
}
