use crate::pr_stack::project_stacks::next_free_node_id;
use crate::pr_stack::project_stacks::referencing_sessions_by_stack;
use crate::pr_stack::project_stacks::stack_target;
use crate::pr_stack::project_stacks::store_status;
use crate::pr_stack::project_stacks::summary_of;
use crate::pr_stack::project_stacks::StackTarget;
use crate::pr_stack::PrStackRpcHandler;
use std::path::Path;
use tddy_core::changeset::StackNode;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::pr_stack::{
    AddPlannedPrRequest, AddPlannedPrResponse, GetProjectStackRequest, GetProjectStackResponse,
    ListProjectStacksRequest, ListProjectStacksResponse, ProjectStackSummary, StackAddress,
    UnreachableStackHost,
};

impl PrStackRpcHandler {
    /// Append a planned PR to the stack at `address`, in this host's store.
    pub(super) fn add_planned_pr_to_store(
        &self,
        sessions_base: &Path,
        address: &StackAddress,
        req: &AddPlannedPrRequest,
    ) -> Result<AddPlannedPrResponse, Status> {
        let store = self.stack_store(sessions_base);
        let existing = store
            .read(&address.project_id, &address.stack_id)
            .map_err(store_status)?;
        for parent in &req.parents {
            if existing.stack.node(parent).is_none() {
                return Err(Status::invalid_argument(format!(
                    "dangling parent ref: {parent}"
                )));
            }
        }
        let node_id = next_free_node_id(&existing.stack);
        let branch_suggestion =
            (!req.branch_suggestion.trim().is_empty()).then(|| req.branch_suggestion.clone());
        let mut added: Option<StackNode> = None;
        let updated = store
            .update(&address.project_id, &address.stack_id, |stack| {
                tddy_workflow_recipes::pr_stack::assign_missing_display_order(stack);
                let display_order = stack
                    .nodes
                    .iter()
                    .filter_map(|node| node.display_order)
                    .max()
                    .map_or(0, |highest| highest.saturating_add(1));
                let node = StackNode {
                    node_id: node_id.clone(),
                    title: req.title.clone(),
                    description: req.description.clone(),
                    branch_suggestion,
                    parents: req.parents.clone(),
                    display_order: Some(display_order),
                    ..StackNode::default()
                };
                stack.nodes.push(node.clone());
                added = Some(node);
            })
            .map_err(store_status)?;
        let added = added.ok_or_else(|| Status::internal("the added node was not recorded"))?;
        let stack_plan_json =
            serde_json::to_string(&updated.stack).map_err(|e| Status::internal(e.to_string()))?;
        Ok(AddPlannedPrResponse {
            stack_plan_json,
            node_id: added.node_id,
        })
    }

    /// Move a node in the stack at `address`, in this host's store.
    pub(super) fn reorder_planned_pr_in_store(
        &self,
        sessions_base: &Path,
        address: &StackAddress,
        node_id: &str,
        direction: &str,
    ) -> Result<String, Status> {
        let up = match direction {
            "up" => true,
            "down" => false,
            other => {
                return Err(Status::invalid_argument(format!(
                    "unknown direction '{other}' — expected 'up' or 'down'"
                )))
            }
        };
        let store = self.stack_store(sessions_base);
        let updated = store
            .update(&address.project_id, &address.stack_id, |stack| {
                tddy_workflow_recipes::pr_stack::assign_missing_display_order(stack);
                let order = stack.display_order();
                let Some(position) = order.iter().position(|id| id == node_id) else {
                    return;
                };
                let neighbour = if up {
                    position.checked_sub(1)
                } else {
                    position.checked_add(1)
                }
                .and_then(|index| order.get(index).cloned());
                let Some(neighbour) = neighbour else { return };
                let moved = stack.node(node_id).and_then(|n| n.display_order);
                let displaced = stack.node(&neighbour).and_then(|n| n.display_order);
                if let (Some(moved), Some(displaced)) = (moved, displaced) {
                    for node in &mut stack.nodes {
                        if node.node_id == node_id {
                            node.display_order = Some(displaced);
                        } else if node.node_id == neighbour {
                            node.display_order = Some(moved);
                        }
                    }
                }
            })
            .map_err(store_status)?;
        // An unknown node is refused rather than silently left unmoved.
        if updated.stack.node(node_id).is_none() {
            return Err(Status::not_found(format!("node '{node_id}' not found")));
        }
        serde_json::to_string(&updated.stack).map_err(|e| Status::internal(e.to_string()))
    }

    /// `ListProjectStacks`: this daemon's own stacks of the project, plus — unless the request is a
    /// peer's `this_host_only` copy — every eligible peer's, asked concurrently, with each peer that
    /// did not answer named in `unreachable_hosts`.
    pub(super) async fn list_project_stacks_rpc(
        &self,
        request: Request<ListProjectStacksRequest>,
    ) -> Result<Response<ListProjectStacksResponse>, Status> {
        let req = request.into_inner();
        if req.project_id.trim().is_empty() {
            return Err(Status::invalid_argument("project_id is required"));
        }
        let os_user = self.authenticated_os_user(&req.session_token)?;
        let sessions_base = self.sessions_base(&os_user)?;
        let store = self.stack_store(&sessions_base);
        let records = store.list(req.project_id.trim()).map_err(store_status)?;
        let referencing = referencing_sessions_by_stack(&sessions_base);
        let local_id = self.local_instance_id();
        let mut stacks: Vec<ProjectStackSummary> = records
            .iter()
            .map(|record| {
                summary_of(
                    record,
                    &local_id,
                    referencing
                        .get(&record.stack_id)
                        .cloned()
                        .unwrap_or_default(),
                )
            })
            .collect();
        let mut unreachable_hosts: Vec<UnreachableStackHost> = Vec::new();
        if !req.this_host_only {
            let forward = ListProjectStacksRequest {
                session_token: req.session_token.clone(),
                project_id: req.project_id.clone(),
                this_host_only: true,
            };
            let peers: Vec<String> = self
                .peer_routing
                .eligible_instance_ids()
                .into_iter()
                .filter(|peer| peer != &local_id)
                .collect();
            let mut joins = tokio::task::JoinSet::new();
            for peer in peers {
                let routing = self.peer_routing.clone();
                let forward = forward.clone();
                joins.spawn(async move {
                    let answered = routing
                        .rpc_served_by_peer::<_, ListProjectStacksResponse>(
                            tddy_workflow_recipes::PR_STACK_SERVICE,
                            "ListProjectStacks",
                            &peer,
                            &forward,
                        )
                        .await;
                    (peer, answered)
                });
            }
            while let Some(joined) = joins.join_next().await {
                match joined {
                    Ok((_, Ok(Some(answer)))) => stacks.extend(answer.stacks),
                    Ok((peer, Err(status))) => unreachable_hosts.push(UnreachableStackHost {
                        daemon_instance_id: peer,
                        reason: status.message().to_string(),
                    }),
                    Ok((_, Ok(None))) => {}
                    Err(_) => {}
                }
            }
        }
        Ok(Response::new(ListProjectStacksResponse {
            stacks,
            unreachable_hosts,
        }))
    }

    /// `GetProjectStack`: the stack at `stack`, forwarded to the daemon the address names when that
    /// is a peer.
    pub(super) async fn get_project_stack_rpc(
        &self,
        request: Request<GetProjectStackRequest>,
    ) -> Result<Response<GetProjectStackResponse>, Status> {
        let req = request.into_inner();
        let os_user = self.authenticated_os_user(&req.session_token)?;
        let address = req
            .stack
            .clone()
            .ok_or_else(|| Status::invalid_argument("a stack address is required"))?;
        let StackTarget::Address(address) = stack_target(Some(&address), "")? else {
            unreachable!("a validated address resolves to an address")
        };
        if let Some(answered) = self
            .forward_addressed::<_, GetProjectStackResponse>(
                "GetProjectStack",
                &address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let sessions_base = self.sessions_base(&os_user)?;
        let store = self.stack_store(&sessions_base);
        let record = store
            .read(&address.project_id, &address.stack_id)
            .map_err(store_status)?;
        let referencing = referencing_sessions_by_stack(&sessions_base);
        let summary = summary_of(
            &record,
            &self.local_instance_id(),
            referencing
                .get(&record.stack_id)
                .cloned()
                .unwrap_or_default(),
        );
        let stack_plan_json =
            serde_json::to_string(&record.stack).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(GetProjectStackResponse {
            summary: Some(summary),
            stack_plan_json,
        }))
    }
}
