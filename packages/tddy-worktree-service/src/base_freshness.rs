//! Whether an orchestrator's home worktree is at the tip of its project's trunk, and bringing it
//! there (`#orchestra` 4/9). The orchestrator runs both itself, by tool, before it plans or starts
//! work: nothing here runs on a timer.
//!
//! Freshness fetches the trunk's branch from its remote first, so `behind` counts what has landed
//! since — not what had landed at the last fetch. A refresh is only ever a fast-forward; a home with
//! commits of its own is refused, never reset.

use std::path::Path;

/// Where a home worktree stands against its trunk's remote tip, just fetched.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BaseFreshness {
    /// The trunk ref compared against (`origin/master`).
    pub trunk_ref: String,
    /// The home worktree's `HEAD`.
    pub home_commit: String,
    /// The trunk ref's commit after the fetch.
    pub remote_tip: String,
    /// Commits the home has that the tip does not.
    pub ahead: u32,
    /// Commits the tip has that the home does not.
    pub behind: u32,
    pub fetched_at_unix_ms: u64,
}

/// Why a home worktree was not brought to its trunk's tip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshRefusal {
    /// The home has commits the tip does not: a fast-forward cannot reach the tip.
    Diverged(BaseFreshness),
    /// A git command failed; the message is git's.
    Git(String),
}

impl std::fmt::Display for RefreshRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diverged(at) => write!(
                f,
                "the home worktree has {} commit(s) {} does not; it is only ever fast-forwarded, so it was left at {}",
                at.ahead, at.trunk_ref, at.home_commit
            ),
            Self::Git(message) => write!(f, "git failed: {message}"),
        }
    }
}

/// Fetch `trunk_ref`'s branch and report where `home` stands against it.
pub fn base_freshness(home: &Path, trunk_ref: &str) -> Result<BaseFreshness, String> {
    // TODO(base-freshness): implement
    let _ = (home, trunk_ref);
    unimplemented!("base_freshness")
}

/// Fetch `trunk_ref`'s branch and fast-forward `home` to it, reporting where it then stands.
pub fn refresh_home(home: &Path, trunk_ref: &str) -> Result<BaseFreshness, RefreshRefusal> {
    // TODO(base-freshness): implement
    let _ = (home, trunk_ref);
    unimplemented!("refresh_home")
}
