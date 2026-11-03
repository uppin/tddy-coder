//! The exit-hook source of a tmux-run main (`#tmux-agent` 3/3): a hook runs when the *agent*
//! exits, with the pane's final screen as its output — never when only a tmux client ended.

use std::sync::{Arc, Mutex};

use tddy_task::TerminalCapture;

use super::tmux_reattach::MainExit;
use super::{CliSessionManager, ExitHook};
use crate::tmux::TmuxError;

/// The output an exit hook is handed for a tmux-run main that ended as `exit`: the final screen
/// as a capture when the agent ended, `None` — and `final_screen` never called — when only the
/// client did.
pub fn exit_hook_capture(
    exit: MainExit,
    final_screen: impl FnOnce() -> Result<String, TmuxError>,
) -> Option<Arc<Mutex<TerminalCapture>>> {
    // TODO(harnesses-in-tmux): implement
    let _ = (exit, final_screen);
    unimplemented!("TODO(harnesses-in-tmux): exit_hook_capture")
}

impl CliSessionManager {
    /// Run `on_exit` once the agent of `session_id`'s tmux session `name` has exited, with the
    /// pane's final screen (`remain-on-exit`), then end the tmux session. A client exit never runs
    /// it. Registered as the session's pending exit, so `exit_hook_settled` waits on it.
    pub fn run_exit_hook_on_agent_exit(
        &self,
        session_id: &str,
        os_user: &str,
        name: &str,
        on_exit: ExitHook,
    ) {
        // TODO(harnesses-in-tmux): implement
        let _ = (self, session_id, os_user, name, on_exit);
        unimplemented!("TODO(harnesses-in-tmux): run_exit_hook_on_agent_exit")
    }
}
