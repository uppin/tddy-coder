//! The host's tmux: which sessions a "tmux" tab can attach to, and the client a tab runs.
//!
//! Everything runs **as the session's OS user**, so it reaches that user's own tmux server — the
//! sessions they started in their own terminals. Nothing here changes a tmux option: a tab's client
//! announces that it accepts OSC 52 with `-T clipboard`, a per-client feature, so tmux copy-mode
//! copies reach the browser without editing the user's tmux configuration.
//!
//! The binary is `TDDY_TMUX_BINARY` when set and non-empty, else `tmux` on the daemon's `PATH`.

use async_trait::async_trait;
use tddy_rpc::Status;
use tddy_terminal_rpc::{TmuxHost, TmuxSession};

/// The environment variable that names the tmux binary, as `TDDY_CLAUDE_BINARY` names Claude's.
pub(super) const TMUX_BINARY_ENV: &str = "TDDY_TMUX_BINARY";

/// The `list-sessions` format [`parse_list_sessions`] reads: name, windows, attached clients.
pub(super) const LIST_SESSIONS_FORMAT: &str =
    "#{session_name}\t#{session_windows}\t#{session_attached}";

/// The tmux binary this daemon runs.
pub(super) fn tmux_binary() -> String {
    // TODO(tmux-attach): implement
    let _ = TMUX_BINARY_ENV;
    String::new()
}

/// The argv of a tab's client attached to exactly `name` (`=` stops tmux matching a prefix).
pub(super) fn tmux_attach_argv(binary: &str, name: &str) -> Vec<String> {
    // TODO(tmux-attach): implement
    let _ = (binary, name);
    Vec::new()
}

/// The sessions `tmux list-sessions -F LIST_SESSIONS_FORMAT` printed.
pub(super) fn parse_list_sessions(stdout: &str) -> Vec<TmuxSession> {
    // TODO(tmux-attach): implement
    let _ = stdout;
    Vec::new()
}

/// [`TmuxHost`] over the daemon host's tmux, run as the asking OS user.
pub(super) struct HostTmux;

#[async_trait]
impl TmuxHost for HostTmux {
    async fn list(&self, os_user: &str) -> Result<Vec<TmuxSession>, Status> {
        // TODO(tmux-attach): implement
        let _ = (os_user, LIST_SESSIONS_FORMAT, parse_list_sessions("").len());
        Err(Status::unimplemented(
            "listing tmux sessions is not implemented",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_reads_name_windows_and_attached_clients_per_line() {
        // Given
        let stdout = "alpha\t3\t0\nbeta\t1\t2\n";

        // When
        let sessions = parse_list_sessions(stdout);

        // Then
        assert_eq!(
            sessions,
            vec![
                TmuxSession {
                    name: "alpha".to_string(),
                    windows: 3,
                    attached: 0,
                },
                TmuxSession {
                    name: "beta".to_string(),
                    windows: 1,
                    attached: 2,
                },
            ]
        );
    }

    #[test]
    fn a_session_name_with_spaces_is_kept_whole() {
        // Given
        let stdout = "my work\t2\t1\n";

        // When
        let sessions = parse_list_sessions(stdout);

        // Then
        assert_eq!(sessions[0].name, "my work");
    }

    #[test]
    fn an_empty_listing_has_no_sessions() {
        assert_eq!(parse_list_sessions(""), Vec::<TmuxSession>::new());
    }

    #[test]
    fn a_tab_attaches_with_the_clipboard_feature_to_exactly_the_named_session() {
        // When
        let argv = tmux_attach_argv("/usr/bin/tmux", "alpha");

        // Then
        assert_eq!(
            argv,
            vec![
                "/usr/bin/tmux",
                "-T",
                "clipboard",
                "attach-session",
                "-t",
                "=alpha"
            ]
        );
    }
}
