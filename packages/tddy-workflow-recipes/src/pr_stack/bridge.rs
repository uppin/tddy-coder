//! `begin-orchestrate` bridge task: connects the plan phase to the orchestrate loop in one
//! session. Not a backend/agent goal — a host-only [`Task`] that seeds `Changeset.stack` from
//! the just-written `stack-plan.yaml` (idempotent, same guard as
//! [`crate::orchestrate_pr_stack::seed_orchestrator_stack_from_plan`]) and replays any in-flight
//! crash-recovery journal before handing off to `assess`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::task::{NextAction, Task, TaskResult};

use crate::plan_pr_stack::STACK_PLAN_BASENAME;

/// Host-only bridge task: `write-stack-plan` -> `begin-orchestrate` -> `assess`.
pub struct BeginOrchestrateTask {}

impl BeginOrchestrateTask {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for BeginOrchestrateTask {
    fn default() -> Self {
        Self::new()
    }
}

/// Seed `Changeset.stack` from `stack-plan.yaml` if the stack is not already populated, then
/// replay any in-flight crash-recovery journal. Idempotent — safe to call on every entry.
fn seed_and_recover(session_dir: &Path) -> Result<(), String> {
    let changeset = tddy_core::changeset::read_changeset(session_dir)
        .map_err(|e| format!("begin-orchestrate: failed to read changeset: {e}"))?;
    let needs_seed = changeset
        .stack
        .as_ref()
        .map(|s| s.nodes.is_empty())
        .unwrap_or(true);

    if needs_seed {
        let plan_path = session_dir.join(STACK_PLAN_BASENAME);
        if plan_path.exists() {
            let yaml = std::fs::read_to_string(&plan_path).map_err(|e| {
                format!("begin-orchestrate: failed to read {STACK_PLAN_BASENAME}: {e}")
            })?;
            let plan: crate::plan_pr_stack::StackPlanOutput =
                serde_yaml::from_str(&yaml).map_err(|e| {
                    format!("begin-orchestrate: failed to parse {STACK_PLAN_BASENAME}: {e}")
                })?;
            crate::orchestrate_pr_stack::seed_orchestrator_stack_from_plan(session_dir, &plan)
                .map_err(|e| {
                    format!("begin-orchestrate: seed_orchestrator_stack_from_plan failed: {e}")
                })?;
        }
    }

    crate::orchestrate_pr_stack::recover_in_flight_stack_op(session_dir)
        .map_err(|e| format!("begin-orchestrate: recover_in_flight_stack_op failed: {e}"))?;

    Ok(())
}

#[async_trait]
impl Task for BeginOrchestrateTask {
    fn id(&self) -> &str {
        "begin-orchestrate"
    }

    async fn run(
        &self,
        context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        let session_dir = context
            .get_sync::<PathBuf>("session_dir")
            .ok_or("begin-orchestrate: session_dir not in context")?;

        seed_and_recover(&session_dir)?;

        Ok(TaskResult {
            response: "begin-orchestrate: stack seeded and in-flight journal recovered".to_string(),
            next_action: NextAction::GoTo("assess".to_string()),
            task_id: self.id().to_string(),
            status_message: None,
        })
    }
}
