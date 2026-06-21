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

use super::{STACK_STATUS_JSON_BASENAME, STACK_STATUS_MD_BASENAME};

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

fn write_stack_status(
    session_dir: &Path,
    stack: &Stack,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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
        let child_state = node.child_state.as_ref().map(|s| s.as_str()).unwrap_or("-");
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

    fn on_error(&self, task_id: &str, context: &Context, error: &(dyn Error + Send + Sync)) {
        log::warn!("[orchestrate-pr-stack hooks] on_error task={task_id} err={error}");
        let Some(dir) = session_dir_from_context(context) else {
            return;
        };
        set_changeset_state(&dir, WorkflowState::new("Failed"));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tddy_core::changeset::{read_changeset, write_changeset, Changeset, Stack, StackNode};
    use tddy_core::workflow::context::Context;
    use tddy_core::workflow::ids::WorkflowState;
    use tddy_core::workflow::task::{NextAction, TaskResult};

    use super::*;

    fn tmp_session(label: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("orch-hooks-{}-{}", label, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        write_changeset(&d, &Changeset::default()).unwrap();
        d
    }

    fn dummy_result(task_id: &str) -> TaskResult {
        TaskResult {
            response: String::new(),
            next_action: NextAction::Continue,
            task_id: task_id.to_string(),
            status_message: None,
        }
    }

    #[test]
    fn after_task_writes_stack_status_files_when_stack_present() {
        let dir = tmp_session("status-files");
        let mut cs = Changeset::default();
        cs.stack = Some(Stack {
            version: 1,
            nodes: vec![
                StackNode {
                    node_id: "n1".into(),
                    title: "Auth store".into(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: Some("feature/auth-store".into()),
                    session_id: None,
                    parents: vec![],
                    pr_status: None,
                    child_state: None,
                },
                StackNode {
                    node_id: "n2".into(),
                    title: "Auth middleware".into(),
                    description: String::new(),
                    branch_suggestion: None,
                    branch: None,
                    session_id: None,
                    parents: vec!["n1".into()],
                    pr_status: None,
                    child_state: None,
                },
            ],
        });
        write_changeset(&dir, &cs).unwrap();

        let ctx = Context::new();
        ctx.set_sync("session_dir", dir.clone());
        let hooks = OrchestratePrStackHooks::new(None);

        hooks
            .after_task("assess", &ctx, &dummy_result("assess"))
            .unwrap();

        let md_path = dir.join("artifacts").join(STACK_STATUS_MD_BASENAME);
        let json_path = dir.join("artifacts").join(STACK_STATUS_JSON_BASENAME);
        assert!(md_path.exists(), "stack-status.md must be written");
        assert!(json_path.exists(), "stack-status.json must be written");
        let md = fs::read_to_string(&md_path).unwrap();
        assert!(md.contains("n1"), "markdown must include node n1");
        assert!(
            md.contains("Auth store"),
            "markdown must include node title"
        );
        assert!(md.contains("n2"), "markdown must include node n2");
        let json_str = fs::read_to_string(&json_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let nodes = json["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2, "JSON must contain 2 nodes");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn after_task_no_op_when_no_stack_in_changeset() {
        let dir = tmp_session("no-stack");
        let ctx = Context::new();
        ctx.set_sync("session_dir", dir.clone());
        let hooks = OrchestratePrStackHooks::new(None);

        // Should not error and should not create artifacts dir
        hooks
            .after_task("assess", &ctx, &dummy_result("assess"))
            .unwrap();

        assert!(
            !dir.join("artifacts")
                .join(STACK_STATUS_MD_BASENAME)
                .exists(),
            "no status file when stack is absent"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn on_error_writes_failed_state() {
        let dir = tmp_session("on-error");
        let ctx = Context::new();
        ctx.set_sync("session_dir", dir.clone());
        let hooks = OrchestratePrStackHooks::new(None);

        hooks.on_error("assess", &ctx, &std::io::Error::other("boom"));

        let cs = read_changeset(&dir).unwrap();
        assert_eq!(cs.state.current, WorkflowState::new("Failed"));
        let _ = fs::remove_dir_all(&dir);
    }
}
