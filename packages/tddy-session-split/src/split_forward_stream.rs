//! Forwarding a split start's codebase half, with the codebase host's progress relayed to the caller.
//!
//! A split start asks the codebase host to cut the worktree. That host reports its phases and
//! keep-alives as it works, and they are passed on, unchanged, to the operator's own stream — so the
//! operator watches the worktree being cut on the host that cuts it. The forward has no overall
//! deadline: the codebase host is bounded by its keep-alives, and a silent one by the relay's idle
//! timeout (see `docs/ft/daemon/remote-managed-worktree.md` § Failure is atomic).
//!
//! The codebase host runs its half as a task and names it first (`started`). That event is relayed
//! with the host's own task id — it is what the operator's Cancel addresses — and remembered, so
//! cancelling the agent host's own start can cancel that task too.

use std::sync::Mutex;

use tddy_daemon_kernel::peer_calls::{self, ForwardedStartFailure};
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_service::proto::session::start_session_event::Event as StartEvent;
use tddy_service::proto::session::{StartSessionRequest, StartSessionResponse, StartTaskStarted};
use tddy_session_files::attachment_progress::AttachmentProgressSink;

/// The task the codebase host runs its half of a split start as, once it has said so.
#[derive(Default)]
pub struct CodebaseStartTask(Mutex<Option<StartTaskStarted>>);

impl CodebaseStartTask {
    fn remember(&self, started: &StartTaskStarted) {
        *self.0.lock().expect("codebase start task lock") = Some(started.clone());
    }

    /// The codebase host's task, when it has reported one.
    pub fn reported(&self) -> Option<StartTaskStarted> {
        self.0.lock().expect("codebase start task lock").clone()
    }
}

/// Start `request` on the codebase host `codebase_instance_id`, relaying every event it reports into
/// `progress` as it arrives and noting its task in `task`. Returns the host's result, or how the
/// start failed.
pub async fn forward_split_start(
    room: &CommonRoom,
    codebase_instance_id: &str,
    request: &StartSessionRequest,
    progress: &AttachmentProgressSink,
    task: &CodebaseStartTask,
) -> Result<StartSessionResponse, ForwardedStartFailure> {
    peer_calls::forward_start_session_relaying(room, codebase_instance_id, request, &|event| {
        if let Some(StartEvent::Started(started)) = &event.event {
            task.remember(started);
        }
        progress.relay(event)
    })
    .await
}
