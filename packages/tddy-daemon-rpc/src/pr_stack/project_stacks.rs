//! A project's PR stacks, addressed by id (`#orchestrators` 9/11): `ListProjectStacks`,
//! `GetProjectStack`, and the resolution every other PR-stack request goes through to find the one
//! stack it acts on — a [`StackAddress`], or an orchestrator `session_id` resolved through the
//! stack its changeset names.
//!
//! Kept out of `ports.rs`, which only delegates here: that file is over its size budget
//! (`docs/code-issues/oversized-file-pr-stack-ports.md`).
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md

use tddy_core::changeset::Stack;
use tddy_core::project_stack_store::StackRecord;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::pr_stack::{
    GetProjectStackRequest, GetProjectStackResponse, ListProjectStacksRequest,
    ListProjectStacksResponse, ProjectStackSummary, StackAddress, StackNodeCounts,
};

use super::PrStackRpcHandler;

/// The one stack a PR-stack request acts on, as the request named it.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // TODO(stack-web-by-id): read by every PR-stack handler once implemented.
pub(crate) enum StackTarget {
    /// The stack's own address — served by the daemon it names.
    Address(StackAddress),
    /// An orchestrator session, resolved through the stack its changeset names
    /// (`Changeset.stack_id`). Kept so a caller that has no stack id yet still works.
    Session(String),
}

/// Which stack a request names: its `stack` address when it carries one, else its `session_id`.
///
/// Refused as `InvalidArgument` when it names neither, or names an address missing its project or
/// stack id — an address is never completed from a guess.
#[allow(dead_code)] // TODO(stack-web-by-id): reached from every PR-stack handler once implemented.
pub(crate) fn stack_target(
    stack: Option<&StackAddress>,
    session_id: &str,
) -> Result<StackTarget, Status> {
    // TODO(stack-web-by-id): implement — the address wins; a blank address field and a request
    // naming neither are refused. Until then every request resolves by its session, as today.
    let _ = stack;
    Ok(StackTarget::Session(session_id.trim().to_string()))
}

/// How far `stack` has come: every node, the nodes a child session materialized (a session or a
/// branch recorded), and the nodes whose pull request merged.
#[allow(dead_code)] // TODO(stack-web-by-id): reached from `summary_of` once implemented.
pub(crate) fn node_counts_of(stack: &Stack) -> StackNodeCounts {
    // TODO(stack-web-by-id): implement
    let _ = stack;
    StackNodeCounts::default()
}

/// The drawer's view of one stack record held by `daemon_instance_id`, naming the sessions there
/// whose changeset references it.
#[allow(dead_code)] // TODO(stack-web-by-id): reached from both RPCs once implemented.
pub(crate) fn summary_of(
    record: &StackRecord,
    daemon_instance_id: &str,
    referencing_session_ids: Vec<String>,
) -> ProjectStackSummary {
    // TODO(stack-web-by-id): implement — address, title, node counts, referencing sessions, creator.
    let _ = (record, daemon_instance_id, referencing_session_ids);
    ProjectStackSummary::default()
}

impl PrStackRpcHandler {
    /// `ListProjectStacks`: this daemon's own stacks of the project, plus — unless the request is a
    /// peer's `this_host_only` copy — every eligible peer's, asked concurrently, with each peer that
    /// did not answer named in `unreachable_hosts`.
    pub(super) async fn list_project_stacks_rpc(
        &self,
        request: Request<ListProjectStacksRequest>,
    ) -> Result<Response<ListProjectStacksResponse>, Status> {
        // TODO(stack-web-by-id): implement
        let _ = request;
        Err(Status::unimplemented(
            "ListProjectStacks is not implemented yet (TODO(stack-web-by-id))",
        ))
    }

    /// `GetProjectStack`: the stack at `stack`, forwarded to the daemon the address names when that
    /// is a peer.
    pub(super) async fn get_project_stack_rpc(
        &self,
        request: Request<GetProjectStackRequest>,
    ) -> Result<Response<GetProjectStackResponse>, Status> {
        // TODO(stack-web-by-id): implement
        let _ = request;
        Err(Status::unimplemented(
            "GetProjectStack is not implemented yet (TODO(stack-web-by-id))",
        ))
    }
}

#[cfg(test)]
#[path = "project_stacks_unit_tests.rs"]
mod project_stacks_unit_tests;
