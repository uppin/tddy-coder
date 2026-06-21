//! RunnerHooks for orchestrate-pr-stack.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use tddy_core::backend::AgentOutputSink;
use tddy_core::changeset::{read_changeset, update_state, write_changeset, Stack};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::task::TaskResult;

use super::{STACK_STATUS_MD_BASENAME, STACK_STATUS_JSON_BASENAME};

pub struct OrchestratePrStackHooks {
    event_tx: Option<mpsc::Sender<WorkflowEvent>>,
}

impl OrchestratePrStackHooks {
    pub fn new(event_tx: Option<mpsc::Sender<WorkflowEvent>>) -> Self {
        Self { event_tx }
    }
}

fn session_dir_from_context(context: &Context) -> Option<PathBuf> {
    context
        .get_sync::<PathBuf>("session_dir")
        .or_else(|| context.get_sync::<PathBuf>("output_dir"))
}

fn set_changeset_state(session_dir: &Path, state: WorkflowState) {
    if let Ok(mut cs) = read_changeset(session_dir) {
        update_state(&mut cs, state);
        if let Err(e) = write_changeset(session_dir, &cs) {
            log::warn!("[orchestrate-pr-stack hooks] could not persist state: {e}");
        }
    }
}

fn write_stack_status(session_dir: &Path, stack: &Stack) -> Result<(), Box<dyn Error + Send + Sync>> {
    let artifacts_dir = session_dir.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir)?;

    // Markdown rollup table.
    let mut md = String::from("# Stack Status\n\n");
    md.push_str("| Node | Title | Branch | Parents | PR Phase | Child State |\n");
    md.push_str("|------|-------|--------|---------|----------|-------------|\n");
    for node in &stack.nodes {
        let branch = node.branch.as_deref().unwrap_or("-");
        let parents = if node.parents.is_empty() {
            "(root)".to_string()
        } else {
            node.parents.join(", ")
        };
        let pr_phase = node
            .pr_status
            .as_ref()
            .map(|p| p.phase.as_str())
            .unwrap_or("-");
        let child_state = node
            .child_state
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("-");
        md.push_str(&format!(
            "| {} | {} | `{}` | {} | {} | {} |\n",
            node.node_id, node.title, branch, parents, pr_phase, child_state
        ));
    }
    std::fs::write(artifacts_dir.join(STACK_STATUS_MD_BASENAME), &md)?;

    // JSON summary.
    let json_nodes: Vec<serde_json::Value> = stack
        .nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "node_id": node.node_id,
                "title": node.title,
                "branch": node.branch,
                "parents": node.parents,
                "pr_phase": node.pr_status.as_ref().map(|p| p.phase.as_str()),
                "pr_url": node.pr_status.as_ref().and_then(|p| p.url.as_deref()),
                "child_state": node.child_state.as_ref().map(|s| s.as_str()),
            })
        })
        .collect();
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "nodes": json_nodes,
        "updated_at": chrono::Utc::now().to_rfc3339(),
    }))?;
    std::fs::write(artifacts_dir.join(STACK_STATUS_JSON_BASENAME), &json)?;

    Ok(())
}

impl RunnerHooks for OrchestratePrStackHooks {
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
        _context: &Context,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::debug!("[orchestrate-pr-stack hooks] before_task: {task_id}");
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(WorkflowEvent::GoalStarted(task_id.to_string()));
        }
        Ok(())
    }

    fn after_task(
        &self,
        task_id: &str,
        context: &Context,
        _result: &TaskResult,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        log::debug!("[orchestrate-pr-stack hooks] after_task: {task_id}");
        let Some(dir) = session_dir_from_context(context) else {
            return Ok(());
        };
        if let Ok(cs) = read_changeset(&dir) {
            if let Some(ref stack) = cs.stack {
                if let Err(e) = write_stack_status(&dir, stack) {
                    log::warn!("[orchestrate-pr-stack hooks] write_stack_status failed: {e}");
                }
            }
        }
        Ok(())
    }

    fn on_error(
        &self,
        task_id: &str,
        context: &Context,
        error: &(dyn Error + Send + Sync),
    ) {
        log::warn!("[orchestrate-pr-stack hooks] on_error task={task_id} err={error}");
        let Some(dir) = session_dir_from_context(context) else { return };
        set_changeset_state(&dir, WorkflowState::new("Failed"));
    }
}
