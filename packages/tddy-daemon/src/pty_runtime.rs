//! PTY action runtime — spawns interactive tools as [`tddy_task::Task`] entries.
//!
//! The transport-agnostic PTY core (spawn + I/O pump + master registry) lives in [`tddy_pty`].
//! This module keeps the daemon-only concern of **OS-user impersonation**: it resolves a target
//! `os_user` to its uid/gid/home, front-loads a `setpriv` privilege drop when needed, and computes
//! the child's `HOME`/`PATH` overrides — then hands the fully-resolved `argv`/`env` to
//! [`tddy_pty::PtyRuntime`], which has no notion of an `os_user`.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_task::{TaskBody, TaskContext, TaskHandle, TaskRegistry, TaskStatus};
use tokio::sync::oneshot;

use crate::pty_registry::PtyRegistry;

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
    /// Extra environment variables set on the spawned process (in addition to the inherited env).
    pub env: Vec<(String, String)>,
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
        match resolve_final_argv_env(&spec) {
            Ok((argv, env)) => {
                let core = tddy_pty::PtySpawnSpec {
                    argv,
                    worktree_path: spec.worktree_path,
                    session_id: spec.session_id,
                    terminal_id: spec.terminal_id,
                    kind: spec.kind,
                    env,
                };
                tddy_pty::PtyRuntime::spawn(registry, pty_registry, core, ready_tx).await
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
/// argv is front-loaded with a `setpriv` privilege drop.
#[cfg(unix)]
fn resolve_final_argv_env(spec: &PtySpawnSpec) -> Result<ResolvedArgvEnv, String> {
    if spec.argv.is_empty() {
        return Err("empty argv".into());
    }
    match spec.os_user.as_deref() {
        None => Ok((spec.argv.clone(), spec.env.clone())),
        Some(user) => {
            let resolved = resolve_pty_os_user(user)
                .map_err(|e| format!("cannot resolve os_user '{user}': {e}"))?;
            let home = std::path::PathBuf::from(&resolved.home_dir);
            let path_extra = crate::tddy_user_config::spawn_path_extra_for_home(&home);
            let mut env = pty_user_env_overrides(&home, path_extra.as_deref());
            // The managed session's explicit overrides win over the impersonation defaults.
            env.extend(spec.env.iter().cloned());
            let current_uid = unsafe { libc::getuid() };
            let current_gid = unsafe { libc::getgid() };
            let argv = if pty_requires_privilege_drop(
                resolved.uid,
                resolved.gid,
                current_uid,
                current_gid,
            ) {
                log::info!(
                    target: "tddy_daemon::pty_runtime",
                    "PTY child for os_user '{user}': dropping to uid={} gid={} via setpriv",
                    resolved.uid,
                    resolved.gid
                );
                wrap_argv_for_privilege_drop(&spec.argv, resolved.uid, resolved.gid)
            } else {
                spec.argv.clone()
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

/// Environment overrides applied to a PTY child impersonating a target OS user.
///
/// Sets `HOME` to the target user's home directory (so per-user config/credentials resolve there)
/// and prepends the user's configured `PATH` extra ahead of the daemon's `PATH` (so a user-local
/// install such as `~/.local/bin/claude` is found). Mirrors the env `spawner::spawn_as_user`
/// applies to non-interactive spawns.
pub fn pty_user_env_overrides(
    home_dir: &std::path::Path,
    path_extra: Option<&str>,
) -> Vec<(String, String)> {
    vec![
        ("HOME".to_string(), home_dir.to_string_lossy().into_owned()),
        (
            "PATH".to_string(),
            crate::spawner::merge_spawn_child_path(path_extra),
        ),
    ]
}

/// Whether spawning as the target user requires dropping privileges: true unless the target
/// uid+gid already match the daemon's current identity (dev / single-user, where no setuid is
/// needed).
pub fn pty_requires_privilege_drop(
    target_uid: u32,
    target_gid: u32,
    current_uid: u32,
    current_gid: u32,
) -> bool {
    !(target_uid == current_uid && target_gid == current_gid)
}

/// Wrap `argv` so it execs behind `setpriv`, dropping to the target user's uid/gid with
/// initialized supplementary groups. `setpriv` preserves the environment, so the HOME/PATH
/// overrides applied to the command are kept.
pub fn wrap_argv_for_privilege_drop(argv: &[String], uid: u32, gid: u32) -> Vec<String> {
    let mut wrapped = vec![
        "setpriv".to_string(),
        "--reuid".to_string(),
        uid.to_string(),
        "--regid".to_string(),
        gid.to_string(),
        "--init-groups".to_string(),
        "--".to_string(),
    ];
    wrapped.extend_from_slice(argv);
    wrapped
}

/// The uid/gid/home of a target OS user, resolved from the passwd database.
#[cfg(unix)]
struct ResolvedPtyUser {
    uid: u32,
    gid: u32,
    home_dir: String,
}

/// Resolve `os_user` to its uid/gid/home via `getpwnam_r`. Mirrors the passwd lookup in
/// `spawner::spawn_as_user`.
#[cfg(unix)]
fn resolve_pty_os_user(os_user: &str) -> Result<ResolvedPtyUser, String> {
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    let name = std::ffi::CString::new(os_user).map_err(|e| format!("invalid username: {e}"))?;
    let ret = unsafe {
        libc::getpwnam_r(
            name.as_ptr(),
            passwd.as_mut_ptr(),
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if ret != 0 || result.is_null() {
        return Err(format!("user '{os_user}' not found"));
    }
    let passwd = unsafe { &*result };
    if passwd.pw_dir.is_null() {
        return Err(format!("user '{os_user}' has no home directory"));
    }
    let home_dir = unsafe { std::ffi::CStr::from_ptr(passwd.pw_dir) }
        .to_string_lossy()
        .into_owned();
    Ok(ResolvedPtyUser {
        uid: passwd.pw_uid,
        gid: passwd.pw_gid,
        home_dir,
    })
}

/// The login shell (`pw_shell`) of `os_user` from the passwd database, or `None` when the entry is
/// missing or has no shell. Preferred over the daemon's `$SHELL` for Bash terminals, since the
/// daemon's own `$SHELL` (systemd / nix) is not the target user's interactive shell.
#[cfg(unix)]
pub fn login_shell_for_os_user(os_user: &str) -> Option<String> {
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    let name = std::ffi::CString::new(os_user).ok()?;
    let ret = unsafe {
        libc::getpwnam_r(
            name.as_ptr(),
            passwd.as_mut_ptr(),
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if ret != 0 || result.is_null() {
        return None;
    }
    let passwd = unsafe { &*result };
    if passwd.pw_shell.is_null() {
        return None;
    }
    let shell = unsafe { std::ffi::CStr::from_ptr(passwd.pw_shell) }
        .to_string_lossy()
        .into_owned();
    if shell.is_empty() || shell.ends_with("/nologin") || shell.ends_with("/false") {
        None
    } else {
        Some(shell)
    }
}

#[cfg(not(unix))]
pub fn login_shell_for_os_user(_os_user: &str) -> Option<String> {
    None
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
            os_user: Some("tddy".to_string()),
        };

        // When reading back the target user
        // Then it is preserved on the spec
        assert_eq!(spec.os_user.as_deref(), Some("tddy"));
    }
}
