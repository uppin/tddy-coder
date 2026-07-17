//! Per-session terminal manager for the coder participant — bash "tabs" backed by [`tddy_pty`].
//!
//! The coder already runs as the target OS user, so terminals spawn the user's login shell
//! (fallback `/bin/bash`) in the session worktree with no impersonation. Mirrors the daemon's design
//! (`cli_session_manager`): a rolling capture buffer for replay to late subscribers, a broadcast
//! fan-out of live output, and OSC-resize interception on the input path. Started terminals get a
//! fresh UUIDv7 id (never the reserved [`MAIN_TERMINAL_ID`]) and kind `"bash"`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tddy_pty::Bytes;
use tddy_task::{TaskId, TaskRegistry};
use tokio::sync::{broadcast, mpsc, watch, RwLock};

/// Reserved terminal id for a session's original (agent) terminal. It is not managed here — it is
/// torn down via Delete/Signal on the daemon — so stopping it via `StopTerminalSession` is rejected.
pub const MAIN_TERMINAL_ID: &str = "main";

/// Handle to a running shell in a PTY.
///
/// Output flows through the task's PTY channel: `stdout_tx` fans out live bytes and `capture`
/// holds a rolling replay buffer for late subscribers. Resize is applied via the shared
/// [`tddy_pty::PtyRegistry`], keyed by the owning task.
pub struct PtyHandle {
    pub terminal_id: String,
    pub kind: String,
    pub pid: u32,
    stdin_tx: mpsc::UnboundedSender<Bytes>,
    pub stdout_tx: broadcast::Sender<Bytes>,
    pub capture: Arc<Mutex<Vec<u8>>>,
    pub pty_done: watch::Receiver<bool>,
    task_id: TaskId,
    pty_registry: tddy_pty::PtyRegistry,
}

impl PtyHandle {
    /// Resize the PTY to the given dimensions (SIGWINCH), updating the stored size.
    pub async fn resize(&self, rows: u16, cols: u16) {
        self.pty_registry.resize(&self.task_id, rows, cols).await;
    }

    /// Forward input to the PTY stdin, intercepting an embedded OSC resize escape.
    ///
    /// When `\x1b]resize;{cols};{rows}\x07` is found, the PTY is resized and the escape bytes are
    /// not forwarded to the shell. Mirrors the daemon's input handling so resize works over the
    /// unary `SendTerminalInput` transport.
    pub fn send_input(&self, data: Bytes) {
        let (resize, remaining) = strip_resize(&data);
        if let Some((cols, rows)) = resize {
            let pty_registry = self.pty_registry.clone();
            let task_id = self.task_id.clone();
            tokio::spawn(async move {
                pty_registry.resize(&task_id, rows, cols).await;
            });
        }
        if !remaining.is_empty() {
            let _ = self.stdin_tx.send(remaining);
        }
    }
}

/// Manages a session's started bash terminals, keyed by `terminal_id`.
pub struct TerminalManager {
    task_registry: TaskRegistry,
    pty_registry: tddy_pty::PtyRegistry,
    terminals: Arc<RwLock<HashMap<String, Arc<PtyHandle>>>>,
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalManager {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            task_registry: TaskRegistry::new(),
            pty_registry: tddy_pty::PtyRegistry::new(),
            terminals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a bash terminal (`shell_path`, the user's login shell resolved at the RPC layer) in
    /// `worktree_path`. Returns the handle with a fresh `terminal_id` and kind `"bash"`.
    pub async fn start_terminal(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        shell_path: &str,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        let terminal_id = uuid::Uuid::now_v7().to_string();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let spec = tddy_pty::PtySpawnSpec {
            argv: vec![shell_path.to_string()],
            worktree_path,
            session_id: session_id.to_string(),
            terminal_id: terminal_id.clone(),
            kind: "bash".to_string(),
            env: Vec::new(),
        };

        let task =
            tddy_pty::PtyRuntime::spawn(&self.task_registry, &self.pty_registry, spec, ready_tx)
                .await;
        let ready = ready_rx
            .await
            .map_err(|_| anyhow::anyhow!("PTY runtime did not signal ready"))?
            .map_err(|e| anyhow::anyhow!("PTY spawn failed: {e}"))?;

        let channel = task
            .channel("0")
            .ok_or_else(|| anyhow::anyhow!("PTY task missing channel 0"))?;
        let stdin_tx = channel
            .stdin_sender()
            .ok_or_else(|| anyhow::anyhow!("PTY channel missing stdin"))?;

        let (pty_done_tx, pty_done_rx) = watch::channel(false);
        let mut status_rx = task.status_watch();
        tokio::spawn(async move {
            loop {
                if status_rx.borrow().is_terminal() {
                    let _ = pty_done_tx.send(true);
                    break;
                }
                if status_rx.changed().await.is_err() {
                    break;
                }
            }
        });

        let handle = Arc::new(PtyHandle {
            terminal_id: terminal_id.clone(),
            kind: "bash".to_string(),
            pid: ready.pid,
            stdin_tx,
            stdout_tx: channel.output_broadcast(),
            capture: channel.capture_arc(),
            pty_done: pty_done_rx,
            task_id: task.id.clone(),
            pty_registry: self.pty_registry.clone(),
        });

        self.terminals
            .write()
            .await
            .insert(terminal_id.clone(), Arc::clone(&handle));
        self.spawn_terminal_cleanup(terminal_id, task.id.clone());

        log::info!(
            target: "tddy_coder::session_participant",
            "terminal_manager: started session={} terminal={} pid={} task_id={}",
            session_id,
            handle.terminal_id,
            handle.pid,
            handle.task_id
        );

        Ok(handle)
    }

    /// Look up a started terminal by id.
    pub async fn get_terminal(&self, terminal_id: &str) -> Option<Arc<PtyHandle>> {
        self.terminals.read().await.get(terminal_id).cloned()
    }

    /// List all started terminals.
    pub async fn list_terminals(&self) -> Vec<Arc<PtyHandle>> {
        self.terminals.read().await.values().cloned().collect()
    }

    /// Stop a started terminal: cancel its task and remove it. Returns `true` if it existed.
    pub async fn stop_terminal(&self, terminal_id: &str) -> bool {
        let handle = self.terminals.write().await.remove(terminal_id);
        match handle {
            Some(h) => {
                self.task_registry.cancel_task(&h.task_id).await;
                true
            }
            None => false,
        }
    }

    /// Remove the terminal from the index once its backing task reaches a terminal status, so a
    /// shell that exits on its own (e.g. `exit`) does not linger in the listing.
    fn spawn_terminal_cleanup(&self, terminal_id: String, task_id: TaskId) {
        let terminals = Arc::clone(&self.terminals);
        let task_registry = self.task_registry.clone();
        tokio::spawn(async move {
            let task = match task_registry.get(&task_id).await {
                Some(t) => t,
                None => return,
            };
            let mut status_rx = task.status_watch();
            loop {
                if status_rx.borrow().is_terminal() {
                    break;
                }
                if status_rx.changed().await.is_err() {
                    break;
                }
            }
            let mut reg = terminals.write().await;
            if reg.get(&terminal_id).is_some_and(|h| h.task_id == task_id) {
                reg.remove(&terminal_id);
            }
        });
    }
}

/// Resolve the interactive login shell for the coder's own user.
///
/// Prefers the user's passwd `pw_shell`. The daemon / `./web-dev` (nix) environment frequently
/// exports a `$SHELL` pointing at a stripped Nix bash with no programmable completion, so a login
/// shell started from it floods the pane with `complete: command not found` /
/// `shopt: progcomp: invalid shell option name` from the user's completion rc. The passwd entry is
/// the user's real login shell. Falls back to `$SHELL`, then `/bin/bash`.
pub fn resolve_login_shell() -> String {
    #[cfg(unix)]
    let passwd_shell = passwd_login_shell();
    #[cfg(not(unix))]
    let passwd_shell: Option<String> = None;
    pick_shell(
        passwd_shell.as_deref(),
        std::env::var("SHELL").ok().as_deref(),
    )
}

/// Choose a shell: a usable passwd shell wins over `$SHELL` (which may be a Nix bash); `/bin/bash`
/// is the final fallback. Pure so the selection policy is unit-tested without the passwd database.
fn pick_shell(passwd_shell: Option<&str>, env_shell: Option<&str>) -> String {
    if let Some(s) = passwd_shell.map(str::trim).filter(|s| is_usable_shell(s)) {
        return s.to_string();
    }
    if let Some(s) = env_shell.map(str::trim).filter(|s| is_usable_shell(s)) {
        return s.to_string();
    }
    "/bin/bash".to_string()
}

/// A passwd/`$SHELL` value usable as an interactive shell — non-empty and not a login-disabling stub.
fn is_usable_shell(shell: &str) -> bool {
    !shell.is_empty() && !shell.ends_with("/nologin") && !shell.ends_with("/false")
}

/// The current user's login shell from the passwd database (`getpwuid_r(geteuid())`), or `None`
/// when the entry is missing or has no shell.
#[cfg(unix)]
fn passwd_login_shell() -> Option<String> {
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    let uid = unsafe { libc::geteuid() };
    let ret = unsafe {
        libc::getpwuid_r(
            uid,
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
    if shell.is_empty() {
        None
    } else {
        Some(shell)
    }
}

/// Strip an OSC resize sequence (`\x1b]resize;{cols};{rows}\x07`) from `data`.
///
/// Returns `(Some((cols, rows)), remaining)` when found, or `(None, original)` otherwise. The
/// escape sequence is removed from the returned bytes so it is not forwarded to the PTY stdin.
fn strip_resize(data: &[u8]) -> (Option<(u16, u16)>, Bytes) {
    let prefix = b"\x1b]resize;";
    let start = match (0..data.len().saturating_sub(prefix.len()))
        .find(|&i| data[i..].starts_with(prefix))
    {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let after = &data[start + prefix.len()..];
    let bel = match after.iter().position(|&b| b == 0x07) {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let inner = &after[..bel];
    let semi = match inner.iter().position(|&b| b == b';') {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let parsed = std::str::from_utf8(&inner[..semi])
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .zip(
            std::str::from_utf8(&inner[semi + 1..])
                .ok()
                .and_then(|s| s.parse::<u16>().ok()),
        );
    match parsed {
        Some((cols, rows)) => {
            let end = start + prefix.len() + bel + 1;
            let mut remaining = data[..start].to_vec();
            remaining.extend_from_slice(&data[end..]);
            (Some((cols, rows)), Bytes::from(remaining))
        }
        None => (None, Bytes::copy_from_slice(data)),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_usable_shell, pick_shell};

    #[test]
    fn prefers_the_passwd_shell_over_a_nix_env_shell() {
        // Given the passwd login shell is the real /bin/bash and $SHELL is a stripped Nix bash
        let passwd = Some("/bin/bash");
        let env = Some("/nix/store/v8sa6r6q037ihghxfbwzjj4p59v2x0pv-bash-5.3p9/bin/bash");

        // When resolving the shell
        let shell = pick_shell(passwd, env);

        // Then the user's real login shell wins (the Nix bash lacks programmable completion)
        assert_eq!(shell, "/bin/bash");
    }

    #[test]
    fn falls_back_to_env_shell_when_passwd_is_unavailable() {
        // Given no passwd shell but a usable $SHELL
        // When resolving the shell
        // Then $SHELL is used
        assert_eq!(pick_shell(None, Some("/usr/bin/zsh")), "/usr/bin/zsh");
    }

    #[test]
    fn falls_back_to_bin_bash_when_nothing_usable_is_available() {
        // Given a login-disabling passwd shell and no $SHELL
        // When resolving the shell
        // Then the built-in default is used
        assert_eq!(pick_shell(Some("/usr/sbin/nologin"), None), "/bin/bash");
    }

    #[test]
    fn rejects_login_disabling_shells() {
        assert!(!is_usable_shell("/usr/sbin/nologin"));
        assert!(!is_usable_shell("/bin/false"));
        assert!(!is_usable_shell(""));
        assert!(is_usable_shell("/bin/bash"));
    }
}
