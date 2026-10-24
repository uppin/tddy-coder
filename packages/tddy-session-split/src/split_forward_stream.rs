//! Forwarding a split start's codebase half, with the codebase host's progress relayed to the caller.
//!
//! A split start asks the codebase host to cut the worktree. That host reports its phases and
//! keep-alives as it works, and they are passed on, unchanged, to the operator's own stream — so the
//! operator watches the worktree being cut on the host that cuts it. The forward has no overall
//! deadline: the codebase host is bounded by its keep-alives, and a silent one by the relay's idle
//! timeout (see `docs/ft/daemon/remote-managed-worktree.md` § Failure is atomic).

use tddy_daemon_kernel::peer_calls::{self, ForwardedStartFailure};
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse};
use tddy_session_files::attachment_progress::AttachmentProgressSink;

/// Start `request` on the codebase host `codebase_instance_id`, relaying every event it reports into
/// `progress` as it arrives. Returns the host's result, or how the start failed.
pub async fn forward_split_start(
    room: &CommonRoom,
    codebase_instance_id: &str,
    request: &StartSessionRequest,
    progress: &AttachmentProgressSink,
) -> Result<StartSessionResponse, ForwardedStartFailure> {
    peer_calls::forward_start_session_relaying(room, codebase_instance_id, request, &|event| {
        progress.relay(event)
    })
    .await
}
