use tddy_task::TerminalCapture;

use crate::cli_session_manager::MAIN_TERMINAL_ID;

use std::path::PathBuf;

use std::sync::Arc;

use std::path::Path;

use tddy_core::BranchWorktreeIntent;

use tddy_rpc::Status;

use std::time::Duration;

/// Runs blocking clone/spawn work with a wall-clock cap so hung NSS/git/spawn cannot block RPCs forever.
pub(crate) async fn spawn_blocking_with_timeout<T: Send + 'static>(
    timeout: Duration,
    op_label: &'static str,
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> Result<T, Status> {
    match tokio::time::timeout(timeout, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(Ok(v))) => Ok(v),
        Ok(Ok(Err(e))) => {
            log::error!("{} failed: {}", op_label, e);
            Err(Status::internal(e.to_string()))
        }
        Ok(Err(join_err)) => Err(Status::internal(join_err.to_string())),
        Err(_elapsed) => {
            log::error!(
                "{} timed out after {}s (spawn_worker_request_timeout_secs); blocking task may still run in the pool",
                op_label,
                timeout.as_secs()
            );
            Err(Status::deadline_exceeded(format!(
                "{}: timed out after {}s (see daemon log: spawner: child I/O paths; if same_user=false, parent blocks until pre_exec/initgroups completes)",
                op_label,
                timeout.as_secs()
            )))
        }
    }
}

/// Await a `tddy-supervisor`-brokered operation under the same deadline the forked spawn backend
/// gets from [`spawn_blocking_with_timeout`].
///
/// An unreachable or refusing supervisor fails the RPC. There is deliberately no local spawn to fall
/// back to: doing the work here would run a session as the daemon's own user, which is the isolation
/// the supervisor exists to provide.
pub(crate) async fn await_supervised_with_timeout<T>(
    timeout: Duration,
    op_label: &'static str,
    operation: impl std::future::Future<Output = anyhow::Result<T>>,
) -> Result<T, Status> {
    match tokio::time::timeout(timeout, operation).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(e)) => {
            log::error!("{} failed: {:#}", op_label, e);
            Err(Status::internal(format!("{e:#}")))
        }
        Err(_elapsed) => {
            log::error!(
                "{} timed out after {}s (spawn_worker_request_timeout_secs) waiting for tddy-supervisor",
                op_label,
                timeout.as_secs()
            );
            Err(Status::deadline_exceeded(format!(
                "{}: tddy-supervisor did not answer within {}s",
                op_label,
                timeout.as_secs()
            )))
        }
    }
}

/// After a `new_branch_from_base` worktree is created, optionally push the freshly created branch to
/// its remote. Reads the actual created branch from the session's changeset (it may carry a
/// collision suffix), resolves the remote from the persisted integration base ref
/// (`<remote>/<branch>`) — falling back to main-worktree detection then `origin` — runs
/// `git push -u <remote> <branch>` from the worktree, and records `Changeset.remote_pushed = true`.
/// A push failure fails the session start — no silent fallback.
pub(crate) async fn push_new_branch_to_origin_if_requested(
    create_remote_branch: bool,
    intent: BranchWorktreeIntent,
    session_dir: &Path,
    worktree_path: &Path,
    timeout: Duration,
) -> Result<(), Status> {
    if !create_remote_branch || !matches!(intent, BranchWorktreeIntent::NewBranchFromBase) {
        return Ok(());
    }
    let session_dir = session_dir.to_path_buf();
    let worktree_path = worktree_path.to_path_buf();
    spawn_blocking_with_timeout(
        timeout,
        "StartSession: push new branch to remote",
        move || {
            let mut cs = tddy_core::read_changeset(&session_dir)
                .map_err(|e| anyhow::anyhow!("read changeset for remote push: {e}"))?;
            let branch = cs
                .branch
                .clone()
                .ok_or_else(|| anyhow::anyhow!("no branch recorded after worktree setup"))?;
            // Resolve the remote from the persisted integration base ref (`<remote>/<branch>`),
            // falling back to main-worktree detection then `origin` as the last resort.
            let remote = cs
                .effective_worktree_integration_base_ref
                .as_deref()
                .and_then(|r| r.split_once('/').map(|(remote, _)| remote.to_string()))
                .or_else(|| tddy_core::worktree::detect_default_remote_name(&worktree_path))
                .unwrap_or_else(|| "origin".to_string());
            tddy_core::worktree::push_new_branch_to_remote(&worktree_path, &branch, &remote)
                .map_err(|e| anyhow::anyhow!(e))?;
            cs.remote_pushed = true;
            tddy_core::write_changeset(&session_dir, &cs)
                .map_err(|e| anyhow::anyhow!("write changeset after remote push: {e}"))?;
            Ok(())
        },
    )
    .await
}

/// Resolves session token to GitHub user login.
pub type SessionUserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Resolves OS user to sessions base path.
pub type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// Resolve a request's `terminal_id`, defaulting an empty value to the reserved main terminal so
/// existing single-terminal clients keep working.
pub(crate) fn resolved_terminal_id(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        MAIN_TERMINAL_ID
    } else {
        trimmed
    }
}

/// Maximum size of a single terminal-output frame published to a client on attach. Chosen to stay
/// well under the LiveKit/WebRTC data-channel and gRPC-web message size limits while keeping the
/// number of replay frames for a long-lived session reasonable.
pub(crate) const TERMINAL_OUTPUT_FRAME_MAX_BYTES: usize = 32 * 1024;

/// Split a terminal capture buffer into ordered frames of at most `max_frame_bytes` each so a long
/// session history is replayed as several bounded frames instead of one oversized frame that could
/// exceed the transport's per-message limit and never reach the client.
///
/// An empty input yields no frames. Any non-empty input yields `ceil(len / max_frame_bytes)`
/// frames; concatenating them in order reproduces the input exactly.
///
/// Retained for the `sandbox_replay_tests` unit tests (the production sandbox path now uses
/// `TerminalCapture::replay_from` directly with offset-tagged frames).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn chunk_terminal_output(data: &[u8], max_frame_bytes: usize) -> Vec<bytes::Bytes> {
    data.chunks(max_frame_bytes)
        .map(bytes::Bytes::copy_from_slice)
        .collect()
}

/// Frames a newly attached sandbox-session subscriber receives before the live broadcast: the
/// mouse-tracking modes still in effect, then the retained output.
///
/// Without the prologue a browser attaching to a long-running sandbox session never learns the
/// application enabled mouse reporting, because the DECSET that enabled it was evicted from the
/// capture ring long ago and nothing re-emits it.
///
/// Retained for the `sandbox_replay_tests` unit tests (the production sandbox path now uses
/// `TerminalCapture::replay_from` directly with offset-tagged frames).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn sandbox_replay_frames(
    capture: &TerminalCapture,
    max_frame_bytes: usize,
) -> Vec<bytes::Bytes> {
    chunk_terminal_output(&capture.replay(), max_frame_bytes)
}

/// Derives the agent and recipe to relaunch a resumed session with, from its persisted
/// `.session.yaml`. Empty/whitespace-only values are treated as absent (`None`), mirroring the
/// spawner's trimming, so a legacy session with no persisted agent/recipe restores as `None`.
pub(crate) fn resume_agent_and_recipe(
    metadata: &tddy_core::SessionMetadata,
) -> (Option<String>, Option<String>) {
    fn non_blank(value: &Option<String>) -> Option<String> {
        value
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }
    (non_blank(&metadata.agent), non_blank(&metadata.recipe))
}
