//! A work item's commit flow (`#workqueue` 11/14): its own commits, its **inbox** (what its line
//! ancestors have that it lacks, each attributed to the nearest ancestor carrying it, with a conflict
//! prediction made for *this* item) and its **outbox** (what it has that each successor lacks — always
//! its successors' inbox entries sourced from it).
//!
//! Read from remote tips (`origin/<branch>`) after a fetch: no local branch, index or working tree is
//! touched. Results are cached by the tips they were read at.
//!
//! `inbox(item)` = ⋃ over line ancestors `a` of `origin/<item>..origin/<a>`, trunk and external bases
//! listed first; `prediction` = `git merge-tree --write-tree` of the commit onto `origin/<item>` with the
//! commit's parent as merge base.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::lineage::LineEndpoint;

/// An item resolved for commit analysis: its branch and its line parent (n9's lineage).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemBranch {
    pub item_id: String,
    pub branch: String,
    pub line_parent: LineEndpoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowCommit {
    pub sha: String,
    pub subject: String,
}

/// Where an inbox commit comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitSource {
    /// The nearest line ancestor item whose branch carries it.
    Item(String),
    Trunk(String),
    External(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prediction {
    Clean,
    /// The files it would conflict in.
    Conflicts(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxEntry {
    pub commit: FlowCommit,
    pub source: CommitSource,
    pub prediction: Prediction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxEntry {
    pub successor: String,
    pub commit: FlowCommit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFlowError(pub String);

impl std::fmt::Display for CommitFlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CommitFlowError {}

/// Commit analysis over one clone (the queue owner's worktree), cached by remote tips.
pub struct CommitFlow {
    repo_dir: PathBuf,
    /// Per item: the tips its inbox was read at and the answer — replaced when the tips move, so
    /// the cache holds one entry per item for the daemon's lifetime.
    inbox_cache: Mutex<HashMap<String, (String, Vec<InboxEntry>)>>,
    hits: AtomicU64,
}

/// What an inbox read covers: the item, its line ancestors nearest first, and the base they end at.
struct Chain<'a> {
    item: &'a ItemBranch,
    ancestors: Vec<&'a ItemBranch>,
    base: &'a LineEndpoint,
}

impl CommitFlow {
    pub fn new(repo_dir: impl Into<PathBuf>) -> Self {
        Self {
            repo_dir: repo_dir.into(),
            inbox_cache: Mutex::new(HashMap::new()),
            hits: AtomicU64::new(0),
        }
    }

    /// `git fetch origin --prune` — remote-tracking refs only.
    pub fn fetch(&self) -> Result<(), CommitFlowError> {
        self.git(&["fetch", "origin", "--prune", "--quiet"])
            .map(|_| ())
    }

    /// `item_id`'s own commits beyond its line parent, oldest first.
    pub fn own_commits(
        &self,
        queue: &[ItemBranch],
        item_id: &str,
    ) -> Result<Vec<FlowCommit>, CommitFlowError> {
        let item = find_item(queue, item_id)?;
        let parent = parent_branch(queue, item)?;
        self.commits_in(&format!("origin/{}..origin/{}", parent, item.branch))
    }

    /// `item_id`'s inbox: trunk/external sources first, then items nearest first; oldest first within.
    pub fn inbox(
        &self,
        queue: &[ItemBranch],
        item_id: &str,
    ) -> Result<Vec<InboxEntry>, CommitFlowError> {
        let chain = chain_of(queue, item_id)?;
        let key = self.cache_key(&chain)?;
        if let Some((tips, cached)) = self.inbox_cache.lock().expect("cache lock").get(item_id) {
            if *tips == key {
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Ok(cached.clone());
            }
        }
        let entries = self.read_inbox(&chain)?;
        self.inbox_cache
            .lock()
            .expect("cache lock")
            .insert(item_id.to_string(), (key, entries.clone()));
        Ok(entries)
    }

    /// `item_id`'s outbox: per successor, the commits it lacks that `item_id` carries.
    pub fn outbox(
        &self,
        queue: &[ItemBranch],
        item_id: &str,
    ) -> Result<Vec<OutboxEntry>, CommitFlowError> {
        find_item(queue, item_id)?;
        let mut out = Vec::new();
        for successor in queue
            .iter()
            .filter(|i| i.line_parent == LineEndpoint::Item(item_id.to_string()))
        {
            for entry in self.inbox(queue, &successor.item_id)? {
                if entry.source == CommitSource::Item(item_id.to_string()) {
                    out.push(OutboxEntry {
                        successor: successor.item_id.clone(),
                        commit: entry.commit,
                    });
                }
            }
        }
        Ok(out)
    }

    /// How many reads were answered from the cache (unchanged tips).
    pub fn cache_hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// The remote tips every ancestry range of the chain is read at, as the cache key.
    fn cache_key(&self, chain: &Chain<'_>) -> Result<String, CommitFlowError> {
        let mut parts = vec![chain.item.item_id.clone()];
        let branches = std::iter::once(chain.item.branch.as_str())
            .chain(chain.ancestors.iter().map(|a| a.branch.as_str()))
            .chain(std::iter::once(chain.base.name()));
        for branch in branches {
            parts.push(format!("{branch}={}", self.tip(branch)?));
        }
        Ok(parts.join(" "))
    }

    fn read_inbox(&self, chain: &Chain<'_>) -> Result<Vec<InboxEntry>, CommitFlowError> {
        let item_ref = format!("origin/{}", chain.item.branch);
        let mut seen = std::collections::HashSet::new();
        let mut from_items = Vec::new();
        for ancestor in &chain.ancestors {
            let range = format!("{item_ref}..origin/{}", ancestor.branch);
            let source = CommitSource::Item(ancestor.item_id.clone());
            from_items.extend(self.entries_in(&range, &source, &item_ref, &mut seen)?);
        }
        let base_source = match chain.base {
            LineEndpoint::External(name) => CommitSource::External(name.clone()),
            other => CommitSource::Trunk(other.name().to_string()),
        };
        let range = format!("{item_ref}..origin/{}", chain.base.name());
        let mut entries = self.entries_in(&range, &base_source, &item_ref, &mut seen)?;
        entries.extend(from_items);
        Ok(entries)
    }

    fn entries_in(
        &self,
        range: &str,
        source: &CommitSource,
        item_ref: &str,
        seen: &mut std::collections::HashSet<String>,
    ) -> Result<Vec<InboxEntry>, CommitFlowError> {
        let mut entries = Vec::new();
        for commit in self.commits_in(range)? {
            if !seen.insert(commit.sha.clone()) {
                continue;
            }
            let prediction = self.predict(item_ref, &commit.sha)?;
            entries.push(InboxEntry {
                commit,
                source: source.clone(),
                prediction,
            });
        }
        Ok(entries)
    }

    /// Whether `commit` merges onto `item_ref` (with its own parent as the merge base) and where not.
    fn predict(&self, item_ref: &str, commit: &str) -> Result<Prediction, CommitFlowError> {
        let base = format!("--merge-base={commit}^");
        let out = self.git_output(&[
            "merge-tree",
            "--write-tree",
            "--name-only",
            &base,
            item_ref,
            commit,
        ])?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        match out.status.code() {
            Some(0) => Ok(Prediction::Clean),
            Some(1) => Ok(Prediction::Conflicts(
                stdout
                    .lines()
                    .skip(1)
                    .take_while(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect(),
            )),
            _ => Err(CommitFlowError(format!(
                "git merge-tree {commit} onto {item_ref}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))),
        }
    }

    fn commits_in(&self, range: &str) -> Result<Vec<FlowCommit>, CommitFlowError> {
        let out = self.git(&[
            "log",
            "--reverse",
            "--topo-order",
            "--format=%H%x1f%s",
            range,
        ])?;
        Ok(out
            .lines()
            .filter_map(|l| l.split_once('\u{1f}'))
            .map(|(sha, subject)| FlowCommit {
                sha: sha.to_string(),
                subject: subject.to_string(),
            })
            .collect())
    }

    fn tip(&self, branch: &str) -> Result<String, CommitFlowError> {
        self.git(&["rev-parse", "--verify", &format!("origin/{branch}")])
            .map(|s| s.trim().to_string())
    }

    fn git(&self, args: &[&str]) -> Result<String, CommitFlowError> {
        let out = self.git_output(args)?;
        if !out.status.success() {
            return Err(CommitFlowError(format!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn git_output(&self, args: &[&str]) -> Result<std::process::Output, CommitFlowError> {
        Command::new("git")
            .args(args)
            .current_dir(&self.repo_dir)
            .output()
            .map_err(|e| CommitFlowError(format!("git {}: {e}", args.join(" "))))
    }
}

fn find_item<'a>(
    queue: &'a [ItemBranch],
    item_id: &str,
) -> Result<&'a ItemBranch, CommitFlowError> {
    queue
        .iter()
        .find(|i| i.item_id == item_id)
        .ok_or_else(|| CommitFlowError(format!("item '{item_id}' not found")))
}

/// The branch an item's own commits are counted from: its line parent's branch.
fn parent_branch<'a>(
    queue: &'a [ItemBranch],
    item: &'a ItemBranch,
) -> Result<&'a str, CommitFlowError> {
    match &item.line_parent {
        LineEndpoint::Item(id) => find_item(queue, id).map(|p| p.branch.as_str()),
        other => Ok(other.name()),
    }
}

fn chain_of<'a>(queue: &'a [ItemBranch], item_id: &str) -> Result<Chain<'a>, CommitFlowError> {
    let item = find_item(queue, item_id)?;
    let mut ancestors = Vec::new();
    let mut at = item;
    loop {
        match &at.line_parent {
            LineEndpoint::Item(id) => {
                let parent = find_item(queue, id)?;
                if ancestors.len() > queue.len() {
                    return Err(CommitFlowError(format!(
                        "item '{item_id}' is on a line cycle"
                    )));
                }
                ancestors.push(parent);
                at = parent;
            }
            base => {
                return Ok(Chain {
                    item,
                    ancestors,
                    base,
                })
            }
        }
    }
}

/// The queue's items that own a branch, each with its line parent. An item whose line edge is unknown
/// (cyclic or dangling planned parents) is left out.
pub fn item_branches(
    queue: &tddy_core::changeset::WorkQueue,
    line: &[(String, LineEndpoint)],
) -> Vec<ItemBranch> {
    queue
        .nodes
        .iter()
        .filter_map(|item| {
            let branch = item.branch.clone()?;
            let (_, line_parent) = line.iter().find(|(id, _)| *id == item.node_id)?;
            Some(ItemBranch {
                item_id: item.node_id.clone(),
                branch,
                line_parent: line_parent.clone(),
            })
        })
        .collect()
}

/// One item's counts for its queue row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemCounts {
    pub item_id: String,
    pub inbox: usize,
    pub outbox: usize,
    /// Inbox commits predicted to conflict.
    pub conflicts: usize,
}

impl CommitFlow {
    /// Every item's counts, from the same cached reads the item views use.
    pub fn counts(&self, queue: &[ItemBranch]) -> Result<Vec<ItemCounts>, CommitFlowError> {
        queue
            .iter()
            .map(|item| {
                let inbox = self.inbox(queue, &item.item_id)?;
                Ok(ItemCounts {
                    item_id: item.item_id.clone(),
                    outbox: self.outbox(queue, &item.item_id)?.len(),
                    conflicts: inbox
                        .iter()
                        .filter(|e| matches!(e.prediction, Prediction::Conflicts(_)))
                        .count(),
                    inbox: inbox.len(),
                })
            })
            .collect()
    }
}

/// The one [`CommitFlow`] of a clone, so its cache outlives a request.
pub fn shared_for(repo_dir: &std::path::Path) -> std::sync::Arc<CommitFlow> {
    use std::sync::{Arc, OnceLock};
    static FLOWS: OnceLock<Mutex<HashMap<PathBuf, Arc<CommitFlow>>>> = OnceLock::new();
    let mut flows = FLOWS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("flows lock");
    Arc::clone(
        flows
            .entry(repo_dir.to_path_buf())
            .or_insert_with(|| Arc::new(CommitFlow::new(repo_dir))),
    )
}
