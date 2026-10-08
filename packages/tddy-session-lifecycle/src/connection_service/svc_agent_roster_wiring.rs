use crate::connection_service::DaemonSessionHost;
use tddy_rpc::Request;
use tddy_rpc::Status;
use tddy_service::proto::session::GetWorktreeSnapshotRequest;

/// The daemon measuring a checkout that lives on one of its peers.
///
/// Routed through its own `GetWorktreeSnapshot` handler rather than a bespoke client, so a remote
/// measurement takes exactly the path a caller's would — including the peer routing and the
/// blocking-pool budget.
#[async_trait::async_trait]
impl tddy_daemon_livekit::session_room::RemoteSnapshotSource for DaemonSessionHost {
    async fn snapshot(
        &self,
        session_token: &str,
        codebase_session_id: &str,
        codebase_instance_id: &str,
    ) -> Result<tddy_daemon_livekit::session_room::WorktreeSnapshot, Status> {
        let answered = self
            .launch_sessions()
            .get_worktree_snapshot_at_session_coordinate(Request::direct(
                GetWorktreeSnapshotRequest {
                    session_token: session_token.to_string(),
                    session_id: codebase_session_id.to_string(),
                    daemon_instance_id: codebase_instance_id.to_string(),
                },
            ))
            .await?
            .into_inner();
        Ok(tddy_daemon_livekit::session_room::WorktreeSnapshot {
            head_commit: answered.head_commit,
            branch: answered.branch,
            changed_paths: answered.changed_paths,
            changed_files: answered.changed_files,
            lines_added: answered.lines_added,
            lines_removed: answered.lines_removed,
            untracked_files: answered.untracked_files,
            // FIXME(session-worktree-sync): a SPLIT session's snapshot arrives over
            // GetWorktreeSnapshot, whose response carries no tree — so the facilitating daemon
            // cannot diff a checkout it does not hold. Closing this means a `wip_tree` field on
            // GetWorktreeSnapshotResponse and the codebase daemon writing it. Until then a split
            // session syncs committed history only, and says so rather than mirroring silently
            // stale content. See docs/dev/TODO.md.
            wip_tree: String::new(),
        })
    }
}
