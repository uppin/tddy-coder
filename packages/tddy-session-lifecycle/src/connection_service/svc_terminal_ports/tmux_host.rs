//! [`HostTmux`]: the [`TmuxHost`] port over the daemon host's tmux.
//!
//! The tmux helpers themselves live in [`tddy_cli_sessions::tmux`] — moved down so the agent
//! launcher can use them too (`#tmux-agent` 1/3).

use async_trait::async_trait;
use tddy_cli_sessions::tmux::list_tmux_sessions;
use tddy_rpc::Status;
use tddy_terminal_rpc::{TmuxHost, TmuxSession};

/// [`TmuxHost`] over the daemon host's tmux, run as the asking OS user.
pub(super) struct HostTmux;

#[async_trait]
impl TmuxHost for HostTmux {
    async fn list(&self, os_user: &str) -> Result<Vec<TmuxSession>, Status> {
        let os_user = os_user.to_string();
        tokio::task::spawn_blocking(move || list_tmux_sessions(&os_user))
            .await
            .map_err(|e| Status::internal(format!("tmux listing task failed: {e}")))?
    }
}
