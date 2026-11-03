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
    plan_privilege_change_from, pty_user_env_overrides, resolve_pty_os_user,
    resolve_target_account, CannotBecomeUser, EffectiveIdentity, ResolvedPtyUser,
};
use tddy_daemon_kernel::spawn_as_user::run_output_as_user;
use tddy_daemon_kernel::user_paths::spawn_path_extra_for_home;
use tddy_rpc::Status;
use tddy_terminal_rpc::TmuxSession;

pub(crate) mod pane;
pub mod pane_env;

use pane::remain_on_exit_args;
pub use pane::{capture_final_screen, pane_state, PaneState};
use pane_env::{pane_command, remove_pane_env_file, write_pane_env_file, PaneEnvOwner};

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

/// The prefix of an agent's tmux session name, before the session id's characters.
const AGENT_SESSION_NAME_PREFIX: &str = "tddy-";

/// The tmux session an agent session runs in: `tddy-` and the full session id.
///
/// The whole id, never a prefix: the leading characters of a UUIDv7 are the high bits of its
/// millisecond timestamp, so a prefix repeats across sessions started close together. The name is
/// derivable from the id alone, so it stays authoritative even when the session's metadata cannot
/// be read. tmux reads `:` and `.` as target separators, so each is replaced by `_` — the character
/// tmux itself substitutes when it names a session. Stable for a session id.
pub fn agent_tmux_session_name(session_id: &str) -> String {
    let name: String = session_id
        .chars()
        .map(|c| if matches!(c, ':' | '.') { '_' } else { c })
        .collect();
    format!("{AGENT_SESSION_NAME_PREFIX}{name}")
}

/// The arguments of `tmux new-session` that start the pane command `command` detached in a session
/// `name`, in `worktree`: `new-session -d -s <name> -c <worktree> -- command`. The environment is not
/// among them — see [`pane_env`].
pub fn agent_new_session_args(name: &str, worktree: &Path, command: &[String]) -> Vec<String> {
    let mut args = vec![
        "new-session".to_string(),
        "-d".to_string(),
        "-s".to_string(),
        name.to_string(),
        "-c".to_string(),
        worktree.to_string_lossy().into_owned(),
        "--".to_string(),
    ];
    args.extend(command.iter().cloned());
    args
}

/// Start `argv` in a new detached tmux session `name` in `worktree`, as `os_user`, with `env` on the
/// pane, and return the pane process's pid.
///
/// A tmux pane inherits the *server's* environment, not a client's, so the pane's environment —
/// the target user's `HOME` and configured `PATH`, as a PTY child run as that user gets them, with
/// `env` on top — is written to a `0600` file under `session_dir` that the pane sources, deletes and
/// replaces with `argv` ([`pane_env`]). No value is on tmux's argv. A start that fails removes the
/// file itself.
///
/// `remain_on_exit` keeps the pane — dead, with its final screen — after the agent exits, so an
/// exit hook can read it with [`capture_final_screen`] (`#tmux-agent` 3/3); the hook's owner then
/// ends the session with [`kill_tmux_session`].
pub fn start_agent_tmux_session(
    os_user: &str,
    name: &str,
    worktree: &Path,
    session_dir: &Path,
    env: &[(String, String)],
    argv: &[String],
    remain_on_exit: bool,
) -> Result<u32, TmuxError> {
    let account = resolve_pty_os_user(os_user).map_err(TmuxError::new)?;
    let owner = (EffectiveIdentity::current().uid != account.uid).then_some(PaneEnvOwner {
        uid: account.uid,
        gid: account.gid,
    });
    let env_file = write_pane_env_file(session_dir, owner, &pane_environment(&account, env))?;
    let args = agent_new_session_args(name, worktree, &pane_command(&env_file, argv));
    let started = run_tmux_as_user(os_user, &args).and_then(|output| {
        if output.status.success() {
            Ok(())
        } else {
            Err(TmuxError::new(format!(
                "tmux could not start session {name:?}: {}",
                stderr_of(&output)
            )))
        }
    });
    if let Err(error) = started {
        remove_pane_env_file(&env_file);
        return Err(error);
    }
    if remain_on_exit {
        keep_pane_after_exit(os_user, name)?;
    }
    pane_pid(os_user, name)
}

/// Set `remain-on-exit` on session `name`, and if that fails, end the session: a start that reports
/// an error must not leave an agent running that its caller believes never started.
fn keep_pane_after_exit(os_user: &str, name: &str) -> Result<(), TmuxError> {
    let args = remain_on_exit_args(name);
    let set = run_tmux_as_user(os_user, &args).and_then(|output| {
        if output.status.success() {
            Ok(())
        } else {
            Err(TmuxError::new(format!(
                "tmux could not keep the pane of session {name:?} after exit: {}",
                stderr_of(&output)
            )))
        }
    });
    if let Err(error) = set {
        let _ = kill_tmux_session(os_user, name);
        return Err(error);
    }
    Ok(())
}

/// Kill the tmux session `name` of `os_user`, and with it the agent running in it. Killing a
/// session that is not there is not an error.
pub fn kill_tmux_session(os_user: &str, name: &str) -> Result<(), TmuxError> {
    let args = vec![
        "kill-session".to_string(),
        "-t".to_string(),
        exact_target(name),
    ];
    let output = run_tmux_as_user(os_user, &args)?;
    if output.status.success() || reports_no_session(&stderr_of(&output)) {
        return Ok(());
    }
    Err(TmuxError::new(format!(
        "tmux could not kill session {name:?}: {}",
        stderr_of(&output)
    )))
}

/// Whether `os_user`'s tmux has a session `name`. A run that is refused (not as that user, or
/// tmux not installed) answers `false`, and the reason is logged rather than dropped.
pub fn tmux_session_alive(os_user: &str, name: &str) -> bool {
    let args = vec![
        "has-session".to_string(),
        "-t".to_string(),
        exact_target(name),
    ];
    match run_tmux_as_user(os_user, &args) {
        Ok(output) => output.status.success(),
        Err(refusal) => {
            log::warn!(
                target: "tddy_daemon::tmux",
                "tmux session {name:?} of {os_user} is reported as not alive: {refusal}"
            );
            false
        }
    }
}

/// The pid of the process in the first pane of `os_user`'s tmux session `name`.
pub fn pane_pid(os_user: &str, name: &str) -> Result<u32, TmuxError> {
    // `=<name>:` names the session's current window. A bare `=<name>` prints nothing for a pane
    // format on tmux 3.6, so the window is named explicitly.
    let args = vec![
        "display-message".to_string(),
        "-p".to_string(),
        "-t".to_string(),
        format!("{}:", exact_target(name)),
        "#{pane_pid}".to_string(),
    ];
    let output = run_tmux_as_user(os_user, &args)?;
    if !output.status.success() {
        return Err(TmuxError::new(format!(
            "tmux could not read the pane of session {name:?}: {}",
            stderr_of(&output)
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse::<u32>().map_err(|_| {
        TmuxError::new(format!(
            "tmux reported no pane pid for session {name:?}: {:?}",
            stdout.trim()
        ))
    })
}

/// The `=`-prefixed target that names exactly `name`, not a session whose name it starts with.
fn exact_target(name: &str) -> String {
    format!("={name}")
}

/// Whether tmux's error says there is no session to kill. The session is absent, or the server that
/// would hold it is gone: a server exits with its last session, and the socket it leaves behind is
/// then reported as "no server running", or is missing altogether.
fn reports_no_session(stderr: &str) -> bool {
    let lowered = stderr.to_lowercase();
    lowered.contains("no server running")
        || lowered.contains("can't find session")
        || lowered.contains("session not found")
        || (lowered.contains("error connecting") && lowered.contains("no such file"))
}

/// What a tmux run wrote to stderr, trimmed.
fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

/// Run tmux as `os_user` with `args`, and hand back its output. A run that cannot happen — as that
/// user, or at all — is a [`TmuxError`] naming why; nothing runs as this daemon's own user instead.
fn run_tmux_as_user(os_user: &str, args: &[String]) -> Result<std::process::Output, TmuxError> {
    let binary =
        absolute_tmux(&tmux_binary()).map_err(|refusal| TmuxError::new(refusal.message()))?;
    let run = run_output_as_user(os_user, &binary, args).map_err(|e| {
        match e.downcast_ref::<CannotBecomeUser>() {
            Some(refusal) => TmuxError::new(refusal.to_string()),
            None => TmuxError::new(format!("cannot run tmux as {os_user}: {e}")),
        }
    })?;
    run.result.map_err(|e| {
        TmuxError::new(format!(
            "tmux ({}) could not be run: {e}",
            run.resolved_program.display()
        ))
    })
}

/// The environment of a pane run as `account`: the target user's `HOME` and configured `PATH`, as
/// the PTY runtime gives a child it runs as that user, with `env` on top. A pane inherits the tmux
/// server's environment, and that was taken from whichever client started the server, so the
/// user's own `HOME` and `PATH` are set on the pane explicitly.
fn pane_environment(account: &ResolvedPtyUser, env: &[(String, String)]) -> Vec<(String, String)> {
    let home = PathBuf::from(&account.home_dir);
    let path_extra = spawn_path_extra_for_home(&home);
    let mut pane = pty_user_env_overrides(&home, path_extra.as_deref());
    pane.retain(|(key, _)| !env.iter().any(|(set, _)| set == key));
    pane.extend(env.iter().cloned());
    pane
}

#[cfg(test)]
mod tests {
    use super::pane::capture_pane_args;
    use super::*;

    #[test]
    fn remain_on_exit_is_set_on_exactly_the_named_session() {
        assert_eq!(
            remain_on_exit_args("tddy-1234abcd"),
            vec![
                "set-option",
                "-t",
                "=tddy-1234abcd:",
                "remain-on-exit",
                "on"
            ]
        );
    }

    #[test]
    fn the_final_screen_is_the_whole_scrollback_of_exactly_the_named_session() {
        assert_eq!(
            capture_pane_args("tddy-1234abcd"),
            vec!["capture-pane", "-p", "-S", "-", "-t", "=tddy-1234abcd:"]
        );
    }

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
    fn an_agent_tmux_session_is_named_after_the_full_session_id() {
        // When
        let name = agent_tmux_session_name("01900000-0000-7000-8000-0000000f5c01");

        // Then
        assert_eq!(name, "tddy-01900000-0000-7000-8000-0000000f5c01");
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
    fn an_agent_session_starts_detached_in_the_worktree_with_its_pane_command_and_no_environment() {
        // Given a pane environment holding a secret
        let session_dir = tempfile::tempdir().unwrap();
        let env_file = pane_env::write_pane_env_file(
            session_dir.path(),
            None,
            &[(
                "ANTHROPIC_AUTH_TOKEN".to_string(),
                "sk-sentinel-secret".to_string(),
            )],
        )
        .unwrap();
        let argv = ["/usr/bin/claude".to_string(), "--session-id".to_string()];

        // When
        let args = agent_new_session_args(
            "tddy-01900000",
            std::path::Path::new("/home/dev/feature"),
            &pane_env::pane_command(&env_file, &argv),
        );

        // Then
        assert_eq!(
            args[..7],
            [
                "new-session",
                "-d",
                "-s",
                "tddy-01900000",
                "-c",
                "/home/dev/feature",
                "--"
            ]
        );
        assert_eq!(args[args.len() - 2..], argv);
        assert!(!args.iter().any(|a| a == "-e"), "{args:?}");
        assert!(
            args.iter().all(|a| !a.contains("sk-sentinel-secret")),
            "no environment value is on tmux's argv: {args:?}"
        );
    }
}
