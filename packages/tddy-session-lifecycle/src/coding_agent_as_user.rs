//! Running a coding agent's ACP child as the session's OS user, with piped stdio.
//!
//! The ACP driver talks to its child over stdin/stdout, so no PTY and no detached spawn will do. The
//! daemon's PTY path already drops privileges by wrapping argv in `setpriv`
//! ([`wrap_argv_for_privilege_drop`]), which keeps the stdio it was given; this reuses that, the same
//! passwd lookup ([`resolve_pty_os_user`]) and the same `HOME`/`PATH` overrides.
//!
//! It can do that only when this daemon process is itself privileged (root, as an unsupervised
//! daemon with a forked spawn worker is). A supervised daemon owns no privilege, so this refuses
//! with [`AsUserRefusal::Unprivileged`]; the launcher then starts the agent through the
//! supervisor's relayed spawn when one is configured ([`crate::coding_agent_relay`]), and refuses
//! the launch when none is. It is never run as the daemon's own user instead.

use std::path::PathBuf;

use tddy_core::AcpAgentCommand;
use tddy_daemon_kernel::privilege_drop::{
    pty_requires_privilege_drop, pty_user_env_overrides, resolve_pty_os_user,
    wrap_argv_for_privilege_drop, ResolvedPtyUser,
};
/// The GitHub token variables a daemon may have exported; a child run as a user must not inherit one.
use tddy_daemon_kernel::spawn_as_user::GITHUB_TOKEN_VARIABLES;

/// Why a command could not be made to run as the session's user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsUserRefusal {
    /// This daemon holds no privilege to become another user — the case a supervised host's
    /// `SpawnRelayed` exists for.
    Unprivileged(String),
    /// Anything else: an unknown user, no `setpriv`.
    Other(String),
}

impl std::fmt::Display for AsUserRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AsUserRefusal::Unprivileged(reason) | AsUserRefusal::Other(reason) => {
                f.write_str(reason)
            }
        }
    }
}

/// What this daemon process is allowed to do about another user.
#[derive(Debug, Clone)]
pub struct DropContext {
    pub uid: u32,
    pub gid: u32,
    /// Whether this process may become another user (it is root).
    pub privileged: bool,
    /// The `setpriv` that does it, when one is on `PATH`.
    pub setpriv: Option<PathBuf>,
}

impl DropContext {
    /// This process's own identity and the `setpriv` on its `PATH`.
    #[must_use]
    pub fn current() -> Self {
        #[cfg(unix)]
        {
            let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
            Self {
                uid,
                gid,
                privileged: uid == 0,
                setpriv: tddy_daemon_kernel::spawn_as_user::find_program_on_spawn_child_path(
                    "setpriv",
                ),
            }
        }
        #[cfg(not(unix))]
        {
            Self {
                uid: 0,
                gid: 0,
                privileged: false,
                setpriv: None,
            }
        }
    }
}

/// `command` made to run as `os_user`; unchanged when that is the user this process already is.
///
/// `Err` names why it cannot be, in words an operator can act on.
pub fn run_as_session_user(
    command: AcpAgentCommand,
    os_user: &str,
    drop: &DropContext,
) -> Result<AcpAgentCommand, AsUserRefusal> {
    let target = resolve_pty_os_user(os_user).map_err(AsUserRefusal::Other)?;
    wrap_for_user(command, os_user, &target, drop)
}

fn wrap_for_user(
    mut command: AcpAgentCommand,
    os_user: &str,
    target: &ResolvedPtyUser,
    drop: &DropContext,
) -> Result<AcpAgentCommand, AsUserRefusal> {
    if !pty_requires_privilege_drop(target.uid, target.gid, drop.uid, drop.gid) {
        return Ok(command);
    }
    if !drop.privileged {
        return Err(AsUserRefusal::Unprivileged(format!(
            "the session runs as '{os_user}' but this daemon is not privileged, and a piped-stdio \
             child cannot be started as another user without privilege; it will not run the agent \
             as the daemon's own user"
        )));
    }
    let setpriv = drop.setpriv.as_ref().ok_or_else(|| {
        AsUserRefusal::Other(format!(
            "starting the agent as '{os_user}' needs `setpriv` (util-linux) on this daemon's PATH, \
             and none was found"
        ))
    })?;
    let mut inner = vec!["env".to_string()];
    for variable in GITHUB_TOKEN_VARIABLES {
        inner.push("-u".to_string());
        inner.push(variable.to_string());
    }
    inner.push(command.program.to_string_lossy().into_owned());
    inner.extend(command.args.iter().cloned());
    let mut argv = wrap_argv_for_privilege_drop(&inner, target.uid, target.gid);
    argv.remove(0);
    command.program = setpriv.clone();
    command.args = argv;
    command.env.extend(pty_user_env_overrides(
        std::path::Path::new(&target.home_dir),
        None,
    ));
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_command() -> AcpAgentCommand {
        AcpAgentCommand {
            program: PathBuf::from("opencode"),
            args: vec!["acp".to_string()],
            env: vec![("K".to_string(), "V".to_string())],
            cwd: Some(PathBuf::from("/work")),
        }
    }

    fn a_user(uid: u32) -> ResolvedPtyUser {
        ResolvedPtyUser {
            uid,
            gid: uid,
            home_dir: "/home/dev".to_string(),
        }
    }

    fn a_daemon(uid: u32, privileged: bool, setpriv: bool) -> DropContext {
        DropContext {
            uid,
            gid: uid,
            privileged,
            setpriv: setpriv.then(|| PathBuf::from("/usr/bin/setpriv")),
        }
    }

    fn wrapped_by_a_privileged_daemon() -> AcpAgentCommand {
        wrap_for_user(a_command(), "dev", &a_user(1000), &a_daemon(0, true, true)).expect("wrapped")
    }

    #[test]
    fn the_daemons_own_user_runs_the_command_unchanged() {
        // Given
        let daemon = a_daemon(1000, false, false);

        // When
        let wrapped = wrap_for_user(a_command(), "dev", &a_user(1000), &daemon).expect("same user");

        // Then
        assert_eq!(
            (wrapped.program, wrapped.args),
            (PathBuf::from("opencode"), vec!["acp".to_string()])
        );
    }

    #[test]
    fn a_privileged_daemon_runs_it_behind_setpriv_as_the_user() {
        // When
        let wrapped = wrapped_by_a_privileged_daemon();

        // Then
        assert_eq!(
            (wrapped.program, wrapped.args[..2].to_vec()),
            (
                PathBuf::from("/usr/bin/setpriv"),
                vec!["--reuid".to_string(), "1000".to_string()]
            )
        );
    }

    #[test]
    fn a_privileged_daemon_still_runs_the_harness_with_its_arguments() {
        // When
        let wrapped = wrapped_by_a_privileged_daemon();

        // Then
        assert!(
            wrapped.args.windows(2).any(|w| w == ["opencode", "acp"]),
            "{:?}",
            wrapped.args
        );
    }

    #[test]
    fn a_privileged_daemon_unsets_the_github_tokens_it_holds() {
        // When
        let wrapped = wrapped_by_a_privileged_daemon();

        // Then
        assert!(
            wrapped.args.windows(2).any(|w| w == ["-u", "GITHUB_TOKEN"]),
            "{:?}",
            wrapped.args
        );
    }

    #[test]
    fn a_privileged_daemon_gives_the_agent_the_users_home() {
        // When
        let wrapped = wrapped_by_a_privileged_daemon();

        // Then
        assert!(wrapped
            .env
            .contains(&("HOME".to_string(), "/home/dev".to_string())));
    }

    #[test]
    fn a_privileged_daemon_keeps_the_launchs_own_environment_and_directory() {
        // When
        let wrapped = wrapped_by_a_privileged_daemon();

        // Then
        assert!(wrapped.env.contains(&("K".to_string(), "V".to_string())));
        assert_eq!(wrapped.cwd, Some(PathBuf::from("/work")));
    }

    #[test]
    fn an_unprivileged_daemon_refuses_another_user_rather_than_run_as_itself() {
        // Given
        let daemon = a_daemon(501, false, true);

        // When
        let refusal =
            wrap_for_user(a_command(), "dev", &a_user(1000), &daemon).expect_err("refused");

        // Then
        assert!(
            matches!(refusal, AsUserRefusal::Unprivileged(_))
                && refusal.to_string().contains("'dev'"),
            "{refusal}"
        );
    }

    #[test]
    fn a_privileged_daemon_without_setpriv_refuses_naming_it() {
        // Given
        let daemon = a_daemon(0, true, false);

        // When
        let refusal =
            wrap_for_user(a_command(), "dev", &a_user(1000), &daemon).expect_err("refused");

        // Then
        assert!(refusal.to_string().contains("setpriv"), "{refusal}");
    }
}
