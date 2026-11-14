//! A project's PR stacks, addressed by id (`#orchestrators` 9/11): `ListProjectStacks`,
//! `GetProjectStack`, and the resolution every other PR-stack request goes through to find the one
//! stack it acts on — a [`StackAddress`], or an orchestrator `session_id` resolved through the
//! stack its changeset names.
//!
//! Kept out of `ports.rs`, which only delegates here: that file is over its size budget
//! (`docs/code-issues/oversized-file-pr-stack-ports.md`).
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tddy_core::changeset::{Stack, StackNode};
use tddy_core::project_stack_store::{ProjectStackStore, StackRecord, StackStoreError};
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_projects::project_storage;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::pr_stack::{
    AddPlannedPrRequest, AddPlannedPrResponse, GetProjectStackRequest, GetProjectStackResponse,
    ListProjectStacksRequest, ListProjectStacksResponse, ProjectStackSummary, StackAddress,
    StackNodeCounts, UnreachableStackHost,
};
use tddy_session_lifecycle::user_sessions_path::{projects_path_for_user, sessions_base_for_user};

use super::PrStackRpcHandler;

/// The one stack a PR-stack request acts on, as the request named it.
#[derive(Debug, Clone, PartialEq)]
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
pub(crate) fn stack_target(
    stack: Option<&StackAddress>,
    session_id: &str,
) -> Result<StackTarget, Status> {
    if let Some(address) = stack {
        if address.project_id.trim().is_empty() || address.stack_id.trim().is_empty() {
            return Err(Status::invalid_argument(
                "a stack address must name its project and stack",
            ));
        }
        return Ok(StackTarget::Address(address.clone()));
    }
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err(Status::invalid_argument(
            "a PR-stack request must name a stack address or a session",
        ));
    }
    Ok(StackTarget::Session(session_id.to_string()))
}

/// How far `stack` has come: every node, the nodes a child session materialized (a session or a
/// branch recorded), and the nodes whose pull request merged.
pub(crate) fn node_counts_of(stack: &Stack) -> StackNodeCounts {
    StackNodeCounts {
        total: stack.nodes.len() as u32,
        started: stack
            .nodes
            .iter()
            .filter(|node| node.session_id.is_some() || node.branch.is_some())
            .count() as u32,
        merged: stack
            .nodes
            .iter()
            .filter(|node| {
                node.pr_status
                    .as_ref()
                    .is_some_and(|status| status.phase == "merged")
            })
            .count() as u32,
    }
}

/// The drawer's view of one stack record held by `daemon_instance_id`, naming the sessions there
/// whose changeset references it.
pub(crate) fn summary_of(
    record: &StackRecord,
    daemon_instance_id: &str,
    referencing_session_ids: Vec<String>,
) -> ProjectStackSummary {
    ProjectStackSummary {
        address: Some(StackAddress {
            project_id: record.project_id.clone(),
            stack_id: record.stack_id.clone(),
            daemon_instance_id: daemon_instance_id.to_string(),
        }),
        title: record.title.clone().unwrap_or_default(),
        node_counts: Some(node_counts_of(&record.stack)),
        referencing_session_ids,
        created_by_session_id: record.created_by_session_id.clone(),
        created_at_unix_ms: record.created_at_unix_ms,
    }
}

pub(crate) fn store_status(error: StackStoreError) -> Status {
    match error {
        StackStoreError::InvalidProjectId(_) | StackStoreError::InvalidStackId(_) => {
            Status::invalid_argument(error.to_string())
        }
        StackStoreError::NotFound { .. } => Status::not_found(error.to_string()),
        other => Status::internal(other.to_string()),
    }
}

/// The recipe guard, relaxed for a session that already names a stack: such a session resolves
/// through its `stack_id` whatever its recipe, and only a session naming no stack still needs the
/// `pr-stack` recipe, because its first write creates the stack.
pub(crate) fn require_orchestrator_unless_stacked(session_dir: &Path) -> Result<(), Status> {
    let changeset = tddy_core::read_changeset(session_dir)
        .map_err(|e| Status::invalid_argument(e.to_string()))?;
    if changeset.stack_id.is_some() {
        return Ok(());
    }
    super::guards::require_pr_stack_orchestrator(session_dir)
}

/// Sessions on this host whose changeset names each stack, by stack id.
fn referencing_sessions_by_stack(sessions_base: &Path) -> HashMap<String, Vec<String>> {
    let mut by_stack: HashMap<String, Vec<String>> = HashMap::new();
    let sessions_dir = sessions_base.join("sessions");
    let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
        return by_stack;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(changeset) = tddy_core::read_changeset(&dir) else {
            continue;
        };
        let Some(stack_id) = changeset.stack_id else {
            continue;
        };
        let session_id = entry.file_name().to_string_lossy().into_owned();
        by_stack.entry(stack_id).or_default().push(session_id);
    }
    for ids in by_stack.values_mut() {
        ids.sort();
    }
    by_stack
}

fn next_free_node_id(stack: &Stack) -> String {
    let max = stack
        .nodes
        .iter()
        .filter_map(|node| node.node_id.strip_prefix('n')?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("n{}", max + 1)
}

impl PrStackRpcHandler {
    pub(crate) fn local_instance_id(&self) -> String {
        tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(&self.config)
    }

    pub(crate) fn authenticated_os_user(&self, session_token: &str) -> Result<String, Status> {
        Ok(self.authenticated_user(session_token)?.1)
    }

    pub(crate) fn authenticated_user(
        &self,
        session_token: &str,
    ) -> Result<(String, String), Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        Ok((github_user, os_user))
    }

    pub(crate) fn sessions_base(&self, os_user: &str) -> Result<PathBuf, Status> {
        sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))
    }

    pub(crate) fn stack_store(&self, sessions_base: &Path) -> ProjectStackStore {
        ProjectStackStore::new(sessions_base.join("projects"))
    }

    /// Forward `req` to the daemon its target's address names, when the target is an address
    /// naming a peer; `Ok(None)` when this daemon is the one to serve it.
    pub(crate) async fn forward_if_addressed<Req, Resp>(
        &self,
        rpc_name: &str,
        target: &StackTarget,
        req: &Req,
    ) -> Result<Option<Resp>, Status>
    where
        Req: prost::Message,
        Resp: prost::Message + Default,
    {
        let StackTarget::Address(address) = target else {
            return Ok(None);
        };
        self.forward_addressed(rpc_name, &address.daemon_instance_id, req)
            .await
    }

    /// The session directory a target is served through on this host: an address resolves to a
    /// session naming its stack, a session to its own directory (guarded as
    /// [`require_orchestrator_unless_stacked`] guards it).
    pub(crate) fn session_dir_for_target(
        &self,
        sessions_base: &Path,
        target: &StackTarget,
    ) -> Result<PathBuf, Status> {
        match target {
            StackTarget::Address(address) => {
                self.session_dir_for_address(sessions_base, address)
                    .ok_or_else(|| {
                        Status::failed_precondition(
                            "no session on this host names the stack, so its repository could not be resolved",
                        )
                    })
            }
            StackTarget::Session(session_id) => {
                validate_session_id_segment(session_id)
                    .map_err(|e| Status::invalid_argument(e.message()))?;
                let dir = unified_session_dir_path(sessions_base, session_id);
                require_orchestrator_unless_stacked(&dir)?;
                Ok(dir)
            }
        }
    }

    /// The checkout a target's repository legs read: an address resolves through its project's
    /// registered checkout, a session through its own record.
    pub(crate) fn repo_root_for_target(
        &self,
        sessions_base: &Path,
        os_user: &str,
        target: &StackTarget,
    ) -> Result<Option<PathBuf>, Status> {
        match target {
            StackTarget::Address(address) => {
                let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                    .ok_or_else(|| Status::internal("could not resolve projects path"))?;
                Ok(
                    project_storage::find_project(&projects_dir, &address.project_id)
                        .map_err(|e| Status::internal(e.to_string()))?
                        .map(|project| PathBuf::from(&project.main_repo_path)),
                )
            }
            StackTarget::Session(_) => Ok(tddy_core::repo_root_for_session(
                &self.session_dir_for_target(sessions_base, target)?,
            )),
        }
    }

    /// Forward `req` to the daemon `address` names when that is a peer; `Ok(None)` when this
    /// daemon is the one to serve it.
    pub(crate) async fn forward_addressed<Req, Resp>(
        &self,
        rpc_name: &str,
        daemon_instance_id: &str,
        req: &Req,
    ) -> Result<Option<Resp>, Status>
    where
        Req: prost::Message,
        Resp: prost::Message + Default,
    {
        self.peer_routing
            .rpc_served_by_peer(
                tddy_workflow_recipes::PR_STACK_SERVICE,
                rpc_name,
                daemon_instance_id,
                req,
            )
            .await
    }

    /// The session directory a local address is served through, when a session on this host names
    /// the stack (its creator first, then any referencing session).
    pub(crate) fn session_dir_for_address(
        &self,
        sessions_base: &Path,
        address: &StackAddress,
    ) -> Option<PathBuf> {
        let store = self.stack_store(sessions_base);
        let record = store.read(&address.project_id, &address.stack_id).ok()?;
        let mut candidates = vec![record.created_by_session_id.clone()];
        candidates.extend(
            referencing_sessions_by_stack(sessions_base)
                .get(&address.stack_id)
                .cloned()
                .unwrap_or_default(),
        );
        candidates.into_iter().find_map(|session_id| {
            if validate_session_id_segment(&session_id).is_err() {
                return None;
            }
            let dir = unified_session_dir_path(sessions_base, &session_id);
            // Only a session whose changeset names this stack can serve through its directory.
            let names_stack = tddy_core::read_changeset(&dir)
                .map(|c| c.stack_id.as_deref() == Some(address.stack_id.as_str()))
                .unwrap_or(false);
            names_stack.then_some(dir)
        })
    }

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
        let os_user = self.authenticated_os_user(&req.session_token)?;
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

#[cfg(test)]
#[path = "project_stacks_unit_tests.rs"]
mod project_stacks_unit_tests;
