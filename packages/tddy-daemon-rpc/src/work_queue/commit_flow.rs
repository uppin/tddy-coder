//! `GetWorkItemInbox`, `GetWorkItemOutbox` and `GetWorkQueueCommitCounts` (`#workqueue` 11/14): what an
//! item is missing, what it owes, and the counts a queue row shows.
//!
//! Read from remote tips in the queue owner's checkout after a fetch (the analysis is
//! `tddy_work_queue::commit_flow`; nothing local is touched). Each item's line parent comes from its
//! PR's base through the GitHub inbox; when the inbox cannot be read the planned bases stand in and the
//! answer says so (`unavailable_reason`).

use std::path::{Path, PathBuf};

use super::WorkQueueRpcHandler;
use tddy_core::changeset::WorkQueue;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    GetWorkItemCommitFlowRequest, GetWorkQueueGraphRequest, InboxCommitView, ItemCommitCountsView,
    OutboxCommitView, OutboxSuccessorView, WorkItemInboxResponse, WorkItemOutboxResponse,
    WorkQueueCommitCountsResponse,
};
use tddy_work_queue::commit_flow::{
    self, CommitFlow, CommitFlowError, CommitSource, InboxEntry, ItemBranch, ItemCounts,
    OutboxEntry, Prediction,
};
use tddy_work_queue::lineage;
use tddy_worktree_service::pull_request_view::SessionPullRequests;

/// What a commit-flow read runs over: the queue's branch-owning items with their line parents, the
/// checkout to read in, and why the line edges are only the planned ones (empty when read from GitHub).
pub(crate) struct FlowSite {
    pub items: Vec<ItemBranch>,
    pub repo_root: PathBuf,
    pub unavailable_reason: String,
}

/// Where the line edges and the checkout come from, for the queue `queue_id` (its owning session).
pub(crate) struct FlowInputs<'a> {
    pub pull_requests: &'a dyn SessionPullRequests,
    pub os_user: &'a str,
    pub sessions_base: &'a Path,
    pub queue_id: &'a str,
    /// The session the GitHub site is registered for.
    pub caller_session_id: &'a str,
    pub project_id: &'a str,
    pub session_token: &'a str,
    pub queue: &'a WorkQueue,
    pub trunk: &'a str,
}

pub(crate) async fn flow_site(inputs: FlowInputs<'_>) -> Result<FlowSite, String> {
    let site = inputs.pull_requests.site_for_session(
        inputs.os_user,
        inputs.caller_session_id,
        inputs.project_id,
        inputs.session_token,
    );
    let (line, unavailable_reason) = match site {
        Ok(site) => (
            lineage::queue_graph(&site.inbox, &site.repo, inputs.queue, inputs.trunk)
                .await
                .line,
            String::new(),
        ),
        Err(status) => (
            lineage::planned_line_edges(inputs.queue, inputs.trunk),
            status.message().to_string(),
        ),
    };
    let session_dir = tddy_core::session_lifecycle::unified_session_dir_path(
        inputs.sessions_base,
        inputs.queue_id,
    );
    let repo_root = tddy_core::repo_root_for_session(&session_dir).ok_or_else(|| {
        "the queue's owning session has no checkout to read commits in".to_string()
    })?;
    Ok(FlowSite {
        items: commit_flow::item_branches(inputs.queue, &line),
        repo_root,
        unavailable_reason,
    })
}

/// Fetch, then run `read` over the site's items — git is blocking, and the clone's flow (and its
/// cache) is shared across requests.
pub(crate) async fn read_flow<T: Send + 'static>(
    site: &FlowSite,
    read: impl FnOnce(&CommitFlow, &[ItemBranch]) -> Result<T, CommitFlowError> + Send + 'static,
) -> Result<T, String> {
    let flow = commit_flow::shared_for(&site.repo_root);
    let items = site.items.clone();
    tokio::task::spawn_blocking(move || {
        flow.fetch()?;
        read(&flow, &items)
    })
    .await
    .map_err(|e| format!("the commit flow read failed to finish: {e}"))?
    .map_err(|e| e.to_string())
}

/// The refusal for an item the analysis has no branch for, naming why.
pub(crate) fn check_item(queue: &WorkQueue, site: &FlowSite, item_id: &str) -> Result<(), String> {
    let Some(node) = queue.nodes.iter().find(|n| n.node_id == item_id) else {
        return Err(format!("item '{item_id}' not found"));
    };
    if node.branch.is_none() {
        return Err(format!(
            "item '{item_id}' has no branch yet, so it has no commits to compare"
        ));
    }
    if !site.items.iter().any(|i| i.item_id == item_id) {
        return Err(format!("item '{item_id}' has no readable line parent"));
    }
    Ok(())
}

fn source_parts(source: &CommitSource) -> (&'static str, &str) {
    match source {
        CommitSource::Item(id) => ("item", id),
        CommitSource::Trunk(branch) => ("trunk", branch),
        CommitSource::External(branch) => ("external", branch),
    }
}

fn conflict_files(prediction: &Prediction) -> Vec<String> {
    match prediction {
        Prediction::Clean => Vec::new(),
        Prediction::Conflicts(files) => files.clone(),
    }
}

pub(crate) fn inbox_view(entries: Vec<InboxEntry>) -> Vec<InboxCommitView> {
    entries
        .into_iter()
        .map(|entry| {
            let (kind, source) = source_parts(&entry.source);
            InboxCommitView {
                sha: entry.commit.sha,
                subject: entry.commit.subject,
                source_kind: kind.to_string(),
                source: source.to_string(),
                conflict_files: conflict_files(&entry.prediction),
            }
        })
        .collect()
}

pub(crate) fn outbox_view(entries: Vec<OutboxEntry>) -> Vec<OutboxSuccessorView> {
    let mut successors: Vec<OutboxSuccessorView> = Vec::new();
    for entry in entries {
        let commit = OutboxCommitView {
            sha: entry.commit.sha,
            subject: entry.commit.subject,
        };
        match successors
            .iter_mut()
            .find(|s| s.successor == entry.successor)
        {
            Some(existing) => existing.commits.push(commit),
            None => successors.push(OutboxSuccessorView {
                successor: entry.successor,
                commits: vec![commit],
            }),
        }
    }
    successors
}

pub(crate) fn counts_view(counts: Vec<ItemCounts>) -> Vec<ItemCommitCountsView> {
    counts
        .into_iter()
        .map(|c| ItemCommitCountsView {
            item_id: c.item_id,
            inbox_count: c.inbox as u32,
            outbox_count: c.outbox as u32,
            conflict_count: c.conflicts as u32,
        })
        .collect()
}

/// The tools' answers, as the wire's views in JSON.
pub(crate) mod json {
    use serde_json::{json, Value};

    use super::*;

    pub(crate) fn inbox(entries: Vec<InboxEntry>, unavailable_reason: &str) -> Value {
        json!({
            "commits": inbox_view(entries).into_iter().map(|c| json!({
                "sha": c.sha,
                "subject": c.subject,
                "source": { "kind": c.source_kind, "name": c.source },
                "conflict": !c.conflict_files.is_empty(),
                "conflict_files": c.conflict_files,
            })).collect::<Vec<_>>(),
            "unavailable_reason": unavailable_reason,
        })
    }

    pub(crate) fn outbox(entries: Vec<OutboxEntry>, unavailable_reason: &str) -> Value {
        json!({
            "successors": outbox_view(entries).into_iter().map(|s| json!({
                "successor": s.successor,
                "commits": s.commits.into_iter().map(|c| json!({
                    "sha": c.sha, "subject": c.subject,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "unavailable_reason": unavailable_reason,
        })
    }
}

impl WorkQueueRpcHandler {
    /// The owner's queue, its trunk and the site a commit-flow read runs over, forwarded when the
    /// address names another host (`None`).
    async fn commit_flow_site(
        &self,
        call: &super::links::LinkCall,
        session_token: &str,
    ) -> Result<(WorkQueue, FlowSite), Status> {
        let (sessions_base, queue, trunk) = self.read_queue_and_trunk(call)?;
        let site = flow_site(FlowInputs {
            pull_requests: self.pull_requests.as_ref(),
            os_user: &call.os_user,
            sessions_base: &sessions_base,
            queue_id: &call.address.queue_id,
            caller_session_id: &call.address.queue_id,
            project_id: &call.address.project_id,
            session_token,
            queue: &queue,
            trunk: &trunk,
        })
        .await
        .map_err(Status::failed_precondition)?;
        Ok((queue, site))
    }

    pub(super) async fn get_work_item_inbox_rpc(
        &self,
        request: Request<GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<WorkItemInboxResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, WorkItemInboxResponse>(
                "GetWorkItemInbox",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let item_id = req.item_id.trim().to_string();
        let (queue, site) = self.commit_flow_site(&call, &req.session_token).await?;
        check_item(&queue, &site, &item_id).map_err(Status::not_found)?;
        let entries = read_flow(&site, move |flow, items| flow.inbox(items, &item_id))
            .await
            .map_err(Status::failed_precondition)?;
        Ok(Response::new(WorkItemInboxResponse {
            commits: inbox_view(entries),
            unavailable_reason: site.unavailable_reason,
        }))
    }

    pub(super) async fn get_work_item_outbox_rpc(
        &self,
        request: Request<GetWorkItemCommitFlowRequest>,
    ) -> Result<Response<WorkItemOutboxResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, WorkItemOutboxResponse>(
                "GetWorkItemOutbox",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let item_id = req.item_id.trim().to_string();
        let (queue, site) = self.commit_flow_site(&call, &req.session_token).await?;
        check_item(&queue, &site, &item_id).map_err(Status::not_found)?;
        let entries = read_flow(&site, move |flow, items| flow.outbox(items, &item_id))
            .await
            .map_err(Status::failed_precondition)?;
        Ok(Response::new(WorkItemOutboxResponse {
            successors: outbox_view(entries),
            unavailable_reason: site.unavailable_reason,
        }))
    }

    pub(super) async fn get_work_queue_commit_counts_rpc(
        &self,
        request: Request<GetWorkQueueGraphRequest>,
    ) -> Result<Response<WorkQueueCommitCountsResponse>, Status> {
        let req = request.into_inner();
        let call = self.link_call(&req.session_token, req.queue.as_ref())?;
        if let Some(answered) = self
            .forward_addressed::<_, WorkQueueCommitCountsResponse>(
                "GetWorkQueueCommitCounts",
                &call.address.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }
        let (_, site) = self.commit_flow_site(&call, &req.session_token).await?;
        let counts = read_flow(&site, |flow, items| flow.counts(items))
            .await
            .map_err(Status::failed_precondition)?;
        Ok(Response::new(WorkQueueCommitCountsResponse {
            items: counts_view(counts),
            unavailable_reason: site.unavailable_reason,
        }))
    }
}
