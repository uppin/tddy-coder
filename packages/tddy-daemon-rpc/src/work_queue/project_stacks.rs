//! A project's PR stacks, addressed by id (`#orchestrators` 9/11): `ListProjectWorkQueues`,
//! `GetWorkQueue`, and the resolution every other PR-stack request goes through to find the one
//! queue it acts on — a [`WorkQueueAddress`] (its `queue_id` is the owning session's id), or a
//! `session_id` naming the owner.
//!
//! Kept out of `ports.rs`, which only delegates here: that file is over its size budget
//! (`docs/code-issues/oversized-file-pr-stack-ports.md`).
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tddy_core::changeset::WorkQueue;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::work_queue_store::{WorkQueueRecord, WorkQueueStore, WorkQueueStoreError};
use tddy_projects::project_storage;
use tddy_rpc::Status;
use tddy_service::proto::work_queue::{WorkItemCounts, WorkQueueAddress, WorkQueueSummary};
use tddy_session_lifecycle::user_sessions_path::{projects_path_for_user, sessions_base_for_user};

use super::WorkQueueRpcHandler;

/// The one stack a PR-stack request acts on, as the request named it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WorkQueueTarget {
    /// The stack's own address — served by the daemon it names.
    Address(WorkQueueAddress),
    /// The session that owns the queue (a queue's id is its owner's), for a caller that holds no
    /// address.
    Session(String),
}

/// Which stack a request names: its `stack` address when it carries one, else its `session_id`.
///
/// Refused as `InvalidArgument` when it names neither, or names an address missing its project or
/// stack id — an address is never completed from a guess.
pub(crate) fn work_queue_target(
    stack: Option<&WorkQueueAddress>,
    session_id: &str,
) -> Result<WorkQueueTarget, Status> {
    if let Some(address) = stack {
        if address.project_id.trim().is_empty() || address.queue_id.trim().is_empty() {
            return Err(Status::invalid_argument(
                "a stack address must name its project and stack",
            ));
        }
        return Ok(WorkQueueTarget::Address(address.clone()));
    }
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err(Status::invalid_argument(
            "a PR-stack request must name a stack address or a session",
        ));
    }
    Ok(WorkQueueTarget::Session(session_id.to_string()))
}

/// How far `stack` has come: every node, the nodes a child session materialized (a session or a
/// branch recorded), and the nodes whose pull request merged.
pub(crate) fn node_counts_of(stack: &WorkQueue) -> WorkItemCounts {
    WorkItemCounts {
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
    record: &WorkQueueRecord,
    daemon_instance_id: &str,
    referencing_session_ids: Vec<String>,
) -> WorkQueueSummary {
    WorkQueueSummary {
        address: Some(WorkQueueAddress {
            project_id: record.project_id.clone(),
            queue_id: record.queue_id.clone(),
            daemon_instance_id: daemon_instance_id.to_string(),
        }),
        title: record.title.clone().unwrap_or_default(),
        node_counts: Some(node_counts_of(&record.stack)),
        referencing_session_ids,
        created_by_session_id: record.owner_session_id().to_string(),
        created_at_unix_ms: record.created_at_unix_ms,
    }
}

pub(crate) fn store_status(error: WorkQueueStoreError) -> Status {
    match error {
        WorkQueueStoreError::InvalidProjectId(_) | WorkQueueStoreError::InvalidStackId(_) => {
            Status::invalid_argument(error.to_string())
        }
        WorkQueueStoreError::NotFound { .. } => Status::not_found(error.to_string()),
        other => Status::internal(other.to_string()),
    }
}

/// The guard on a `session_id`-addressed mutation (`#workqueue` 5/14): the session must own a work
/// queue. Nothing here creates one — a queue exists only because its session was started with one —
/// so a session owning none is refused with the hint naming how a queue comes to exist.
pub(crate) fn require_a_stack(session_dir: &Path) -> Result<(), Status> {
    match tddy_core::changeset::read_session_stack(session_dir) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(Status::failed_precondition(format!(
            "session {} has no work queue — {}",
            session_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            tddy_core::session_work_queue::NO_WORK_QUEUE_HINT
        ))),
        Err(e) => Err(Status::invalid_argument(e.to_string())),
    }
}

/// Sessions on this host that own a work queue, by queue id — a queue's id is its owning session's,
/// so each queue is referenced by that one session.
pub(super) fn referencing_sessions_by_stack(sessions_base: &Path) -> HashMap<String, Vec<String>> {
    let mut by_stack: HashMap<String, Vec<String>> = HashMap::new();
    let sessions_dir = sessions_base.join("sessions");
    let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
        return by_stack;
    };
    for entry in entries.flatten() {
        let owns_a_queue = entry
            .path()
            .join(tddy_core::session_work_queue::WORK_QUEUE_FILE)
            .is_file();
        if owns_a_queue {
            let session_id = entry.file_name().to_string_lossy().into_owned();
            by_stack.insert(session_id.clone(), vec![session_id]);
        }
    }
    by_stack
}

pub(super) fn next_free_node_id(stack: &WorkQueue) -> String {
    let max = stack
        .nodes
        .iter()
        .filter_map(|node| node.node_id.strip_prefix('n')?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("n{}", max + 1)
}

impl WorkQueueRpcHandler {
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

    pub(crate) fn stack_store(&self, sessions_base: &Path) -> WorkQueueStore {
        WorkQueueStore::new(sessions_base)
    }

    /// Forward `req` to the daemon its target's address names, when the target is an address
    /// naming a peer; `Ok(None)` when this daemon is the one to serve it.
    pub(crate) async fn forward_if_addressed<Req, Resp>(
        &self,
        rpc_name: &str,
        target: &WorkQueueTarget,
        req: &Req,
    ) -> Result<Option<Resp>, Status>
    where
        Req: prost::Message,
        Resp: prost::Message + Default,
    {
        let WorkQueueTarget::Address(address) = target else {
            return Ok(None);
        };
        self.forward_addressed(rpc_name, &address.daemon_instance_id, req)
            .await
    }

    /// The session directory a target is served through on this host: an address resolves to a
    /// session naming its stack, a session to its own directory. Reads are served for a session
    /// naming no stack; the mutations guard their own entry with [`require_a_stack`].
    pub(crate) fn session_dir_for_target(
        &self,
        sessions_base: &Path,
        target: &WorkQueueTarget,
    ) -> Result<PathBuf, Status> {
        match target {
            WorkQueueTarget::Address(address) => {
                self.session_dir_for_address(sessions_base, address)
                    .ok_or_else(|| {
                        Status::failed_precondition(
                            "no session on this host names the stack, so its repository could not be resolved",
                        )
                    })
            }
            WorkQueueTarget::Session(session_id) => {
                validate_session_id_segment(session_id)
                    .map_err(|e| Status::invalid_argument(e.message()))?;
                let dir = unified_session_dir_path(sessions_base, session_id);
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
        target: &WorkQueueTarget,
    ) -> Result<Option<PathBuf>, Status> {
        match target {
            WorkQueueTarget::Address(address) => {
                let projects_dir = projects_path_for_user(os_user, Some(&self.tddy_data_dir))
                    .ok_or_else(|| Status::internal("could not resolve projects path"))?;
                Ok(
                    project_storage::find_project(&projects_dir, &address.project_id)
                        .map_err(|e| Status::internal(e.to_string()))?
                        .map(|project| PathBuf::from(&project.main_repo_path)),
                )
            }
            WorkQueueTarget::Session(_) => Ok(tddy_core::repo_root_for_session(
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
                tddy_work_queue::WORK_QUEUE_SERVICE,
                rpc_name,
                daemon_instance_id,
                req,
            )
            .await
    }

    /// The session directory a local address is served through: the owning session's — a queue's id
    /// is its owner's — when it is on this host and the queue belongs to the address's project.
    pub(crate) fn session_dir_for_address(
        &self,
        sessions_base: &Path,
        address: &WorkQueueAddress,
    ) -> Option<PathBuf> {
        let store = self.stack_store(sessions_base);
        let record = store.read(&address.project_id, &address.queue_id).ok()?;
        let owner = record.owner_session_id();
        validate_session_id_segment(owner).ok()?;
        Some(unified_session_dir_path(sessions_base, owner))
    }
}

#[cfg(test)]
#[path = "project_stacks_unit_tests.rs"]
mod project_stacks_unit_tests;
