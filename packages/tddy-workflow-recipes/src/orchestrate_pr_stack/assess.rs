//! Assess task: reads the stack DAG + child session states + GitHub PR states,
//! then decides the next orchestrator action.

use std::path::Path;

use async_trait::async_trait;
use tddy_core::changeset::Stack;
use tddy_core::workflow::ids::WorkflowState;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::task::{Task, TaskResult, NextAction};

use super::github::GithubPrApi;

/// Coarse phase of a single child PR node as seen by the orchestrator.
#[derive(Debug, Clone, PartialEq)]
pub enum ChildPhase {
    NotSpawned,
    Building,
    ReadyForPr,
    PrOpen,
    Failed(String),
}

/// Live GitHub PR status for one stack node branch.
#[derive(Debug, Clone, PartialEq)]
pub enum PrLiveStatus {
    None,
    Open { number: u64, base: String },
    Queued,
    Merged,
    Closed,
}

/// The orchestrator's decision for this tick.
#[derive(Debug, Clone, PartialEq)]
pub enum OrchestratorAction {
    Spawn { node_ids: Vec<String> },
    Wait { reason: String },
    Merge { node_id: String, pr_number: u64 },
    MarkFailed { node_id: String, reason: String },
    Done,
}

/// Snapshot of one stack node as seen during an assess tick.
#[derive(Debug, Clone)]
pub struct NodeView {
    pub node_id: String,
    pub branch: String,
    pub parent_dep_ids: Vec<String>,
    pub child_session_id: Option<String>,
    pub child_state: Option<WorkflowState>,
    pub child_phase: ChildPhase,
    pub pr: PrLiveStatus,
}

/// Pure decision function: given assembled views, pick the next orchestrator action.
pub fn decide_next_action(views: &[NodeView]) -> OrchestratorAction {
    unimplemented!("decide_next_action: not yet implemented")
}

/// Effective base ref for a node (skips merged ancestors; returns default_branch when all merged).
pub fn effective_base_ref(node_id: &str, views: &[NodeView], default_branch: &str) -> String {
    unimplemented!("effective_base_ref: not yet implemented")
}

/// Assemble NodeView list from orchestrator changeset + child changesets + live GitHub.
pub fn assemble_views(
    parent_dir: &Path,
    sessions_root: &Path,
    stack: &Stack,
    gh: &dyn GithubPrApi,
    default_branch: &str,
) -> Result<Vec<NodeView>, tddy_core::WorkflowError> {
    unimplemented!("assemble_views: not yet implemented")
}

/// The assess Task: reads stack, calls assemble_views + decide_next_action, returns GoTo.
pub struct AssessTask {}

impl AssessTask {
    pub fn new() -> Self { Self {} }
}

impl Default for AssessTask {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Task for AssessTask {
    fn id(&self) -> &str { "assess" }

    async fn run(
        &self,
        _context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        unimplemented!("AssessTask::run: not yet implemented")
    }
}
