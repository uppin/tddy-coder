//! The host-session socket: one long-lived unix socket **per OS user**, bound by the daemon, over
//! which that user's `tddy-coder` tool sessions reach the daemon (`spawn_conversation`,
//! `github_token`).
//!
//! **Where.** `<data dir>/run/<os user>/host.sock`. The data dir is the daemon's configured one
//! (the base `sessions_base_for_user` resolves), not a temp dir. Nested under a *session's* own
//! directory it would overflow `AF_UNIX`'s ~104-byte path limit on a real checkout path — the same
//! overflow `agent_tool_socket_path` was digested to avoid — so the per-user directory is the
//! deepest this can safely go, and a path that still does not fit is refused with that reason.
//!
//! **Who can reach it.** The directory is `0o700` and the socket `0o600`, both owned by the OS user
//! the sessions run as — a `chown` this process is not privileged to make **refuses the bind**
//! rather than widening anything (the rule `write_agent_def_file` follows for the agent def). There
//! is never a window with wider permissions: the directory is narrowed to `0o700` *before* the
//! socket is created inside it, so whatever mode the socket is born with, nobody else can reach it;
//! ownership moves last, directory after socket.
//!
//! **Under `tddy-supervisor`** the daemon is unprivileged and cannot make that `chown`. A user listed
//! under `host_sockets:` in `supervisor.yaml` instead has their socket created by the root supervisor
//! and handed to the daemon as an inherited listener (see [`super::inherited_host_sockets`]): the
//! daemon serves it where it is, and never binds, replaces or removes it. A user who is not listed
//! gets the refusal below, which names the setting to add.
//!
//! **Lifetime.** Bound lazily by the first session that needs it, kept for the daemon's lifetime and
//! bound again by the next session after a restart. A stale file a dead daemon left is replaced only
//! after checking it is a socket, ours (or the target user's), and that nothing listens on it; a
//! socket something *is* listening on is never taken over.
//!
//! **Authentication.** The socket's filesystem permissions. Every request names its session, and
//! [`HostSessionService`] refuses an unknown session or one registered for a different OS user.

use std::collections::HashMap;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context};
use tddy_host_service::host_session_service::{
    HostSessionRegistry, HostSessionService, SessionProbe,
};
use tokio::task::JoinHandle;

use crate::inherited_host_sockets::InheritedHostSocket;

/// Longest socket path accepted. `sun_path` is 104 bytes on macOS and 108 on Linux, NUL included.
const MAX_SOCKET_PATH_BYTES: usize = 100;

/// Directory (under the data dir) holding one subdirectory per OS user.
const RUN_DIR: &str = "run";
const SOCKET_FILE: &str = "host.sock";

/// `<data dir>/run/<os user>/host.sock`, or why there can be none.
pub(crate) fn host_session_socket_path(data_dir: &Path, os_user: &str) -> anyhow::Result<PathBuf> {
    let name_is_one_safe_segment = !os_user.is_empty()
        && !os_user.starts_with('.')
        && os_user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !name_is_one_safe_segment {
        bail!("OS user name {os_user:?} cannot name a socket directory");
    }
    let path = data_dir.join(RUN_DIR).join(os_user).join(SOCKET_FILE);
    if path.as_os_str().len() > MAX_SOCKET_PATH_BYTES {
        bail!(
            "host-session socket path {} is too long for a unix socket ({} > {MAX_SOCKET_PATH_BYTES} \
             bytes); use a shorter data dir",
            path.display(),
            path.as_os_str().len()
        );
    }
    Ok(path)
}

struct BoundServer {
    path: PathBuf,
    /// `(dev, ino)` of the socket this server bound, to notice the file was replaced or removed.
    identity: (u64, u64),
    task: JoinHandle<()>,
}

impl Drop for BoundServer {
    /// A server outlives nothing: dropping the daemon's sockets closes the listener, so a daemon that
    /// starts over in the same process (or a test that does) finds no live peer on the path.
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// How often a registered session's process is looked at to see whether it is still running.
const DEFAULT_STOP_WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// The daemon's per-OS-user host-session socket servers and the sessions they answer.
pub struct HostSessionSockets {
    registry: Arc<HostSessionRegistry>,
    servers: tokio::sync::Mutex<HashMap<String, BoundServer>>,
    /// Sockets the supervisor created, by OS user: `Some` until first served, `None` after — the
    /// key stays so a user the supervisor serves is never bound by this daemon itself.
    inherited: std::sync::Mutex<HashMap<String, Option<InheritedHostSocket>>>,
    stop_watch_interval: std::time::Duration,
}

impl Default for HostSessionSockets {
    fn default() -> Self {
        Self::with_stop_watch_interval(DEFAULT_STOP_WATCH_INTERVAL)
    }
}

impl HostSessionSockets {
    /// Sockets whose registered sessions' processes are looked at every `stop_watch_interval`.
    pub fn with_stop_watch_interval(stop_watch_interval: std::time::Duration) -> Self {
        Self {
            registry: Arc::default(),
            servers: tokio::sync::Mutex::default(),
            inherited: std::sync::Mutex::default(),
            stop_watch_interval,
        }
    }

    /// Stop answering `session_id` once `pid` — the process it was just started or resumed as — is
    /// gone, so the daemon stops holding the start's session token for a session that is not running.
    ///
    /// The registered session records `pid`; a later registration of the same id (a resume) replaces
    /// it, and then this watch ends without touching the newer registration. The watch also ends when
    /// the session is deleted. The process's absence is read with `kill(pid, 0)`: `ESRCH` is gone,
    /// anything else (including `EPERM`, a process of another user) is still there.
    pub(crate) fn watch_until_stopped(&self, session_id: &str, pid: u32) {
        if !self.registry.attach_process(session_id, pid) {
            return;
        }
        let registry = Arc::clone(&self.registry);
        let interval = self.stop_watch_interval;
        let session_id = session_id.to_string();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                if registry.process_of(&session_id) != Some(pid) {
                    return;
                }
                if !process_exists(pid) {
                    if registry.unregister_stopped(&session_id, pid) {
                        log::info!(
                            target: "tddy_daemon::connection_service",
                            "session {session_id}: its process {pid} has stopped; no longer answered \
                             on its host-session socket until resumed"
                        );
                    }
                    return;
                }
            }
        });
    }

    /// Serve these supervisor-created sockets for their users from now on (served on first use).
    pub fn adopt_inherited(&self, sockets: Vec<InheritedHostSocket>) {
        let mut inherited = self.inherited.lock().expect("inherited sockets lock");
        for socket in sockets {
            inherited.insert(socket.os_user.clone(), Some(socket));
        }
    }

    /// The sessions answered on these sockets: registered at a session's start or resume, removed
    /// when it is deleted.
    pub fn registry(&self) -> &Arc<HostSessionRegistry> {
        &self.registry
    }

    /// The path of `os_user`'s socket, binding and serving it first if this daemon is not already.
    pub async fn ensure_bound(&self, os_user: &str, data_dir: &Path) -> anyhow::Result<PathBuf> {
        let mut servers = self.servers.lock().await;
        if let Some(server) = servers.get(os_user) {
            let intact = !server.task.is_finished()
                && std::fs::symlink_metadata(&server.path)
                    .map(|m| (m.dev(), m.ino()) == server.identity)
                    .unwrap_or(false);
            if intact {
                return Ok(server.path.clone());
            }
        }
        // A server whose file is gone or replaced listens on nothing anyone can reach; stop it so
        // the probe below does not mistake it for a live peer.
        if let Some(mut stale) = servers.remove(os_user) {
            stale.task.abort();
            let _ = (&mut stale.task).await;
        }
        let inherited = {
            let mut inherited = self.inherited.lock().expect("inherited sockets lock");
            inherited.get_mut(os_user).map(Option::take)
        };
        let (path, listener) = match inherited {
            Some(Some(socket)) => {
                socket
                    .listener
                    .set_nonblocking(true)
                    .context("make the inherited socket non-blocking")?;
                (socket.path, socket.listener)
            }
            // The supervisor made this socket and only it can make another.
            Some(None) => bail!(
                "the host-session socket tddy-supervisor created for {os_user} is gone or no longer                  accepting; it can only be recreated by restarting tddy-supervisor"
            ),
            None => {
                let path = host_session_socket_path(data_dir, os_user)?;
                let listener = bind_owner_only(&path, os_user)?;
                (path, listener)
            }
        };
        let meta = std::fs::symlink_metadata(&path).context("stat the bound socket")?;
        let listener = tokio::net::UnixListener::from_std(listener)
            .context("hand the socket to the runtime")?;
        let registry = Arc::clone(&self.registry);
        let owner = os_user.to_string();
        let exists = session_probe(data_dir);
        let task = tokio::spawn(serve(listener, owner, registry, exists));
        servers.insert(
            os_user.to_string(),
            BoundServer {
                path: path.clone(),
                identity: (meta.dev(), meta.ino()),
                task,
            },
        );
        log::info!(
            target: "tddy_daemon::connection_service",
            "host-session socket for {os_user} listening at {}",
            path.display()
        );
        Ok(path)
    }
}

/// Accept every connection for the daemon's lifetime; each gets its own endpoint over the service
/// for this socket's owner.
async fn serve(
    listener: tokio::net::UnixListener,
    owner: String,
    registry: Arc<HostSessionRegistry>,
    exists: SessionProbe,
) {
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "host-session socket for {owner}: accept failed, it will be bound again by the \
                     next session that needs it: {e}"
                );
                return;
            }
        };
        let service = HostSessionService::new(&owner, Arc::clone(&registry))
            .with_session_probe(Arc::clone(&exists));
        let (reader, writer) = tokio::io::split(stream);
        let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
            reader,
            writer,
            service,
            tddy_rpc::RequestTransport::UnixSocket,
        );
        tokio::spawn(endpoint.run());
    }
}

/// Whether a process with `pid` still exists.
fn process_exists(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the process can be signalled.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// Whether a session exists on disk — durable state under the daemon's data dir — for the OS user a
/// socket belongs to. The id is validated as one path segment first: it comes from a request.
fn session_probe(data_dir: &Path) -> SessionProbe {
    let data_dir = data_dir.to_path_buf();
    Arc::new(move |os_user, session_id| {
        if tddy_core::session_lifecycle::validate_session_id_segment(session_id).is_err() {
            return false;
        }
        let Some(base) =
            tddy_daemon_kernel::user_paths::sessions_base_for_user(os_user, Some(&data_dir))
        else {
            return false;
        };
        tddy_core::read_session_metadata(&tddy_core::session_lifecycle::unified_session_dir_path(
            &base, session_id,
        ))
        .is_ok()
    })
}

/// Bind `path` for `os_user` with no window in which anyone else can reach it. See the module docs.
fn bind_owner_only(path: &Path, os_user: &str) -> anyhow::Result<std::os::unix::net::UnixListener> {
    let target = tddy_daemon_kernel::privilege_drop::resolve_pty_os_user(os_user)
        .map_err(anyhow::Error::msg)?;
    let euid = unsafe { libc::geteuid() };
    let needs_chown = target.uid != euid;
    let user_dir = path.parent().context("socket path has no directory")?;
    let run_dir = user_dir
        .parent()
        .context("socket path has no run directory")?;
    let data_dir = run_dir
        .parent()
        .context("socket path has no data directory")?;
    if !data_dir.is_dir() {
        bail!("data dir {} does not exist", data_dir.display());
    }

    // Traversable but not listable by others: the user reaches its own subdirectory through it.
    let created_run = ensure_dir(run_dir, 0o711, &[euid, target.uid], false)?;
    let created_user = ensure_dir(user_dir, 0o700, &[euid, target.uid], true)?;
    // Only now, with the directory closed to everyone else, may anything exist inside it.
    if let Err(e) = clear_stale_socket(path, &[euid, target.uid]) {
        undo(path, user_dir, run_dir, created_user, created_run, false);
        return Err(e);
    }
    let listener = match std::os::unix::net::UnixListener::bind(path) {
        Ok(l) => l,
        Err(e) => {
            undo(path, user_dir, run_dir, created_user, created_run, false);
            return Err(anyhow::Error::new(e).context(format!("bind {}", path.display())));
        }
    };
    let finish = || -> anyhow::Result<()> {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .context("restrict the socket to its owner")?;
        if needs_chown {
            let not_privileged = || {
                format!(
                    "this daemon (uid {euid}) is not privileged to hand the socket to {os_user} \
                     (uid {}) — as when it runs as tddy-supervisor's unprivileged child, which \
                     cannot chown. Under the supervisor, list {os_user} under `host_sockets:` for \
                     this daemon's service in supervisor.yaml (and in \
                     spawn_policy.allowed_session_users): the supervisor then creates their socket \
                     and hands it over. Until then a session of that user gets no host-session \
                     socket",
                    target.uid
                )
            };
            chown(path, target.uid, target.gid).with_context(not_privileged)?;
            chown(user_dir, target.uid, target.gid).with_context(not_privileged)?;
        }
        listener
            .set_nonblocking(true)
            .context("make the socket non-blocking")
    };
    if let Err(e) = finish() {
        undo(path, user_dir, run_dir, created_user, created_run, true);
        return Err(e.context(format!(
            "refusing to serve {os_user}'s host-session socket with wider access than theirs"
        )));
    }
    Ok(listener)
}

/// Remove what a failed bind created, so a refusal leaves nothing a later start could trust.
fn undo(
    path: &Path,
    user_dir: &Path,
    run_dir: &Path,
    created_user: bool,
    created_run: bool,
    bound: bool,
) {
    if bound {
        let _ = std::fs::remove_file(path);
    }
    if created_user {
        let _ = std::fs::remove_dir(user_dir);
    }
    if created_run {
        let _ = std::fs::remove_dir(run_dir);
    }
}

/// Make `dir` exist as a real directory with exactly `mode`, narrowing an existing one **before**
/// the caller puts anything in it. `Ok(true)` when this call created it.
fn ensure_dir(
    dir: &Path,
    mode: u32,
    allowed_owners: &[u32],
    must_narrow: bool,
) -> anyhow::Result<bool> {
    let created = match std::fs::symlink_metadata(dir) {
        Ok(meta) => {
            if !meta.is_dir() {
                bail!("{} exists and is not a directory", dir.display());
            }
            if !allowed_owners.contains(&meta.uid()) {
                bail!(
                    "{} is owned by uid {}, which is neither this daemon nor the session's user",
                    dir.display(),
                    meta.uid()
                );
            }
            false
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Born with at most `mode` (the umask can only remove bits), never wider.
            std::fs::DirBuilder::new()
                .mode(mode)
                .create(dir)
                .with_context(|| format!("create {}", dir.display()))?;
            true
        }
        Err(e) => return Err(anyhow::Error::new(e).context(format!("stat {}", dir.display()))),
    };
    // The umask may have removed bits the user needs (`x` on the run dir); an old directory may
    // carry more than it should. Both are set explicitly rather than trusted.
    let current = std::fs::metadata(dir)?.permissions().mode() & 0o7777;
    if current != mode || must_narrow {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode))
            .with_context(|| format!("set {} to {mode:o}", dir.display()))?;
    }
    Ok(created)
}

/// Remove a socket file a dead daemon left — and only that: it must be a socket, owned by us or the
/// target user, with nothing listening on it.
fn clear_stale_socket(path: &Path, allowed_owners: &[u32]) -> anyhow::Result<()> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(anyhow::Error::new(e).context(format!("stat {}", path.display()))),
    };
    if !meta.file_type().is_socket() {
        bail!(
            "{} exists and is not a socket; not removing it",
            path.display()
        );
    }
    if !allowed_owners.contains(&meta.uid()) {
        bail!(
            "{} is a socket owned by uid {}; not removing it",
            path.display(),
            meta.uid()
        );
    }
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(_) => bail!(
            "something is already listening on {}; not taking it over",
            path.display()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
            std::fs::remove_file(path).with_context(|| format!("remove stale {}", path.display()))
        }
        Err(e) => Err(anyhow::Error::new(e).context(format!(
            "cannot tell whether {} is live; not removing it",
            path.display()
        ))),
    }
}

fn chown(path: &Path, uid: u32, gid: u32) -> anyhow::Result<()> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .context("path is not a C string")?;
    // SAFETY: `c_path` outlives the call.
    if unsafe { libc::chown(c_path.as_ptr(), uid, gid) } != 0 {
        return Err(anyhow::Error::new(std::io::Error::last_os_error())
            .context(format!("give {} to uid {uid}", path.display())));
    }
    Ok(())
}
