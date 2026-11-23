//! `SetWorkQueueDependencies` and `GetWorkQueueGraph` (`#workqueue` 9/14): a queue's lineage.
//!
//! Dependency edges are stored on the queue (an owner write, refused when they would close a cycle).
//! Line edges are never stored: each item's PR base is read through the daemon's GitHub inbox with the
//! project's credential, and an item without a readable PR shows its planned base. When the inbox
//! cannot be reached the answer says so (`unavailable_reason`) and carries the planned bases.

use super::project_stacks::store_status;
use super::WorkQueueRpcHandler;
use std::path::Path;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    DependencyEdgeView, GetWorkQueueGraphRequest, LineEdgeView, SetWorkQueueDependenciesRequest,
    WorkItemLinksResponse, WorkQueueGraphResponse,
};
use tddy_work_queue::lineage::{self, LineageError, QueueGraph};

fn lineage_refusal(error: LineageError) -> Status {
    match error {
        LineageError::NoSuchItem(_) => Status::not_found(error.to_string()),
        LineageError::Cycle(_) => Status::failed_precondition(error.to_string()),
    }
}

fn graph_response(graph: QueueGraph, unavailable_reason: String) -> WorkQueueGraphResponse {
    WorkQueueGraphResponse {
        line_edges: graph
            .line
            .into_iter()
            .map(|(item_id, endpoint)| LineEdgeView {
                item_id,
                kind: endpoint.kind().to_string(),
                endpoint: endpoint.name().to_string(),
            })
            .collect(),
        dependencies: graph
            .dependencies
            .into_iter()
            .map(|edge| DependencyEdgeView {
                from: edge.from,
                to: edge.to,
            })
            .collect(),
        unavailable_reason,
        gh_stack_branches: Vec::new(),
    }
}

impl WorkQueueRpcHandler {
    pub(super) async fn set_work_queue_dependencies_rpc(
        &self,
        request: Request<SetWorkQueueDependenciesRequest>,
    ) -> Result<Response<WorkItemLinksResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, WorkItemLinksResponse>(
                "SetWorkQueueDependencies",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let item_id = req.item_id.trim();
        if item_id.is_empty() {
            return Err(Status::invalid_argument("item_id is required"));
        }
        let needs = req.needs.iter().map(|n| n.trim().to_string()).collect();
        self.edit_queue(
            &call,
            |queue| lineage::set_dependencies(queue, item_id, needs),
            lineage_refusal,
        )
    }

    pub(super) async fn get_work_queue_graph_rpc(
        &self,
        request: Request<GetWorkQueueGraphRequest>,
    ) -> Result<Response<WorkQueueGraphResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, WorkQueueGraphResponse>(
                "GetWorkQueueGraph",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let sessions_base = self.sessions_base(&call.os_user)?;
        let store = self.stack_store(&sessions_base);
        let record = store
            .read(&call.address.project_id, &call.address.queue_id)
            .map_err(store_status)?;
        let queue = record.stack;
        let integration_base =
            tddy_projects::project_storage::effective_integration_base_ref_for_project(
                &store.projects_dir(),
                &call.address.project_id,
            )
            .map_err(|e| {
                Status::failed_precondition(format!(
                    "the project's trunk could not be resolved: {e:#}"
                ))
            })?;
        let trunk = lineage::trunk_branch_of(&integration_base);
        let site = self.pull_requests.site_for_session(
            &call.os_user,
            &call.address.queue_id,
            &call.address.project_id,
            &req.session_token,
        );
        let response = match site {
            Ok(site) => {
                let graph = lineage::queue_graph(&site.inbox, &site.repo, &queue, trunk).await;
                let gh_stack = self
                    .gh_stack_of(&sessions_base, &call.address.queue_id, &queue)
                    .await;
                WorkQueueGraphResponse {
                    gh_stack_branches: gh_stack,
                    ..graph_response(graph, String::new())
                }
            }
            // The queue is real, its GitHub is not readable: the planned bases stand, and say why.
            Err(status) => graph_response(
                QueueGraph {
                    line: lineage::planned_line_edges(&queue, trunk),
                    dependencies: lineage::dependencies_of(&queue),
                },
                status.message().to_string(),
            ),
        };
        Ok(Response::new(response))
    }

    /// The GitHub stack the queue's first item with a branch sits in, read through the host's `gh`
    /// in the owning session's checkout — optional input: any miss is an empty list.
    async fn gh_stack_of(
        &self,
        sessions_base: &Path,
        owner_session_id: &str,
        queue: &tddy_core::changeset::WorkQueue,
    ) -> Vec<String> {
        let Some(branch) = queue.nodes.iter().find_map(|item| item.branch.clone()) else {
            return Vec::new();
        };
        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, owner_session_id);
        let Some(repo_root) = tddy_core::repo_root_for_session(&session_dir) else {
            return Vec::new();
        };
        tddy_github::gh_stack::gh_stack_membership(Path::new("gh"), &repo_root, &branch)
            .await
            .unwrap_or_default()
    }
}
