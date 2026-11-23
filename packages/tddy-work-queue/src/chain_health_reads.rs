//! Reading each item's chain health (`#workqueue` 12/14): the inputs of [`crate::chain_health`] —
//! PR commits and `mergeable` through the GitHub inbox, the behind count from remote tips in the queue
//! owner's checkout (`base_sync::count_divergence`). Nothing local is touched; the clone is fetched.

use std::collections::HashMap;
use std::path::PathBuf;

use tddy_core::changeset::WorkQueue;
use tddy_github::pr_api::{PrDetail, PrState};
use tddy_github_inbox::{GithubInbox, PrCommit, RepoSlug};

use crate::chain_health::{leaks, line_state, Leak, LineState};
use crate::commit_flow::{item_branches, shared_for, ItemBranch};
use crate::lineage::LineEndpoint;

/// One item's verdicts. `line` is `None` for an item that owns no open PR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemChainHealth {
    pub item_id: String,
    pub leaks: Vec<Leak>,
    pub line: Option<LineState>,
}

fn is_live(pr: &PrDetail) -> bool {
    matches!(pr.state, PrState::Open | PrState::Draft)
}

/// The open PR of `branch` and its commits, when the inbox can read both.
async fn live_pr_commits(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    branch: &str,
) -> Option<(PrDetail, Vec<PrCommit>)> {
    let pr = inbox.pr_for_head(repo, branch).await.ok()?.value?;
    if !is_live(&pr) {
        return None;
    }
    let commits = inbox
        .pr_commits(repo, pr.number, &pr.head_sha)
        .await
        .ok()?
        .value;
    Some((pr, commits))
}

/// The behind count of `item` against its line `parent`, from remote tips; `None` when the parent is
/// merged, planned (no live PR / no branch) or a tip cannot be read.
async fn behind_of(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    repo_root: Option<&PathBuf>,
    item_branch: &str,
    parent: &LineEndpoint,
    parent_branch: Option<&str>,
) -> Option<u32> {
    let repo_root = repo_root?.clone();
    let parent_branch = parent_branch?.to_string();
    // A parent item is read only while its own PR is live: once it merged (or before it has one) the
    // item's line edge points at a branch that no longer is, or not yet is, the thing to be behind.
    if matches!(parent, LineEndpoint::Item(_)) {
        let pr = inbox.pr_for_head(repo, &parent_branch).await.ok()?.value?;
        if !is_live(&pr) {
            return None;
        }
    }
    let item_branch = item_branch.to_string();
    tokio::task::spawn_blocking(move || {
        let tip = |branch: &str| -> Option<String> {
            let out = std::process::Command::new("git")
                .current_dir(&repo_root)
                .args(["rev-parse", "--verify", "--quiet"])
                .arg(format!("origin/{branch}^{{commit}}"))
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        };
        let (base_sha, head_sha) = (tip(&parent_branch)?, tip(&item_branch)?);
        tddy_session_worktree::base_sync::count_divergence(&repo_root, &base_sha, &head_sha)
            .ok()
            .map(|(behind, _ahead)| behind)
    })
    .await
    .ok()?
}

/// `item`'s line ancestors, nearest first, each with its live PR's commits (an ancestor with no live
/// PR carries nothing to leak from).
async fn live_ancestors(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    item: &ItemBranch,
    by_id: &HashMap<&str, &ItemBranch>,
) -> Vec<(String, Vec<PrCommit>)> {
    let mut ancestors = Vec::new();
    let mut at = item;
    let mut hops = 0;
    while let LineEndpoint::Item(parent_id) = &at.line_parent {
        hops += 1;
        let Some(parent) = by_id.get(parent_id.as_str()).copied() else {
            break;
        };
        if hops > by_id.len() {
            break; // a line cycle: stop rather than loop
        }
        if let Some((_, commits)) = live_pr_commits(inbox, repo, &parent.branch).await {
            ancestors.push((parent.branch.clone(), commits));
        }
        at = parent;
    }
    ancestors
}

/// Every branch-owning item's chain health. `line` is the queue's line edges (inbox, else planned).
pub async fn queue_chain_health(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    queue: &WorkQueue,
    line: &[(String, LineEndpoint)],
    repo_root: Option<PathBuf>,
) -> Vec<ItemChainHealth> {
    if let Some(root) = &repo_root {
        // Remote tips only; a failed fetch leaves the last known tips, which is a reading, not a guess.
        let flow = shared_for(root);
        let _ = tokio::task::spawn_blocking(move || flow.fetch()).await;
    }
    let items = item_branches(queue, line);
    let by_id: HashMap<&str, &ItemBranch> = items.iter().map(|i| (i.item_id.as_str(), i)).collect();
    let mut out = Vec::with_capacity(items.len());
    for item in &items {
        let Some((pr, own)) = live_pr_commits(inbox, repo, &item.branch).await else {
            out.push(ItemChainHealth {
                item_id: item.item_id.clone(),
                leaks: Vec::new(),
                line: None,
            });
            continue;
        };
        let ancestors = live_ancestors(inbox, repo, item, &by_id).await;
        let parent_branch = match &item.line_parent {
            LineEndpoint::Item(id) => by_id.get(id.as_str()).map(|p| p.branch.as_str()),
            LineEndpoint::Trunk(b) | LineEndpoint::External(b) => Some(b.as_str()),
        };
        let behind = behind_of(
            inbox,
            repo,
            repo_root.as_ref(),
            &item.branch,
            &item.line_parent,
            parent_branch,
        )
        .await;
        out.push(ItemChainHealth {
            item_id: item.item_id.clone(),
            leaks: leaks(&own, &ancestors),
            line: Some(line_state(behind, pr.mergeable)),
        });
    }
    out
}
