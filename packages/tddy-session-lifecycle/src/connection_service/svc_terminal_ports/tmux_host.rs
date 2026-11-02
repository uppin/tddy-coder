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

/// The tmux binary this daemon runs: `TDDY_TMUX_BINARY` when set and non-empty, else `tmux` on
/// the daemon's `PATH`.
pub(super) fn tmux_binary() -> String {
    match std::env::var(TMUX_BINARY_ENV) {
        Ok(binary) if !binary.is_empty() => binary,
        _ => "tmux".to_string(),
    }
}

/// The argv of a tab's client attached to exactly `name` (`=` stops tmux matching a prefix).
pub(super) fn tmux_attach_argv(binary: &str, name: &str) -> Vec<String> {
    vec![
        binary.to_string(),
        "-T".to_string(),
        "clipboard".to_string(),
        "attach-session".to_string(),
        "-t".to_string(),
        format!("={name}"),
    ]
}

/// The sessions `tmux list-sessions -F LIST_SESSIONS_FORMAT` printed.
///
/// tmux prints exactly the format, one session per line, so a line without its three fields is not
/// a session and is skipped. The name comes first and may itself hold tabs, so the two counts are
/// read from the end of the line.
pub(super) fn parse_list_sessions(stdout: &str) -> Vec<TmuxSession> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.rsplitn(3, '\t');
            let attached = fields.next()?.parse().ok()?;
            let windows = fields.next()?.parse().ok()?;
            let name = fields.next()?;
            Some(TmuxSession {
                name: name.to_string(),
                windows,
                attached,
            })
        })
        .collect()
}

/// [`TmuxHost`] over the daemon host's tmux, run as the asking OS user.
pub(super) struct HostTmux;

#[async_trait]
impl TmuxHost for HostTmux {
    async fn list(&self, os_user: &str) -> Result<Vec<TmuxSession>, Status> {
        // TODO(tmux-attach): implement. BLOCKED on a design decision: the main agent PTY's as-user
        // path (`PtyRuntime`, which wraps argv in `setpriv`) is a terminal, so a one-shot listing
        // gets stdout and stderr as one stream and no separate stdout. See the node report.
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
