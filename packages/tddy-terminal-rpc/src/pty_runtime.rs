//! PTY action runtime — spawns interactive tools as [`tddy_task::Task`] entries.
//!
//! The transport-agnostic PTY core (spawn + I/O pump + master registry) lives in [`tddy_pty`].
//! This module keeps the daemon-only concern of **OS-user impersonation**: it resolves a target
//! `os_user` to its uid/gid/home, front-loads a `setpriv` privilege drop when needed, and computes
//! the child's `HOME`/`PATH` overrides — then hands the fully-resolved `argv`/`env` to
//! [`tddy_pty::PtyRuntime`], which has no notion of an `os_user`.
//!
//! It stays in `tddy-daemon` for that reason: the impersonation it exists for is
//! [`tddy_daemon_kernel::privilege_drop`]'s, and depending on that crate from a subsystem crate
//! would put the whole LiveKit SDK inside `tddy-tools`' `--no-default-features` in-jail build,
//! which carries none. What *did* leave in `#unbundle` node 6 is the passwd lookup that had no
//! daemon state behind it: [`tddy_terminal_rpc::login_shell`] answers which shell a login terminal
//! runs, beside the `StartTerminalSession` handler that asks. No facade is left behind for it —
//! nothing outside this crate ever reached it through here.

/// Becoming another OS user, derived in [`tddy_daemon_kernel::privilege_drop`].
///
/// These five are the only symbols of this module a subsystem crate reaches, and both crates that
/// reach them — `tddy-host-service`'s `ssh_agent` and `tddy-worktree-service`'s
/// `remote_git_service` — have left this one. Re-exported so every caller here keeps its
/// `crate::pty_runtime::…` path.
pub use tddy_daemon_kernel::privilege_drop::{
    pty_requires_privilege_drop, pty_user_env_overrides, resolve_pty_os_user,
    wrap_argv_for_privilege_drop, ResolvedPtyUser,
};

#[cfg(unix)]
use tddy_daemon_kernel::privilege_drop::{
    plan_privilege_change_from, resolve_target_account, EffectiveIdentity, PrivilegeChange,
};

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_task::{TaskBody, TaskContext, TaskHandle, TaskRegistry, TaskStatus};
use tokio::sync::oneshot;

use tddy_pty::PtyRegistry;

// Re-exported from the shared core so existing daemon import paths keep working.
pub use tddy_pty::{PtyReady, DEFAULT_TERM_COLS, DEFAULT_TERM_ROWS};

/// Specification for spawning a process inside a PTY, including the daemon-only target OS user.
#[derive(Debug, Clone)]
pub struct PtySpawnSpec {
    pub argv: Vec<String>,
    pub worktree_path: PathBuf,
    pub session_id: String,
    pub terminal_id: String,
    pub kind: String,
    /// Environment variables set on the spawned process, on top of what `env_base` starts it from.
    pub env: Vec<(String, String)>,
    /// What the child starts from before `env` is applied — see [`tddy_pty::PtyEnvBase`].
    pub env_base: tddy_pty::PtyEnvBase,
    /// Target OS user to impersonate for this spawn. When `Some`, the child receives that user's
    /// `HOME`/`PATH` (see [`pty_user_env_overrides`]). Callers with no user context (e.g. Bash
    /// terminals) pass `None`, leaving the child under the daemon's own identity.
    pub os_user: Option<String>,
}

/// Spawn a PTY-backed tool as a registered task.
///
/// Resolves any `os_user` impersonation up front (before allocating a PTY), then delegates the
/// actual spawn + I/O pump to [`tddy_pty::PtyRuntime`]. An unresolvable user fails loudly via a
/// task that reaches `Failed` and reports the reason on `ready_tx`, without leaking a PTY pair.
pub struct PtyRuntime;

impl PtyRuntime {
    pub async fn spawn(
        registry: &TaskRegistry,
        pty_registry: &PtyRegistry,
        spec: PtySpawnSpec,
        ready_tx: oneshot::Sender<Result<PtyReady, String>>,
    ) -> Arc<TaskHandle> {
        Self::spawn_with_exit_hook(registry, pty_registry, spec, ready_tx, None).await
    }

    /// [`spawn`](Self::spawn), running `exit_hook` when the process has exited, before the terminal
    /// leaves the PTY registry (see [`tddy_pty::PtyExitHook`]).
    pub async fn spawn_with_exit_hook(
        registry: &TaskRegistry,
        pty_registry: &PtyRegistry,
        spec: PtySpawnSpec,
        ready_tx: oneshot::Sender<Result<PtyReady, String>>,
        exit_hook: Option<tddy_pty::PtyExitHook>,
    ) -> Arc<TaskHandle> {
        match resolve_final_argv_env(&spec) {
            Ok((argv, env)) => {
                let core = tddy_pty::PtySpawnSpec {
                    argv,
                    worktree_path: spec.worktree_path,
                    session_id: spec.session_id,
                    terminal_id: spec.terminal_id,
                    kind: spec.kind,
                    env,
                    env_base: spec.env_base,
                };
                tddy_pty::PtyRuntime::spawn_with_exit_hook(
                    registry,
                    pty_registry,
                    core,
                    ready_tx,
                    exit_hook,
                )
                .await
            }
            Err(msg) => {
                let _ = ready_tx.send(Err(msg.clone()));
                let (channel, _stdin_rx) = tddy_task::TaskChannel::pty("0", "pty");
                registry
                    .spawn(
                        FailedSpawnBody { message: msg },
                        spec.kind,
                        spec.session_id,
                        vec![channel],
                    )
                    .await
            }
        }
    }
}

/// Task body for a spawn that failed to resolve before a PTY could be opened. It reaches `Failed`
/// immediately so the registry retention/cap policy still tracks the attempt.
struct FailedSpawnBody {
    message: String,
}

#[async_trait]
impl TaskBody for FailedSpawnBody {
    async fn run(self: Box<Self>, _ctx: TaskContext) -> TaskStatus {
        TaskStatus::Failed {
            message: self.message,
        }
    }
}

/// The final `(argv, env)` a spawn resolves to: the command line and the environment overrides.
type ResolvedArgvEnv = (Vec<String>, Vec<(String, String)>);

/// Resolve the daemon's [`PtySpawnSpec`] into the final `(argv, env)` the shared PTY runtime
/// spawns verbatim. With no `os_user`, argv/env pass through unchanged. With one, the target user's
/// `HOME`/`PATH` are prepended and — when the target differs from the daemon's own identity — the
/// argv is front-loaded with a `setpriv` privilege drop — or the spawn is refused with the kernel's
/// `CannotBecomeUser` when this daemon is not root, the same refusal the tmux listing makes.
#[cfg(unix)]
fn resolve_final_argv_env(spec: &PtySpawnSpec) -> Result<ResolvedArgvEnv, String> {
    resolve_final_argv_env_as(spec, &EffectiveIdentity::current())
}

/// [`resolve_final_argv_env`] with the daemon's identity passed in, so the refusal is testable on
/// a host where the test is not root.
#[cfg(unix)]
fn resolve_final_argv_env_as(
    spec: &PtySpawnSpec,
    current: &EffectiveIdentity,
) -> Result<ResolvedArgvEnv, String> {
    if spec.argv.is_empty() {
        return Err("empty argv".into());
    }
    match spec.os_user.as_deref() {
        None => Ok((spec.argv.clone(), spec.env.clone())),
        Some(user) => {
            let target = resolve_target_account(user)
                .map_err(|e| format!("cannot resolve os_user '{user}': {e}"))?;
            let home = std::path::PathBuf::from(&target.home);
            let path_extra = crate::tddy_user_config::spawn_path_extra_for_home(&home);
            let mut env = pty_user_env_overrides(&home, path_extra.as_deref());
            // The managed session's explicit overrides win over the impersonation defaults.
            env.extend(spec.env.iter().cloned());
            // The kernel decides, as it does for the tmux listing: refuse before any PTY opens when
            // this daemon cannot become the user, instead of launching a `setpriv` that fails inside.
            let argv = match plan_privilege_change_from(target, current)
                .map_err(|refusal| refusal.to_string())?
            {
                PrivilegeChange::None => spec.argv.clone(),
                PrivilegeChange::Drop(target) => {
                    log::info!(
                        target: "tddy_daemon::pty_runtime",
                        "PTY child for os_user '{user}': dropping to uid={} gid={} via setpriv",
                        target.uid,
                        target.gid
                    );
                    wrap_argv_for_privilege_drop(&spec.argv, target.uid, target.gid)
                }
            };
            Ok((argv, env))
        }
    }
}

#[cfg(not(unix))]
fn resolve_final_argv_env(spec: &PtySpawnSpec) -> Result<ResolvedArgvEnv, String> {
    if spec.argv.is_empty() {
        return Err("empty argv".into());
    }
    Ok((spec.argv.clone(), spec.env.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A PTY child impersonating a target OS user must receive that user's `HOME`, so tools read
    /// their per-user config and credentials (e.g. claude's `~/.claude`) from the impersonated
    /// home rather than the daemon's.
    #[test]
    fn user_env_overrides_set_home_to_the_target_user_home() {
        // Given the target user's home directory
        let home = Path::new("/home/tddy");

        // When building the child environment overrides
        let env = pty_user_env_overrides(home, None);

        // Then HOME points at the target user's home
        let home_value = env
            .iter()
            .find(|(k, _)| k == "HOME")
            .map(|(_, v)| v.as_str());
        assert_eq!(home_value, Some("/home/tddy"));
    }

    /// The child `PATH` is prefixed with the target user's configured extra path so a user-local
    /// install (e.g. `~/.local/bin/claude`) resolves ahead of the daemon's minimal systemd `PATH`.
    #[test]
    fn user_env_overrides_prepend_the_user_path_extra() {
        // Given the target user's home and their extra PATH entry
        let home = Path::new("/home/tddy");

        // When building the child environment overrides
        let env = pty_user_env_overrides(home, Some("/home/tddy/.local/bin"));

        // Then the user's bin dir is prepended to PATH
        let path_value = env
            .iter()
            .find(|(k, _)| k == "PATH")
            .map(|(_, v)| v.as_str())
            .expect("PATH override must be present");
        assert!(
            path_value.starts_with("/home/tddy/.local/bin:"),
            "user path extra must be prepended, was: {path_value}"
        );
    }

    /// No privilege drop is needed when the target user is already the daemon's own identity
    /// (dev / single-user), so the child spawns without a setuid.
    #[test]
    fn no_privilege_drop_for_the_daemons_own_user() {
        // Given the daemon runs as uid/gid 1000 and the target is the same user
        // When deciding whether to drop privileges
        // Then no drop is required
        assert!(!pty_requires_privilege_drop(1000, 1000, 1000, 1000));
    }

    /// A privilege drop is required when the target user differs from the daemon's identity
    /// (e.g. root daemon spawning as a regular user).
    #[test]
    fn privilege_drop_required_for_a_different_user() {
        // Given the daemon runs as root and the target is uid/gid 1000
        // When deciding whether to drop privileges
        // Then a drop is required
        assert!(pty_requires_privilege_drop(1000, 1000, 0, 0));
    }

    /// When impersonation requires dropping privileges, the child is launched behind a `setpriv`
    /// front so it execs under the target user's uid/gid with initialized supplementary groups,
    /// preserving the already-set `HOME`/`PATH` env.
    #[test]
    fn wraps_the_command_behind_a_setpriv_privilege_drop_launcher() {
        // Given the claude argv and the target user's uid/gid
        let argv = vec![
            "claude".to_string(),
            "--model".to_string(),
            "opus".to_string(),
        ];

        // When wrapping it for a privilege drop to uid/gid 1000
        let wrapped = wrap_argv_for_privilege_drop(&argv, 1000, 1000);

        // Then setpriv leads, drops to the target ids with initialized groups, then execs the argv
        assert_eq!(
            wrapped,
            vec![
                "setpriv",
                "--reuid",
                "1000",
                "--regid",
                "1000",
                "--init-groups",
                "--",
                "claude",
                "--model",
                "opus",
            ]
        );
    }

    /// The spawn spec carries the target OS user end-to-end, so the interactive claude session
    /// runs under the impersonated user rather than the daemon's identity.
    #[test]
    fn spawn_spec_carries_the_target_os_user() {
        // Given a spec describing an impersonated claude spawn
        let spec = PtySpawnSpec {
            argv: vec!["claude".to_string()],
            worktree_path: PathBuf::from("/tmp/worktree"),
            session_id: "session-1".to_string(),
            terminal_id: "main".to_string(),
            kind: "claude-cli".to_string(),
            env: Vec::new(),
            env_base: tddy_pty::PtyEnvBase::Inherited,
            os_user: Some("tddy".to_string()),
        };

        // When reading back the target user
        // Then it is preserved on the spec
        assert_eq!(spec.os_user.as_deref(), Some("tddy"));
    }

    /// A spec running `claude` as `os_user`.
    #[cfg(unix)]
    fn a_spec_running_as(os_user: &str) -> PtySpawnSpec {
        PtySpawnSpec {
            argv: vec!["claude".to_string()],
            worktree_path: PathBuf::from("/tmp/worktree"),
            session_id: "session-1".to_string(),
            terminal_id: "main".to_string(),
            kind: "claude-cli".to_string(),
            env: Vec::new(),
            env_base: tddy_pty::PtyEnvBase::Inherited,
            os_user: Some(os_user.to_string()),
        }
    }

    #[cfg(unix)]
    fn a_daemon_running_as(
        uid: u32,
        gid: u32,
        name: &str,
    ) -> tddy_daemon_kernel::privilege_drop::EffectiveIdentity {
        tddy_daemon_kernel::privilege_drop::EffectiveIdentity {
            uid,
            gid,
            name: name.to_string(),
        }
    }

    /// The tmux listing refuses a daemon that cannot become the session's user with the kernel's
    /// `CannotBecomeUser`; the PTY spawn must refuse the same way, before it opens a PTY, so the
    /// two cannot disagree about a host.
    #[cfg(unix)]
    #[test]
    fn a_daemon_that_cannot_become_the_user_refuses_with_the_kernels_named_error() {
        // Given a daemon that is not root, and a session run as another user (root exists everywhere)
        let daemon = a_daemon_running_as(4242, 4242, "tddy");
        let spec = a_spec_running_as("root");

        // When the spawn is resolved
        let refusal = resolve_final_argv_env_as(&spec, &daemon).unwrap_err();

        // Then it is the kernel's refusal, naming both users
        let expected = tddy_daemon_kernel::privilege_drop::CannotBecomeUser {
            running_as: "tddy".to_string(),
            requested: "root".to_string(),
        }
        .to_string();
        assert_eq!(refusal, expected);
    }

    /// A root daemon spawning as another user keeps today's launch: behind `setpriv`.
    #[cfg(unix)]
    #[test]
    fn a_root_daemon_drops_to_another_user_behind_setpriv() {
        // Given a root daemon and a session run as `nobody`, whose ids are not root's
        let daemon = a_daemon_running_as(0, 0, "root");
        let spec = a_spec_running_as("nobody");

        // When the spawn is resolved
        let (argv, _env) = resolve_final_argv_env_as(&spec, &daemon).expect("root may drop");

        // Then the command line is front-loaded with setpriv
        assert_eq!(argv.first().map(String::as_str), Some("setpriv"));
        assert_eq!(argv.last().map(String::as_str), Some("claude"));
    }

    /// The same-user case needs no drop and must not be refused or wrapped.
    #[cfg(unix)]
    #[test]
    fn the_daemons_own_user_is_neither_refused_nor_wrapped() {
        // Given a daemon running as root and a session run as root
        let daemon = a_daemon_running_as(0, 0, "root");
        let spec = a_spec_running_as("root");

        // When the spawn is resolved
        let (argv, _env) = resolve_final_argv_env_as(&spec, &daemon).expect("same user");

        // Then the argv is untouched
        assert_eq!(argv, vec!["claude".to_string()]);
    }
}
