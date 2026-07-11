//! PTY action runtime — spawns interactive tools as [`tddy_task::Task`] entries.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tddy_task::{TaskBody, TaskChannel, TaskContext, TaskHandle, TaskRegistry, TaskStatus};
use tokio::sync::{mpsc, oneshot};

use crate::pty_registry::{PtyControl, PtyRegistry};

/// Default terminal size for spawned PTY sessions.
pub const DEFAULT_TERM_ROWS: u16 = 24;
pub const DEFAULT_TERM_COLS: u16 = 220;

/// PTY reader buffer size.
const PTY_READ_BUF: usize = 4096;

/// Specification for spawning a process inside a PTY.
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

/// Ready signal emitted once the PTY is open and the child has been spawned.
pub struct PtyReady {
    pub pid: u32,
    pub master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub current_size: Arc<Mutex<PtySize>>,
}

/// Spawn a PTY-backed tool as a registered task.
pub struct PtyRuntime;

impl PtyRuntime {
    pub async fn spawn(
        registry: &TaskRegistry,
        pty_registry: &PtyRegistry,
        spec: PtySpawnSpec,
        ready_tx: oneshot::Sender<Result<PtyReady, String>>,
    ) -> Arc<TaskHandle> {
        let (channel, stdin_rx) = TaskChannel::pty("0", "pty");
        let body = PtyTaskBody {
            spec: spec.clone(),
            pty_registry: pty_registry.clone(),
            stdin_rx: stdin_rx.expect("pty channel must have stdin"),
            ready_tx: Some(ready_tx),
        };
        registry
            .spawn(body, spec.kind, spec.session_id, vec![channel])
            .await
    }
}

struct PtyTaskBody {
    spec: PtySpawnSpec,
    pty_registry: PtyRegistry,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
    ready_tx: Option<oneshot::Sender<Result<PtyReady, String>>>,
}

struct SetupResult {
    pid: u32,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    current_size: Arc<Mutex<PtySize>>,
    exit_rx: oneshot::Receiver<TaskStatus>,
}

#[async_trait]
impl TaskBody for PtyTaskBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let task_id = ctx.task_id();
        let task_id_log = task_id.clone();
        let cancel = ctx.cancel_token();
        let output_ch = match ctx.channel("0") {
            Some(ch) => ch,
            None => {
                return TaskStatus::Failed {
                    message: "missing PTY channel".into(),
                };
            }
        };

        let (setup_tx, setup_rx) = oneshot::channel::<Result<SetupResult, String>>();
        let spec = self.spec.clone();
        let stdin_rx = self.stdin_rx;
        let ready_tx = self.ready_tx;
        let output_ch_thread = Arc::clone(&output_ch);

        std::thread::spawn(move || {
            if let Err(e) = open_pty_and_pump(spec, stdin_rx, output_ch_thread, setup_tx) {
                log::warn!(
                    target: "tddy_daemon::pty_runtime",
                    "PTY setup failed for task {}: {}",
                    task_id_log,
                    e
                );
            }
        });

        let setup = match setup_rx.await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                if let Some(tx) = ready_tx {
                    let _ = tx.send(Err(e.clone()));
                }
                return TaskStatus::Failed { message: e };
            }
            Err(_) => {
                return TaskStatus::Failed {
                    message: "PTY setup thread did not respond".into(),
                };
            }
        };

        ctx.register_child_pid(setup.pid);

        if let Some(tx) = ready_tx {
            let _ = tx.send(Ok(PtyReady {
                pid: setup.pid,
                master: Arc::clone(&setup.master),
                current_size: Arc::clone(&setup.current_size),
            }));
        }

        self.pty_registry
            .insert(
                task_id.clone(),
                PtyControl {
                    master: Arc::clone(&setup.master),
                    current_size: Arc::clone(&setup.current_size),
                    terminal_id: self.spec.terminal_id.clone(),
                    kind: self.spec.kind.clone(),
                },
            )
            .await;

        let exit_status = tokio::select! {
            _ = cancel.cancelled() => {
                #[cfg(unix)]
                {
                    let _ = crate::session_deletion::signal_pid(setup.pid as i32, libc::SIGTERM);
                    let _ = crate::session_deletion::signal_pid(setup.pid as i32, libc::SIGKILL);
                }
                ctx.deregister_child_pid(setup.pid);
                self.pty_registry.remove(&task_id).await;
                return TaskStatus::Cancelled;
            }
            status = setup.exit_rx => status.unwrap_or(TaskStatus::Failed {
                message: "PTY exit monitor dropped".into(),
            }),
        };

        ctx.deregister_child_pid(setup.pid);
        self.pty_registry.remove(&task_id).await;
        exit_status
    }
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

fn open_pty_and_pump(
    spec: PtySpawnSpec,
    mut stdin_rx: mpsc::UnboundedReceiver<Bytes>,
    output_ch: Arc<TaskChannel>,
    setup_tx: oneshot::Sender<Result<SetupResult, String>>,
) -> Result<(), String> {
    if spec.argv.is_empty() {
        let _ = setup_tx.send(Err("empty argv".into()));
        return Err("empty argv".into());
    }

    // Resolve OS-user impersonation up front — before allocating a PTY — so an unresolvable user
    // fails loudly without leaking a pty pair. On success this yields the child's HOME/PATH
    // overrides plus the final argv (front-loaded with a `setpriv` privilege drop when the target
    // differs from the daemon's own identity). With no `os_user`, the argv/env pass through as-is.
    #[cfg(unix)]
    let (final_argv, user_env): (Vec<String>, Vec<(String, String)>) = match spec.os_user.as_deref()
    {
        None => (spec.argv.clone(), Vec::new()),
        Some(user) => {
            let resolved = match resolve_pty_os_user(user) {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("cannot resolve os_user '{user}': {e}");
                    let _ = setup_tx.send(Err(msg.clone()));
                    return Err(msg);
                }
            };
            let home = std::path::PathBuf::from(&resolved.home_dir);
            let path_extra = crate::tddy_user_config::spawn_path_extra_for_home(&home);
            let env = pty_user_env_overrides(&home, path_extra.as_deref());
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
            (argv, env)
        }
    };

    #[cfg(not(unix))]
    let (final_argv, user_env): (Vec<String>, Vec<(String, String)>) =
        (spec.argv.clone(), Vec::new());

    let pty_system = native_pty_system();
    let initial_size = PtySize {
        rows: DEFAULT_TERM_ROWS,
        cols: DEFAULT_TERM_COLS,
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = match pty_system.openpty(initial_size) {
        Ok(pair) => pair,
        Err(e) => {
            let msg = format!("openpty failed: {e}");
            let _ = setup_tx.send(Err(msg.clone()));
            return Err(msg);
        }
    };

    let master = Arc::new(Mutex::new(pair.master));
    let current_size = Arc::new(Mutex::new(initial_size));
    let (exit_tx, exit_rx) = oneshot::channel();

    // Reader: start BEFORE child spawn so fast-exiting stubs cannot miss PTY output.
    let master_for_reader = Arc::clone(&master);
    let output_ch_reader = Arc::clone(&output_ch);
    std::thread::spawn(move || {
        let reader = {
            let m = master_for_reader.lock().unwrap();
            m.try_clone_reader()
        };
        match reader {
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon::pty_runtime",
                    "PTY reader: try_clone_reader failed: {e}"
                );
            }
            Ok(mut r) => {
                let mut buf = vec![0u8; PTY_READ_BUF];
                loop {
                    match std::io::Read::read(&mut r, &mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            output_ch_reader.write(Bytes::copy_from_slice(&buf[..n]));
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    let mut cmd = CommandBuilder::new(&final_argv[0]);
    for arg in &final_argv[1..] {
        cmd.arg(arg);
    }
    cmd.cwd(&spec.worktree_path);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    // Impersonation HOME/PATH (empty when no `os_user`), applied before `spec.env` so a managed
    // session's explicit overrides still win. `setpriv` preserves these across the privilege drop.
    for (key, value) in &user_env {
        cmd.env(key, value);
    }

    for (key, value) in &spec.env {
        cmd.env(key, value);
    }

    let child = match pair.slave.spawn_command(cmd) {
        Ok(child) => child,
        Err(e) => {
            let msg = format!("spawn failed: {e}");
            let _ = setup_tx.send(Err(msg.clone()));
            return Err(msg);
        }
    };
    drop(pair.slave);

    let pid = match child.process_id() {
        Some(pid) => pid,
        None => {
            let msg = "spawned child has no pid".to_string();
            let _ = setup_tx.send(Err(msg.clone()));
            return Err(msg);
        }
    };

    let _ = setup_tx.send(Ok(SetupResult {
        pid,
        master: Arc::clone(&master),
        current_size: Arc::clone(&current_size),
        exit_rx,
    }));

    // Writer: stdin mpsc → PTY master.
    let master_for_writer = Arc::clone(&master);
    std::thread::spawn(move || {
        let writer = {
            let m = master_for_writer.lock().unwrap();
            m.take_writer()
        };
        match writer {
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon::pty_runtime",
                    "PTY writer: take_writer failed: {e}"
                );
            }
            Ok(mut w) => {
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    tokio::task::block_in_place(|| loop {
                        let data = handle.block_on(stdin_rx.recv());
                        match data {
                            None => break,
                            Some(bytes) => {
                                if std::io::Write::write_all(&mut w, &bytes).is_err() {
                                    break;
                                }
                            }
                        }
                    });
                } else {
                    while let Some(bytes) = stdin_rx.blocking_recv() {
                        if std::io::Write::write_all(&mut w, &bytes).is_err() {
                            break;
                        }
                    }
                }
            }
        }
    });

    // Exit monitor: wait for child, report terminal status.
    let mut child_monitor = child;
    std::thread::spawn(move || {
        let status = match child_monitor.wait() {
            Ok(_) => TaskStatus::Completed { exit_code: Some(0) },
            Err(e) => TaskStatus::Failed {
                message: format!("child wait error: {e}"),
            },
        };
        let _ = exit_tx.send(status);
    });

    Ok(())
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
