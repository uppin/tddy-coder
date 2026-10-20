//! Split placement — the agent on this daemon, its checkout on another — as ports.
//!
//! The facilitating daemon hosts the session's room over a checkout it does not hold, measuring it
//! through [`RemoteSnapshotSource`] with a per-poll credential from [`SessionTokenMinter`], and
//! spawns the split agent with a room join from [`SplitRoomHosting::agent_join`]. Split placement
//! *requires* a room integration: [`NoSplitRooms`] refuses it, as a daemon without one always has.
//!
//! [`WorktreeSnapshot`], [`SessionTokenMinter`] and [`RemoteSnapshotSource`] moved here verbatim from
//! the LiveKit plugin, which re-exports them.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tddy_rpc::Status;

use crate::ReachableSessions;

/// Everything one poll of a checkout observed.
///
/// `changed_paths` is carried here and published in room metadata but never in an event: an event
/// says *that* the checkout moved, and reading it is what the file-access RPCs in the same room are
/// for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorktreeSnapshot {
    pub head_commit: String,
    pub branch: String,
    pub changed_paths: Vec<String>,
    pub changed_files: u32,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub untracked_files: u32,
    /// The tree object of the whole working tree as it stands, staged into a **temporary** index.
    ///
    /// This is what makes a delta possible: diffing two of these yields an ordinary git patch that
    /// carries deletions, renames, modes and binary content, and — unlike `git diff HEAD` — it sees
    /// a newly written *untracked* file, which is exactly what a `Write` produces.
    ///
    /// The index is temporary and that is not an optimisation: `git add -A` against the agent's own
    /// index would rewrite its staging area mid-session.
    ///
    /// Empty when the tree could not be written; the tick then produces no delta rather than a
    /// wrong one.
    pub wip_tree: String,
}

impl WorktreeSnapshot {
    /// True when the two snapshots disagree about the **tracked** diff — the paths and their line
    /// counts, everything `git diff --numstat HEAD` measures.
    ///
    /// HEAD is deliberately excluded: it is the other half of the comparison and gets its own event.
    ///
    /// So is `untracked_files`, for two reasons. An event carries counts only, and a HEAD-relative
    /// diff has no line counts for a path git does not track — so a newly written untracked file
    /// could only be announced as `files=0 +0 -0`, which tells a receiver nothing it can act on.
    /// And every write is untracked for the moment between the file appearing and `git add`
    /// staging it, so treating that as activity would put an empty event in front of the commit
    /// event that actually describes the work. The fact is not lost: it lives in the room's
    /// metadata, which [`WorktreeSnapshot`] equality still covers, so a joiner reads the untracked
    /// count from state rather than from a notification with nothing in it.
    pub fn tracked_diff_differs(&self, other: &Self) -> bool {
        self.changed_paths != other.changed_paths
            || self.changed_files != other.changed_files
            || self.lines_added != other.lines_added
            || self.lines_removed != other.lines_removed
    }
}

/// The credential one poll authenticates with.
///
/// A minter rather than a token, because a token is a thing that expires: a session token lives
/// [`tddy_github::SESSION_TOKEN_TTL`] — five minutes — while a room outlives the agent it belongs
/// to, so anything frozen at session start stops being accepted by the codebase daemon long before
/// the room stops asking, and every poll after that reads as an unreachable peer. This daemon holds
/// its own signing key, so it can issue one per poll and needs no long-lived bearer token in memory
/// at all.
///
/// Infallible: a minter can only be built where a signing key and a verified identity are already
/// proven to exist, so "nothing to sign with" is a precondition of constructing one and never a
/// condition a poll has to handle.
pub trait SessionTokenMinter: Send + Sync {
    fn mint(&self) -> String;
}

/// One `GetWorktreeSnapshot` call, as the room's poll loop needs it.
///
/// A trait so the room does not depend on `DaemonSessionHost`: the daemon supplies the real
/// implementation, and the dependency runs one way.
#[async_trait::async_trait]
pub trait RemoteSnapshotSource: Send + Sync {
    async fn snapshot(
        &self,
        session_token: &str,
        codebase_session_id: &str,
        codebase_instance_id: &str,
    ) -> Result<WorktreeSnapshot, Status>;
}

/// The room join a split agent is spawned with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitAgentJoin {
    pub url: String,
    pub room: String,
    pub token: String,
}

/// Hosting a session's room over a checkout held by another daemon.
#[async_trait::async_trait]
pub trait SplitRoomHosting: Send + Sync {
    /// Open `session_id`'s room measuring `codebase_session_id` on `codebase_instance_id`.
    #[allow(clippy::too_many_arguments)]
    async fn host_remote_worktree(
        &self,
        session_id: &str,
        session_dir: &Path,
        codebase_session_id: &str,
        codebase_instance_id: &str,
        snapshots: Arc<dyn RemoteSnapshotSource>,
        minter: Arc<dyn SessionTokenMinter>,
        services: Arc<dyn ReachableSessions>,
    ) -> Result<(), Status>;

    /// The join a split agent is spawned with: `room`, as `identity`, for `ttl`.
    fn agent_join(
        &self,
        room: &str,
        identity: &str,
        ttl: Duration,
    ) -> Result<SplitAgentJoin, Status>;
}

/// No room integration: split placement is refused, as before.
///
/// Both halves refuse, not only the join: the agent's only route to a checkout on another daemon is
/// the room, so a room that cannot be hosted is as fatal to the placement as a join that cannot be
/// issued. This is the precondition a daemon without one has always failed the start with — not a
/// fallback to some other placement.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSplitRooms;

impl NoSplitRooms {
    fn refusal() -> Status {
        Status::failed_precondition(
            "cannot place a session's codebase on another daemon: this daemon has no room integration configured",
        )
    }
}

#[async_trait::async_trait]
impl SplitRoomHosting for NoSplitRooms {
    async fn host_remote_worktree(
        &self,
        _session_id: &str,
        _session_dir: &Path,
        _codebase_session_id: &str,
        _codebase_instance_id: &str,
        _snapshots: Arc<dyn RemoteSnapshotSource>,
        _minter: Arc<dyn SessionTokenMinter>,
        _services: Arc<dyn ReachableSessions>,
    ) -> Result<(), Status> {
        Err(Self::refusal())
    }

    fn agent_join(
        &self,
        _room: &str,
        _identity: &str,
        _ttl: Duration,
    ) -> Result<SplitAgentJoin, Status> {
        Err(Self::refusal())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nothing_wired_split_placement_is_refused_as_before() {
        let refused = NoSplitRooms
            .agent_join("session-abc", "agent-abc", Duration::from_secs(60))
            .expect_err("split placement needs a room integration");

        assert_eq!(refused.code, tddy_rpc::Code::FailedPrecondition);
        assert_eq!(
            refused.message,
            "cannot place a session's codebase on another daemon: this daemon has no room integration configured"
        );
    }
}
