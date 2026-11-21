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
    crate::home_worktree::fetch_ref(home, trunk_ref).map_err(|e| e.to_string())?;
    standing(home, trunk_ref)
}

/// Fetch `trunk_ref`'s branch and fast-forward `home` to it, reporting where it then stands.
pub fn refresh_home(home: &Path, trunk_ref: &str) -> Result<BaseFreshness, RefreshRefusal> {
    crate::home_worktree::fetch_ref(home, trunk_ref)
        .map_err(|e| RefreshRefusal::Git(e.to_string()))?;
    let at = standing(home, trunk_ref).map_err(RefreshRefusal::Git)?;
    if at.ahead > 0 {
        return Err(RefreshRefusal::Diverged(at));
    }
    if at.behind > 0 {
        crate::home_worktree::git(home, &["merge", "--ff-only", trunk_ref])
            .map_err(|e| RefreshRefusal::Git(e.to_string()))?;
    }
    standing(home, trunk_ref).map_err(RefreshRefusal::Git)
}

/// Where `home` stands against `trunk_ref` as it resolves now (no fetch).
fn standing(home: &Path, trunk_ref: &str) -> Result<BaseFreshness, String> {
    use crate::home_worktree::git;
    let rev = |spec: &str| {
        git(
            home,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{spec}^{{commit}}"),
            ],
        )
        .map_err(|e| format!("{spec} does not resolve: {e}"))
    };
    let home_commit = rev("HEAD")?;
    let remote_tip = rev(trunk_ref)?;
    let counts = git(
        home,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...{trunk_ref}"),
        ],
    )
    .map_err(|e| e.to_string())?;
    let mut parts = counts.split_whitespace().map(|n| n.parse::<u32>());
    let (Some(Ok(ahead)), Some(Ok(behind))) = (parts.next(), parts.next()) else {
        return Err(format!("unreadable ahead/behind counts: {counts}"));
    };
    Ok(BaseFreshness {
        trunk_ref: trunk_ref.to_string(),
        home_commit,
        remote_tip,
        ahead,
        behind,
        fetched_at_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    })
}

/// The commit a session's worktree was cut from, and how far it was behind its remote tip.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CutBase {
    pub commit: String,
    pub behind: u32,
}

/// The git invocations that read a worktree's [`CutBase`]: its `HEAD`, then the commits its
/// upstream has that it does not. Shared by the local cut (run here) and the remote cut (run
/// through the target's shell), so both report the same thing.
pub const CUT_BASE_COMMIT_ARGS: [&str; 2] = ["rev-parse", "HEAD"];
pub const CUT_BASE_BEHIND_ARGS: [&str; 3] = ["rev-list", "--count", "HEAD..@{upstream}"];

impl CutBase {
    /// Read `worktree`'s cut base. A branch with no upstream has no remote tip to be behind, so
    /// it is zero behind; a worktree whose `HEAD` cannot be read is an error.
    pub fn of_worktree(worktree: &Path) -> Result<Self, String> {
        use crate::home_worktree::git;
        let commit = git(worktree, &CUT_BASE_COMMIT_ARGS).map_err(|e| e.to_string())?;
        let behind = git(worktree, &CUT_BASE_BEHIND_ARGS)
            .ok()
            .and_then(|count| count.parse().ok())
            .unwrap_or(0);
        Ok(Self { commit, behind })
    }
}
