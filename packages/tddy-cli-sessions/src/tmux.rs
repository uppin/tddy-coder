//! The host's tmux: the sessions a "tmux" tab can attach to, the client a tab runs, and the tmux
//! sessions a Claude CLI agent runs *in* (`#tmux-agent` 1/3).
//!
//! Everything runs **as the session's OS user**, so it reaches that user's own tmux server — the
//! sessions they started in their own terminals. Nothing here changes a tmux option: a client
//! announces that it accepts OSC 52 with `-T clipboard`, a per-client feature, so tmux copy-mode
//! copies reach the browser without editing the user's tmux configuration.
//!
//! The binary is `TDDY_TMUX_BINARY` when set and non-empty, else `tmux` on the daemon's `PATH`.
//!
//! Moved here from `tddy-session-lifecycle`'s `tmux_host.rs`, so `tddy-agent-launch` — which
//! `tddy-session-lifecycle` depends on — can use it too.

use std::path::{Path, PathBuf};

use tddy_daemon_kernel::privilege_drop::{
    plan_privilege_change_from, resolve_target_account, CannotBecomeUser, EffectiveIdentity,
};
use tddy_daemon_kernel::spawn_as_user::run_output_as_user;
use tddy_rpc::Status;
use tddy_terminal_rpc::TmuxSession;

/// The environment variable that names the tmux binary, as `TDDY_CLAUDE_BINARY` names Claude's.
pub const TMUX_BINARY_ENV: &str = "TDDY_TMUX_BINARY";

/// The `list-sessions` format [`parse_list_sessions`] reads: name, windows, attached clients.
pub const LIST_SESSIONS_FORMAT: &str = "#{session_name}\t#{session_windows}\t#{session_attached}";

/// The tmux binary this daemon runs: `TDDY_TMUX_BINARY` when set and non-empty, else `tmux` on
/// the daemon's `PATH`.
pub fn tmux_binary() -> String {
    match std::env::var(TMUX_BINARY_ENV) {
        Ok(binary) if !binary.is_empty() => binary,
        _ => "tmux".to_string(),
    }
}

/// The argv of a tab's client attached to exactly `name` (`=` stops tmux matching a prefix).
pub fn tmux_attach_argv(binary: &str, name: &str) -> Vec<String> {
    vec![
        binary.to_string(),
        "-T".to_string(),
        "clipboard".to_string(),
        "attach-session".to_string(),
        "-t".to_string(),
        format!("={name}"),
    ]
}

/// A tmux session name a "tmux-new" tab may create: trimmed, non-empty, and free of the `:` and `.`
/// tmux reads as target separators. Anything else is `INVALID_ARGUMENT`.
pub fn tmux_session_name(raw: &str) -> Result<String, Status> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(Status::invalid_argument("a new tmux session needs a name"));
    }
    if name.contains([':', '.']) {
        return Err(Status::invalid_argument(format!(
            "a tmux session name may not contain ':' or '.': {name:?}"
        )));
    }
    Ok(name.to_string())
}

/// The argv of a tab's client on a new tmux session `name` in `worktree`; `-A` attaches instead
/// when a session of that name already exists.
pub fn tmux_new_session_argv(binary: &str, name: &str, worktree: &std::path::Path) -> Vec<String> {
    vec![
        binary.to_string(),
        "-T".to_string(),
        "clipboard".to_string(),
        "new-session".to_string(),
        "-A".to_string(),
        "-s".to_string(),
        name.to_string(),
        "-c".to_string(),
        worktree.to_string_lossy().into_owned(),
    ]
}

/// The sessions `tmux list-sessions -F LIST_SESSIONS_FORMAT` printed.
///
/// tmux prints exactly the format, one session per line, so a line without its three fields is not
/// a session and is skipped. The name comes first and may itself hold tabs, so the two counts are
/// read from the end of the line.
pub fn parse_list_sessions(stdout: &str) -> Vec<TmuxSession> {
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

/// The absolute path of `binary`: itself when absolute, else the first `PATH` entry holding an
/// executable of that name. The kernel's as-user spawn anchors a relative program to the daemon's
/// working directory, so a bare `tmux` has to be found here, on the daemon's `PATH`.
pub(crate) fn absolute_tmux(binary: &str) -> Result<PathBuf, Status> {
    let path = Path::new(binary);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let search = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&search)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| Status::failed_precondition(format!("tmux ({binary}) is not installed")))
}

/// Whether this daemon can run a child as `os_user`, decided before anything spawns. A daemon that
/// is not root cannot become another user, and a tab must then be refused with the kernel's named
/// [`CannotBecomeUser`] error — never started as the daemon user.
pub fn ensure_reachable(os_user: &str, current: &EffectiveIdentity) -> Result<(), Status> {
    let target = resolve_target_account(os_user).map_err(Status::failed_precondition)?;
    plan_privilege_change_from(target, current)
        .map(|_| ())
        .map_err(|refusal| Status::failed_precondition(refusal.to_string()))
}

/// What `tmux list-sessions` said when it ran and exited non-zero: a missing server is an empty
/// listing, anything else is an error.
pub(crate) fn listing_failure(stderr: &str) -> Result<Vec<TmuxSession>, Status> {
    let lowered = stderr.to_lowercase();
    if lowered.contains("no server running") || lowered.contains("error connecting") {
        Ok(Vec::new())
    } else {
        Err(Status::internal(format!(
            "tmux list-sessions failed: {}",
            stderr.trim()
        )))
    }
}

/// Run `tmux list-sessions` as `os_user`. Blocking — an async caller wraps it in `spawn_blocking`.
///
/// The child inherits the daemon's environment, `TMUX_TMPDIR` included, so the daemon's own
/// setting decides where tmux looks for the user's socket.
pub fn list_tmux_sessions(os_user: &str) -> Result<Vec<TmuxSession>, Status> {
    let binary = absolute_tmux(&tmux_binary())?;
    let args = vec![
        "list-sessions".to_string(),
        "-F".to_string(),
        LIST_SESSIONS_FORMAT.to_string(),
    ];
    let run = run_output_as_user(os_user, &binary, &args).map_err(|e| {
        match e.downcast_ref::<CannotBecomeUser>() {
            Some(refusal) => Status::failed_precondition(refusal.to_string()),
            None => Status::failed_precondition(format!("cannot run tmux as {os_user}: {e}")),
        }
    })?;
    let output = run.result.map_err(|e| {
        Status::failed_precondition(format!(
            "tmux ({}) could not be run: {e}",
            run.resolved_program.display()
        ))
    })?;
    if output.status.success() {
        Ok(parse_list_sessions(&String::from_utf8_lossy(
            &output.stdout,
        )))
    } else {
        listing_failure(&String::from_utf8_lossy(&output.stderr))
    }
}

/// Why an agent-session tmux operation failed: tmux could not be run as the user, or it refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxError {
    message: String,
}

impl TmuxError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TmuxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TmuxError {}

/// The tmux session an agent session runs in: `tddy-` and the first eight characters of its id.
///
/// A valid tmux session name (no `:` or `.`), stable for a session id.
pub fn agent_tmux_session_name(session_id: &str) -> String {
    // TODO(claude-in-tmux): implement
    let _ = session_id;
    unimplemented!("TODO(claude-in-tmux): agent_tmux_session_name")
}

/// The arguments of `tmux new-session` that start `argv` detached in a session `name`, in
/// `worktree`, with `env` set on the pane: `new-session -d -s <name> -c <worktree> -e K=V -- argv`.
pub fn agent_new_session_args(
    name: &str,
    worktree: &Path,
    env: &[(String, String)],
    argv: &[String],
) -> Vec<String> {
    // TODO(claude-in-tmux): implement
    let _ = (name, worktree, env, argv);
    unimplemented!("TODO(claude-in-tmux): agent_new_session_args")
}

/// Start `argv` in a new detached tmux session `name` in `worktree`, as `os_user`, with `env` set
/// on the pane (a tmux pane inherits the *server's* environment, not a client's). Returns the pane
/// process's pid.
pub fn start_agent_tmux_session(
    os_user: &str,
    name: &str,
    worktree: &Path,
    env: &[(String, String)],
    argv: &[String],
) -> Result<u32, TmuxError> {
    // TODO(claude-in-tmux): implement
    let _ = (os_user, name, worktree, env, argv);
    unimplemented!("TODO(claude-in-tmux): start_agent_tmux_session")
}

/// Kill the tmux session `name` of `os_user`, and with it the agent running in it. Killing a
/// session that is not there is not an error.
pub fn kill_tmux_session(os_user: &str, name: &str) -> Result<(), TmuxError> {
    // TODO(claude-in-tmux): implement
    let _ = (os_user, name);
    unimplemented!("TODO(claude-in-tmux): kill_tmux_session")
}

/// Whether `os_user`'s tmux has a session `name`.
pub fn tmux_session_alive(os_user: &str, name: &str) -> bool {
    // TODO(claude-in-tmux): implement
    let _ = (os_user, name);
    unimplemented!("TODO(claude-in-tmux): tmux_session_alive")
}

/// The pid of the process in the first pane of `os_user`'s tmux session `name`.
pub fn pane_pid(os_user: &str, name: &str) -> Result<u32, TmuxError> {
    // TODO(claude-in-tmux): implement
    let _ = (os_user, name);
    unimplemented!("TODO(claude-in-tmux): pane_pid")
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
    fn a_new_session_tab_creates_or_attaches_the_named_session_in_the_worktree() {
        // When
        let argv = tmux_new_session_argv(
            "/usr/bin/tmux",
            "work",
            std::path::Path::new("/home/dev/feature"),
        );

        // Then
        assert_eq!(
            argv,
            vec![
                "/usr/bin/tmux",
                "-T",
                "clipboard",
                "new-session",
                "-A",
                "-s",
                "work",
                "-c",
                "/home/dev/feature"
            ]
        );
    }

    #[test]
    fn a_new_session_name_is_trimmed() {
        assert_eq!(tmux_session_name("  work ").unwrap(), "work");
    }

    #[test]
    fn an_empty_new_session_name_is_refused() {
        assert_eq!(
            tmux_session_name("   ").unwrap_err().code(),
            tddy_rpc::Code::InvalidArgument
        );
    }

    #[test]
    fn a_new_session_name_with_a_target_separator_is_refused() {
        for name in ["a:b", "a.b"] {
            assert_eq!(
                tmux_session_name(name).unwrap_err().code(),
                tddy_rpc::Code::InvalidArgument,
                "{name}"
            );
        }
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

    #[test]
    fn a_user_this_daemon_cannot_become_is_refused_naming_both_accounts() {
        // Given a daemon that runs as an unprivileged user other than root
        let daemon = EffectiveIdentity {
            uid: 4242,
            gid: 4242,
            name: "unprivileged".to_string(),
        };

        // When a tab is asked for as root
        let refusal = ensure_reachable("root", &daemon).unwrap_err();

        // Then it is refused, naming who runs and who was asked for
        assert_eq!(refusal.code(), tddy_rpc::Code::FailedPrecondition);
        assert!(
            refusal.message().contains("'unprivileged'"),
            "{}",
            refusal.message()
        );
        assert!(
            refusal.message().contains("'root'"),
            "{}",
            refusal.message()
        );
    }

    #[test]
    fn a_missing_server_lists_nothing() {
        // tmux says one of these, depending on whether the socket file exists
        for stderr in [
            "no server running on /tmp/tmux-1/default",
            "error connecting to /tmp/x (No such file)",
        ] {
            assert_eq!(listing_failure(stderr).unwrap(), Vec::<TmuxSession>::new());
        }
    }

    #[test]
    fn any_other_listing_failure_is_an_internal_error_carrying_what_tmux_said() {
        // When
        let refusal = listing_failure("  boom \n").unwrap_err();

        // Then
        assert_eq!(refusal.code(), tddy_rpc::Code::Internal);
        assert_eq!(refusal.message(), "tmux list-sessions failed: boom");
    }

    #[test]
    fn an_agent_tmux_session_is_named_after_the_short_session_id() {
        // When
        let name = agent_tmux_session_name("01900000-0000-7000-8000-0000000f5c01");

        // Then
        assert_eq!(name, "tddy-01900000");
    }

    #[test]
    fn an_agent_tmux_session_name_is_a_valid_new_session_name() {
        // When
        let name = agent_tmux_session_name("0190.0000:ab-cdef");

        // Then
        assert!(name.starts_with("tddy-"), "{name}");
        assert!(!name.contains([':', '.']), "{name}");
    }

    #[test]
    fn an_agent_session_starts_detached_in_the_worktree_with_its_environment_and_argv() {
        // When
        let args = agent_new_session_args(
            "tddy-01900000",
            std::path::Path::new("/home/dev/feature"),
            &[
                ("TDDY_SOCKET".to_string(), "/tmp/s.sock".to_string()),
                ("CLAUDE_CODE_NO_FLICKER".to_string(), "1".to_string()),
            ],
            &[
                "/usr/bin/claude".to_string(),
                "--session-id".to_string(),
                "abc".to_string(),
            ],
        );

        // Then
        assert_eq!(
            args,
            vec![
                "new-session",
                "-d",
                "-s",
                "tddy-01900000",
                "-c",
                "/home/dev/feature",
                "-e",
                "TDDY_SOCKET=/tmp/s.sock",
                "-e",
                "CLAUDE_CODE_NO_FLICKER=1",
                "--",
                "/usr/bin/claude",
                "--session-id",
                "abc"
            ]
        );
    }
}
