//! A queue's GitHub reads through the daemon's inbox (`#workqueue` 6/14, moved here from n2 by the
//! operator): a branch's PR status (`GetPrStatus`, `QueryBranch`'s PR leg), the queue's status views
//! (`work_queue_status`), and each item's check summary. Every read is an inbox row,
//! so a second reader of the same PR costs no GitHub request.

use std::collections::HashMap;
use std::path::Path;

use tddy_core::changeset::{resolve_stack_node_branch, WorkQueue};
use tddy_github::pr_api::{CheckStatus, PrState};
use tddy_github_inbox::{GithubInbox, InboxError, RepoSlug};
use tddy_service::proto::work_queue::{PrStatusView, WorkItemStatus, WorkQueueStatusView};

use crate::assess::{self, NodeView};

/// `branch`'s PR as the wire's `PrStatusView`; `unavailable` (never "no PR") when the inbox could not
/// read it.
pub async fn pr_status_view_through_inbox(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    branch: &str,
) -> PrStatusView {
    match inbox.pr_for_head(repo, branch).await {
        Ok(cached) => match cached.value {
            Some(pr) => PrStatusView {
                exists: true,
                number: pr.number,
                url: pr.url,
                state: pr_state_label(pr.state).to_string(),
                unavailable: false,
                unavailable_reason: String::new(),
            },
            None => PrStatusView::default(),
        },
        Err(error) => PrStatusView {
            unavailable: true,
            unavailable_reason: inbox_error_reason(&error),
            ..Default::default()
        },
    }
}

/// `assess::assemble_views`, reading each item's PR through the inbox instead of a synchronous
/// `GithubPrApi` lookup.
pub async fn assemble_views_through_inbox(
    parent_dir: &Path,
    sessions_root: &Path,
    queue: &WorkQueue,
    inbox: &GithubInbox,
    repo: &RepoSlug,
) -> Result<Vec<NodeView>, tddy_core::WorkflowError> {
    // The only await points: one inbox read per branch an item stands on, before the (synchronous,
    // filesystem-only) view assembly consumes them.
    let mut open_prs: HashMap<String, (u64, String)> = HashMap::new();
    for item in &queue.nodes {
        let Some(branch) = resolve_stack_node_branch(sessions_root, item).filter(|b| !b.is_empty())
        else {
            continue;
        };
        if open_prs.contains_key(&branch) {
            continue;
        }
        let cached = inbox.pr_for_head(repo, &branch).await.map_err(|error| {
            tddy_core::WorkflowError::Backend(tddy_core::error::BackendError::InvocationFailed(
                format!(
                    "reading the PR of branch {branch} through the inbox: {}",
                    inbox_error_reason(&error)
                ),
            ))
        })?;
        if let Some(pr) = cached
            .value
            .filter(|pr| matches!(pr.state, PrState::Open | PrState::Draft))
        {
            open_prs.insert(branch, (pr.number, pr.base_branch));
        }
    }
    assess::assemble_views_with(parent_dir, sessions_root, queue, |branch| {
        Ok(open_prs.get(branch).cloned())
    })
}

/// One PR'ed item's checks at its head, counted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemCheckSummary {
    pub item_id: String,
    pub running: u32,
    pub failed: u32,
    pub succeeded: u32,
}

/// The check summary of every item of `queue` that has a PR.
///
/// An item has a PR when it records an assigned one or its branch has one. An item whose PR or checks
/// the inbox cannot read is left out rather than summarised as zeros: no summary is not "all green".
pub async fn item_check_summaries(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    queue: &WorkQueue,
) -> Vec<ItemCheckSummary> {
    let mut summaries = Vec::new();
    for item in &queue.nodes {
        let head = match &item.pr {
            Some(assigned) => inbox.pr(repo, assigned.number).await.map(|c| Some(c.value)),
            None => match item.branch.as_deref() {
                Some(branch) => inbox.pr_for_head(repo, branch).await.map(|c| c.value),
                None => continue,
            },
        };
        let Ok(Some(pr)) = head else { continue };
        let Ok(runs) = inbox.checks_at(repo, &pr.head_sha).await else {
            continue;
        };
        let mut summary = ItemCheckSummary {
            item_id: item.node_id.clone(),
            ..Default::default()
        };
        for run in &runs.value {
            match run.status {
                CheckStatus::Queued | CheckStatus::InProgress => summary.running += 1,
                CheckStatus::Completed if run.conclusion == "success" => summary.succeeded += 1,
                CheckStatus::Completed => summary.failed += 1,
            }
        }
        summaries.push(summary);
    }
    summaries
}

/// One snapshot of `StreamWorkQueueStatus`: every item's PR state (its assigned PR's head branch, else
/// its own branch) and, for a PR'ed item, its check counts.
pub async fn work_queue_status_view(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    queue: &WorkQueue,
) -> WorkQueueStatusView {
    let summaries = item_check_summaries(inbox, repo, queue).await;
    let mut items = Vec::with_capacity(queue.nodes.len());
    for item in &queue.nodes {
        let branch = item
            .pr
            .as_ref()
            .map(|pr| pr.head_branch.as_str())
            .or(item.branch.as_deref());
        let pr = match branch {
            Some(branch) => pr_status_view_through_inbox(inbox, repo, branch).await,
            None => PrStatusView::default(),
        };
        let checks = summaries.iter().find(|s| s.item_id == item.node_id);
        items.push(WorkItemStatus {
            item_id: item.node_id.clone(),
            pr: Some(pr),
            has_checks: checks.is_some(),
            checks_running: checks.map_or(0, |c| c.running),
            checks_failed: checks.map_or(0, |c| c.failed),
            checks_succeeded: checks.map_or(0, |c| c.succeeded),
        });
    }
    WorkQueueStatusView {
        items,
        unavailable_reason: String::new(),
    }
}

fn pr_state_label(state: PrState) -> &'static str {
    match state {
        PrState::Open => "open",
        PrState::Merged => "merged",
        PrState::Closed => "closed",
        PrState::Draft => "draft",
    }
}

fn inbox_error_reason(error: &InboxError) -> String {
    match error {
        InboxError::Unavailable(reason) => reason.clone(),
        InboxError::RateLimited { reset_at_unix_s } => {
            format!("GitHub's rate limit is spent until {reset_at_unix_s} (unix seconds)")
        }
        InboxError::NotFound => "GitHub answered 404".to_string(),
    }
}
