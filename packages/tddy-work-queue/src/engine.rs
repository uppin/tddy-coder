//! Land and propagate (`#workqueue` 14/14), behind a git port and a GitHub port.
//!
//! - **Propagate** an item: fetch; read its inbox live from git; refuse unless its local branch is
//!   clean, pushed and level with its remote (the refusal names the state and the actions that fix it,
//!   and `force` never overrides it); pre-flight the propagation conditions (`force` skips only the
//!   predicted-conflict one); rebase onto the line parent's tip in its own worktree; push with lease;
//!   retarget its PR when the line parent changed.
//! - **Land** a range of the derived line: for each item, refuse unless its merge conditions are met;
//!   merge its PR; propagate each successor onto the new base before taking the next item; stop at the
//!   first item that cannot land, naming why.
//!
//! The immediate `work_item_merge` / `work_item_repoint` tools call these (land one item / propagate
//! one item); the moved recipe functions they replace are deleted by `/green`.

use async_trait::async_trait;

/// A local branch's relation to its remote, for the propagation gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalBranchState {
    InSync,
    Dirty,
    Unpushed,
    Unpulled,
    Diverged,
    Unpublished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError(pub String);

/// The git operations the engines need, in the item's own worktree.
#[async_trait]
pub trait QueueGit: Send + Sync {
    async fn fetch(&self) -> Result<(), EngineError>;
    async fn local_branch_state(&self, branch: &str) -> Result<LocalBranchState, EngineError>;
    /// Commits on `origin/<onto>` that `origin/<branch>` lacks.
    async fn inbox_count(&self, branch: &str, onto: &str) -> Result<u32, EngineError>;
    /// Files a rebase of `branch` onto `onto` would conflict in (empty: clean).
    async fn predicted_conflicts(
        &self,
        branch: &str,
        onto: &str,
    ) -> Result<Vec<String>, EngineError>;
    async fn tip(&self, branch: &str) -> Result<String, EngineError>;
    async fn rebase_onto(&self, branch: &str, onto: &str) -> Result<(), EngineError>;
    async fn push_with_lease(&self, branch: &str, expected_sha: &str) -> Result<(), EngineError>;
}

/// The GitHub operations the engines need.
#[async_trait]
pub trait QueueGithub: Send + Sync {
    /// Merge PR `number`; answers the merge commit's SHA.
    async fn merge(&self, number: u64) -> Result<String, EngineError>;
    async fn retarget(&self, number: u64, base: &str) -> Result<(), EngineError>;
}

/// Why an engine did not act.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineRefusal {
    /// The item's local branch is not clean, pushed and level; `actions` fix it, in order
    /// (`commit`, `pull`, `push`).
    LocalBranch {
        state: LocalBranchState,
        actions: Vec<&'static str>,
    },
    /// A pre-flight condition does not hold.
    Preflight {
        condition: &'static str,
        detail: String,
    },
    /// Git or GitHub failed.
    Failed(String),
}

/// One item to propagate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropagateRequest {
    pub item_id: String,
    pub branch: String,
    /// The line parent's branch (or the trunk) to rebase onto.
    pub onto: String,
    pub pr_number: Option<u64>,
    /// `Some(base)` when the PR's base must change to the new line parent.
    pub retarget_to: Option<String>,
    /// Whether the base's CI is green (the propagation `checks` condition).
    pub base_checks_green: bool,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Propagated {
    pub item_id: String,
    pub onto: String,
    pub commits_taken: u32,
}

/// Propagate one item.
pub async fn propagate(
    git: &dyn QueueGit,
    gh: &dyn QueueGithub,
    request: &PropagateRequest,
) -> Result<Propagated, EngineRefusal> {
    // TODO(n14 land-propagate): implement
    let _ = (git, gh, request);
    todo!("TODO(n14): implement propagate")
}

/// One item of the line to land, with its merge conditions already evaluated (n13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandItem {
    pub item_id: String,
    pub branch: String,
    pub pr_number: u64,
    /// `None` when every merge condition is met; else the condition holding it up and its detail.
    pub held_up_on: Option<(String, String)>,
    /// What sits directly on this item, to propagate after it merges (onto the trunk).
    pub successors: Vec<PropagateRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LandReport {
    /// `(item_id, merge sha)` in landing order.
    pub landed: Vec<(String, String)>,
    pub propagated: Vec<Propagated>,
    /// The first item that could not land and why.
    pub stopped_at: Option<(String, String)>,
}

/// Land `line` (derived-line order, the requested waves only).
pub async fn land(git: &dyn QueueGit, gh: &dyn QueueGithub, line: &[LandItem]) -> LandReport {
    // TODO(n14 land-propagate): implement
    let _ = (git, gh, line);
    todo!("TODO(n14): implement land")
}
