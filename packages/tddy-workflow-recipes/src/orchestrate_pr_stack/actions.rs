//! Custom orchestrator tasks: SpawnTask, MergeTask, RepointTask.

use async_trait::async_trait;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::task::{NextAction, Task, TaskResult};

/// Task: spawn child sessions for nodes that are NotSpawned.
///
/// Reads `spawn_node_ids` from context, writes a `.spawn-request.json` in the session dir
/// for each node so the daemon can pick up and start them, then returns to assess.
pub struct SpawnTask {}

impl SpawnTask {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for SpawnTask {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Task for SpawnTask {
    fn id(&self) -> &str {
        "spawn"
    }

    async fn run(
        &self,
        context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        use std::path::PathBuf;

        let session_dir = context
            .get_sync::<PathBuf>("session_dir")
            .ok_or("SpawnTask: session_dir not in context")?;
        let node_ids = context
            .get_sync::<Vec<String>>("spawn_node_ids")
            .unwrap_or_default();

        // Write a spawn-request marker for each node so the daemon can pick it up.
        // The daemon polls for `.spawn-request-<node_id>.json` and starts the child session.
        for node_id in &node_ids {
            let marker = session_dir
                .join(".workflow")
                .join(format!("spawn-request-{node_id}.json"));
            if let Some(parent) = marker.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let payload = serde_json::json!({ "node_id": node_id }).to_string();
            std::fs::write(&marker, payload.as_bytes()).ok();
            log::info!("SpawnTask: wrote spawn request for node {node_id} at {marker:?}");
        }

        Ok(TaskResult {
            response: format!("spawn requested for nodes: {node_ids:?}"),
            next_action: NextAction::GoTo("assess".to_string()),
            task_id: self.id().to_string(),
            status_message: None,
        })
    }
}

/// Task: merge the next ready PR via GitHub REST API.
pub struct MergeTask {}

impl MergeTask {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for MergeTask {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Task for MergeTask {
    fn id(&self) -> &str {
        "merge"
    }

    async fn run(
        &self,
        context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        use std::path::PathBuf;

        let session_dir = context
            .get_sync::<PathBuf>("session_dir")
            .ok_or("MergeTask: session_dir not in context")?;
        let node_id = context
            .get_sync::<String>("merge_node_id")
            .ok_or("MergeTask: merge_node_id not in context")?;
        let pr_number = context
            .get_sync::<u64>("merge_pr_number")
            .ok_or("MergeTask: merge_pr_number not in context")?;
        let repo = context.get_sync::<String>("repo").unwrap_or_default();
        let gh = super::github::RealGithubPrApi::new(&repo);

        let _sha =
            super::bridge::execute_stack_merge(&session_dir, &node_id, pr_number, &gh)?;

        Ok(TaskResult {
            response: format!("merged node {node_id} PR #{pr_number}"),
            next_action: NextAction::GoTo("repoint".to_string()),
            task_id: self.id().to_string(),
            status_message: None,
        })
    }
}

/// Task: repoint dependent PRs after a parent has been merged.
pub struct RepointTask {}

impl RepointTask {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for RepointTask {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Task for RepointTask {
    fn id(&self) -> &str {
        "repoint"
    }

    async fn run(
        &self,
        context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        use std::path::PathBuf;

        let session_dir = context
            .get_sync::<PathBuf>("session_dir")
            .ok_or("RepointTask: session_dir not in context")?;
        let repo_root = context
            .get_sync::<PathBuf>("repo_root")
            .unwrap_or_else(|| session_dir.clone());
        let merged_node_id = context
            .get_sync::<String>("merge_node_id")
            .unwrap_or_default();
        let dependents = context
            .get_sync::<Vec<String>>("merge_dependents")
            .unwrap_or_default();
        let default_branch = context
            .get_sync::<String>("default_branch")
            .unwrap_or_else(|| "master".to_string());
        let repo = context.get_sync::<String>("repo").unwrap_or_default();
        let gh = super::github::RealGithubPrApi::new(&repo);

        super::bridge::execute_stack_repoint(
            &session_dir,
            &repo_root,
            &merged_node_id,
            &dependents,
            &default_branch,
            &gh,
        )?;

        Ok(TaskResult {
            response: "repoint complete — returning to assess".to_string(),
            next_action: NextAction::GoTo("assess".to_string()),
            task_id: self.id().to_string(),
            status_message: None,
        })
    }
}
