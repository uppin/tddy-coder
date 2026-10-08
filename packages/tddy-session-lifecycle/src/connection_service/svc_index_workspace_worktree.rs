use crate::connection_service::launch_ports::LaunchSessions;
use std::path::Path;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Status;

impl LaunchSessions {
    /// Build the semantic index for a `workspace` session's worktree.
    ///
    /// The index indexes a worktree, and the worktree that counts is this daemon's: for the codebase
    /// half of a split session, the agent runs on another host with no repository on disk, so this is
    /// the only host that has anything to index (docs/ft/coder/semantic-index.md).
    ///
    /// Blocking, and a failure fails the start — no unindexed fallback, exactly as on the co-located
    /// paths: a session that came up without the index it asked for looks like the session that was
    /// asked for.
    ///
    /// TODO(seeded-agents-on-any-placement): export `TDDY_SEMANTIC_INDEX_DB` on this daemon's
    /// exec-tool surface, so a split session's `mcp__tddy-tools__SemanticSearch` reaches the index
    /// this built rather than reporting it unset. `tool_engine::execute_tool` takes no env pairs on
    /// the daemon's own surface, and the query side is unwired regardless
    /// (`packages/tddy-tool-engine/src/lib.rs` — "index query not yet wired").
    pub(crate) async fn index_workspace_worktree(
        &self,
        sessions_base: &Path,
        session_id: &str,
    ) -> Result<(), Status> {
        let worktree_path =
            tddy_session_agents::peer_session_answer::resolve_worktree_root_for_session(
                sessions_base,
                session_id,
            )?;
        let session_dir = unified_session_dir_path(sessions_base, session_id);
        tddy_session_split::service_util::index_session_worktree(
            &self.tddy_data_dir,
            &self.task_registry,
            session_id,
            &worktree_path,
            &session_dir,
        )
        .await?;
        log::info!(
            "StartSession: indexed workspace session {session_id}'s worktree at {}",
            worktree_path.display()
        );
        Ok(())
    }
}
