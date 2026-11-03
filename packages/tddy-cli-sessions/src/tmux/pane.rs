//! The agent pane of a tmux-run session after its process exits (`#tmux-agent` 3/3): kept with
//! `remain-on-exit`, probed for being dead, and read back whole for an exit hook.

use super::{exact_target, reports_no_session, run_tmux_as_user, stderr_of, TmuxError};

/// The arguments of `tmux set-option` that keep session `name`'s pane after its process exits:
/// `set-option -t =<name>: remain-on-exit on`. `remain-on-exit` is a window option, so the target is
/// the session's current window (`:`), which a bare `=<name>` does not name on tmux 3.6 ("no such
/// window"). `=` stops tmux matching a prefix.
pub fn remain_on_exit_args(name: &str) -> Vec<String> {
    vec![
        "set-option".to_string(),
        "-t".to_string(),
        format!("{}:", exact_target(name)),
        "remain-on-exit".to_string(),
        "on".to_string(),
    ]
}

/// The arguments of `tmux capture-pane` that print session `name`'s pane with its whole scrollback:
/// `capture-pane -p -S - -t =<name>:`. The window is named, as [`super::pane_pid`] does: a bare session
/// target does not name a pane on tmux 3.6 ("can't find pane").
pub fn capture_pane_args(name: &str) -> Vec<String> {
    vec![
        "capture-pane".to_string(),
        "-p".to_string(),
        "-S".to_string(),
        "-".to_string(),
        "-t".to_string(),
        format!("{}:", exact_target(name)),
    ]
}

/// The final screen of the dead pane of `os_user`'s tmux session `name` — everything the agent
/// printed, scrollback included. The exit-hook source of a tmux-run main.
pub fn capture_final_screen(os_user: &str, name: &str) -> Result<String, TmuxError> {
    let output = run_tmux_as_user(os_user, &capture_pane_args(name))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(TmuxError::new(format!(
            "tmux could not capture the pane of session {name:?}: {}",
            stderr_of(&output)
        )))
    }
}

/// Where a session's agent pane is, as the watcher of a tmux-run agent reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneState {
    /// The agent's process is running.
    Running,
    /// The agent's process has exited and its pane is kept (`remain-on-exit`) with its final screen.
    Exited,
    /// The tmux session is gone, so there is no pane at all.
    SessionGone,
}

/// The state of the pane of `os_user`'s tmux session `name`. `#{pane_dead}` is `1` once the pane's
/// process has exited under `remain-on-exit`.
pub fn pane_state(os_user: &str, name: &str) -> Result<PaneState, TmuxError> {
    // The window is named explicitly, as [`super::pane_pid`] does: a bare session target prints nothing
    // for a pane format.
    let args = vec![
        "display-message".to_string(),
        "-p".to_string(),
        "-t".to_string(),
        format!("{}:", exact_target(name)),
        "#{pane_dead}".to_string(),
    ];
    let output = run_tmux_as_user(os_user, &args)?;
    if !output.status.success() {
        let stderr = stderr_of(&output);
        return if reports_no_session(&stderr) {
            Ok(PaneState::SessionGone)
        } else {
            Err(TmuxError::new(format!(
                "tmux could not read the pane state of session {name:?}: {stderr}"
            )))
        };
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    match stdout.trim() {
        // tmux 3.6 answers an absent session's pane with success and no output, not an error.
        "" => Ok(PaneState::SessionGone),
        "0" => Ok(PaneState::Running),
        "1" => Ok(PaneState::Exited),
        other => Err(TmuxError::new(format!(
            "tmux reported no pane state for session {name:?}: {other:?}"
        ))),
    }
}
