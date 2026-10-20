use super::super::DaemonSessionHost;

use std::path::Path;
use std::sync::Arc;

use tddy_daemon_extensions::{OpenedRoom, ReachedSession};
use tddy_rpc::{RequestTransport, Status};

impl DaemonSessionHost {
    /// Open the session's room over a checkout this daemon holds, unless it is open already.
    ///
    /// The one place a room is opened outside a split start, and the reason session *creation* no
    /// longer opens one: a room is what a session is reached through, so it is created when
    /// something first reaches for it. Every caller here is such a reach — a client connecting to
    /// the session, an owning daemon being admitted to it — and each of them is already waiting on
    /// a round trip to the room server by asking. What opens it is the integration behind the
    /// host's [`SessionReach`](tddy_daemon_extensions::SessionReach) port.
    ///
    /// `Ok(None)` means no integration hosts rooms on this daemon; each caller decides what that
    /// means for it.
    pub(crate) async fn ensure_session_room(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_root: &Path,
    ) -> Result<Option<OpenedRoom>, Status> {
        self.session_reach
            .open_now(
                ReachedSession {
                    session_id: session_id.to_string(),
                    session_dir: session_dir.to_path_buf(),
                    worktree_root: worktree_root.to_path_buf(),
                    // The core asks, not a client: nothing arrived over a transport.
                    arrived_over: RequestTransport::Direct,
                },
                Arc::new(self.clone()),
            )
            .await
    }
}
