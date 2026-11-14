//! The PR-stack git tools ([`tddy_tool_engine::stack_git_tools`]) answered on this daemon
//! (`#orchestrators` 10/11).
//!
//! Routed from `run_locally` beside the plan tools ([`super::stack_tools`]) and classified the same
//! way (`stack_caller_of`): only an orchestrator-role caller is served, and a call naming another
//! host with `daemon_instance_id` is forwarded whole to that host, one hop.
//!
//! Every git step runs **here, in the node's own worktree** — the worktree of the project's
//! repository that holds the node's branch — never in the caller's checkout:
//!
//! - a rebase (`pr_repoint`, `pr_set_parents`) is refused, untouched, while that worktree has
//!   uncommitted changes or the node's session is running, then force-pushes with lease to the
//!   project's remote with the project account's token ([`tddy_worktree_service::session_git_ops::PushIdentity`]);
//! - `pr_resolve_conflicts` finds the conflicting paths without leaving a merge behind, and with
//!   `delegate` hands them to the node's session through `SendSessionMessage` (running) or
//!   `ResumeSession` with the message (stopped) — `#orchestrators` 4/11's turn port;
//! - `pr_spawn_child` starts the node's session through the daemon's start path, its stack parent
//!   the caller;
//! - `pr_merge` / `pr_close` reach GitHub as the project's account.
//!
//! A merge-and-repoint keeps its crash-safety journal: beside the stack
//! (`<projects>/<project_id>/stacks/<stack_id>.op.json`) for an operation an orchestrator started.
//!
//! Feature doc: docs/ft/coder/pr-stacking.md § Merge and repoint.
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-git-tools.md.

use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;
use tddy_rpc::Status;
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_tool_engine::stack_git_tools::{StackGitOp, StackGitToolPort};
use tddy_tool_engine::stack_tools::StackCaller;

use super::project_home::DaemonProjectHome;
use super::stack_tools::DaemonStackTools;
use super::ExecToolRpcHandler;

impl ExecToolRpcHandler {
    /// Serve `req`'s stack git tool: classify the caller, refuse one that is not an orchestrator,
    /// forward a call naming another host, else run it on this host's stack and record the call.
    #[allow(dead_code)] // TODO(stack-git-tools): routed from `run_locally` once implemented.
    pub(super) async fn run_stack_git_tool(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
    ) -> Result<ExecuteToolResponse, Status> {
        // TODO(stack-git-tools): implement — `stack_caller_of`, the role refusal
        // (`is_stack_git_tool_offered_to`), forwarding by `daemon_instance_id`,
        // `execute_stack_git_tool` over a `DaemonStackGitTools`, `record_call`.
        let _ = sessions_base;
        Err(Status::unimplemented(format!(
            "tool {} is not served on this host yet",
            req.tool_name
        )))
    }
}

/// [`StackGitToolPort`] over this host's stacks and the nodes' worktrees, for one caller: the plan
/// tools' port for the store and GitHub, and the project-home port for the node's session (its
/// target, its turn, its resume, a start).
#[allow(dead_code)] // TODO(stack-git-tools): built by `run_stack_git_tool` once implemented.
pub(super) struct DaemonStackGitTools {
    pub(super) stacks: DaemonStackTools,
    pub(super) home: DaemonProjectHome,
}

#[async_trait]
impl StackGitToolPort for DaemonStackGitTools {
    async fn run_stack_git_op(
        &self,
        caller: &StackCaller,
        stack_id: &str,
        op: StackGitOp,
    ) -> Result<Value, String> {
        // TODO(stack-git-tools): implement — resolve the node and its worktree
        // (`tddy_git::worktree_path_for_branch` in the project's repository), refuse a dirty or
        // running node before a rebase, run the op, push with the project's `PushIdentity`, write
        // the store, and answer the shapes `tddy-tools` answered.
        let _ = (caller, stack_id, op);
        Err("the stack git tools are not served on this host yet".to_string())
    }
}
