//! Sockets bound for one OS user and owner-only, and the ones `tddy-supervisor` binds for it and
//! hands over — the machinery the per-OS-user **host-session** socket and the per-OS-user
//! **agent-tool** socket share.
//!
//! **Binding** ([`bind_owner_only`]). The directory is `0o700` and the socket `0o600`, both owned
//! by the OS user the sessions run as — a `chown` this process is not privileged to make **refuses
//! the bind** rather than widening anything. There is never a window with wider permissions: the
//! directory is narrowed to `0o700` *before* the socket is created inside it, so whatever mode the
//! socket is born with, nobody else can reach it; ownership moves last, directory after socket. A
//! stale file a dead daemon left is replaced only after checking it is a socket, ours (or the target
//! user's), and that nothing listens on it; a socket something *is* listening on is never taken over.
//!
//! **Adopting** ([`adopt_descriptor`]). Under `tddy-supervisor` the daemon is an unprivileged child
//! and cannot make that `chown`, so the root supervisor creates the socket and passes the listener
//! down, announced in `LISTEN_FDNAMES` as `<prefix><os user>` ([`descriptors_named`]). Only what
//! checks out here is adopted, whatever the variables claim: an open, listening unix socket bound to
//! a path, whose file is a socket owned by the very user it is named for and closed to group and
//! others. Anything else is closed and its user left to the refusal [`OwnerSocketKind`] names.
//!
//! **Who can connect.** Only that user, and root. A file's mode is checked when somebody `connect`s
//! to the path, never when the holder of a listener `accept`s on it, so the daemon is neither and
//! does not need to be.

use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};

/// Which kind of per-OS-user socket is being bound or adopted: how a refusal names it and the
/// `supervisor.yaml` setting that would have had the supervisor create it instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerSocketKind {
    /// The service's `supervisor.yaml` key listing the users the supervisor creates one for.
    pub setting: &'static str,
    /// The socket, as a refusal names it.
    pub noun: &'static str,
}

impl OwnerSocketKind {
    /// The host-session socket (`host_sockets:`).
    pub const HOST_SESSION: OwnerSocketKind = OwnerSocketKind {
        setting: "host_sockets",
        noun: "host-session socket",
    };
    /// The agent-tool socket (`agent_tool_sockets:`).
    pub const AGENT_TOOL: OwnerSocketKind = OwnerSocketKind {
        setting: "agent_tool_sockets",
        noun: "agent-tool socket",
    };
}

/// First descriptor of a socket activation; the supervisor places the service's own listener there.
const LISTEN_FDS_START: RawFd = 3;

/// The most descriptors a handover carries; more is not the supervisor's doing.
const MAX_LISTEN_FDS: usize = 16;

/// `(os user, descriptor)` for each descriptor named `<prefix><os user>` — nothing at all unless the
/// variables were set for this process (`LISTEN_PID`) and the names account for exactly
/// `LISTEN_FDS` descriptors. The same reading `tddy_supervisor::resolve_host_session_fds` makes, on
/// purpose.
pub fn descriptors_named(
    prefix: &str,
    my_pid: u32,
    listen_pid: Option<&str>,
    listen_fds: Option<&str>,
    listen_fdnames: Option<&str>,
) -> Vec<(String, RawFd)> {
    let ours = listen_pid.and_then(|pid| pid.parse::<u32>().ok()) == Some(my_pid);
    let count = listen_fds.and_then(|count| count.parse::<usize>().ok());
    let (true, Some(count), Some(names)) = (ours, count, listen_fdnames) else {
        return Vec::new();
    };
    let names: Vec<&str> = names.split(':').collect();
    if names.len() != count || count > MAX_LISTEN_FDS {
        return Vec::new();
    }
    names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            let user = name.strip_prefix(prefix)?;
            (!user.is_empty()).then(|| (user.to_string(), LISTEN_FDS_START + index as RawFd))
        })
        .collect()
}

/// Bind `path` for `os_user` with no window in which anyone else can reach it. See the module docs.
pub fn bind_owner_only(
    path: &Path,
    os_user: &str,
    kind: OwnerSocketKind,
) -> anyhow::Result<UnixListener> {
    let target = crate::privilege_drop::resolve_pty_os_user(os_user).map_err(anyhow::Error::msg)?;
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
    let listener = match UnixListener::bind(path) {
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
                     cannot chown. Under the supervisor, list {os_user} under `{}:` for \
                     this daemon's service in supervisor.yaml (and in \
                     spawn_policy.allowed_session_users): the supervisor then creates their socket \
                     and hands it over. Until then a session of that user gets no {}",
                    target.uid, kind.setting, kind.noun
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
            "refusing to serve {os_user}'s {} with wider access than theirs",
            kind.noun
        )));
    }
    Ok(listener)
}

/// Remove what a failed bind created, so a refusal leaves nothing a later start could trust.
pub fn undo(
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
pub fn ensure_dir(
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
pub fn clear_stale_socket(path: &Path, allowed_owners: &[u32]) -> anyhow::Result<()> {
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

pub fn chown(path: &Path, uid: u32, gid: u32) -> anyhow::Result<()> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .context("path is not a C string")?;
    // SAFETY: `c_path` outlives the call.
    if unsafe { libc::chown(c_path.as_ptr(), uid, gid) } != 0 {
        return Err(anyhow::Error::new(std::io::Error::last_os_error())
            .context(format!("give {} to uid {uid}", path.display())));
    }
    Ok(())
}

/// Take ownership of `fd` as `os_user`'s socket of `kind`, if it is one. Closed otherwise.
pub fn adopt_descriptor(
    os_user: &str,
    fd: RawFd,
    kind: OwnerSocketKind,
) -> Option<(UnixListener, PathBuf)> {
    let noun = kind.noun;
    // SAFETY: only reads the descriptor's flags; `EBADF` says it is not open, in which case there is
    // nothing to own and nothing may be closed.
    if unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
        log::error!(
            target: "tddy_daemon_kernel::owner_socket",
            "descriptor {fd} announced as {os_user}'s {noun} is not open; ignoring it"
        );
        return None;
    }
    // SAFETY: the descriptor is open, and the supervisor handed it to this process alone (the caller
    // matched `LISTEN_PID` to this pid), so nothing else will close it.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    // Not for the daemon's own children: a tool session must not inherit a listener that is not its.
    // SAFETY: setting a flag on a descriptor this function owns.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        log::error!(
            target: "tddy_daemon_kernel::owner_socket",
            "{os_user}'s {noun} (descriptor {fd}): cannot set close-on-exec: {}",
            std::io::Error::last_os_error()
        );
        return None;
    }
    let listener = UnixListener::from(owned);
    match vet(os_user, &listener) {
        Ok(path) => Some((listener, path)),
        Err(why) => {
            log::error!(
                target: "tddy_daemon_kernel::owner_socket",
                "not serving the inherited descriptor {fd} as {os_user}'s {noun}: {why}"
            );
            None
        }
    }
}

/// The path `listener` is bound at, once it is shown to be `os_user`'s and nobody else's.
pub fn vet(os_user: &str, listener: &UnixListener) -> Result<PathBuf, String> {
    require_listening(listener)?;
    let address = listener
        .local_addr()
        .map_err(|e| format!("it is not a bound unix socket: {e}"))?;
    let path = address
        .as_pathname()
        .ok_or("it is not bound to a path")?
        .to_path_buf();
    let target = crate::privilege_drop::resolve_pty_os_user(os_user)?;
    let file = std::fs::symlink_metadata(&path)
        .map_err(|e| format!("cannot stat its path {}: {e}", path.display()))?;
    if !file.file_type().is_socket() {
        return Err(format!("{} is not a socket", path.display()));
    }
    if file.uid() != target.uid {
        return Err(format!(
            "{} is owned by uid {}, not by {os_user} (uid {})",
            path.display(),
            file.uid(),
            target.uid
        ));
    }
    if file.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "{} is accessible to more than its owner (mode {:o})",
            path.display(),
            file.permissions().mode() & 0o7777
        ));
    }
    Ok(path)
}

/// Refuse a socket that is not listening. Asked of the kernel with `SO_ACCEPTCONN`, which Linux
/// answers for unix sockets and Darwin does not (`ENOPROTOOPT`): there the owner and mode checks
/// below carry the vetting, and a socket that does not listen ends the server that tries to accept
/// on it, which is reported when the next session asks for it.
#[cfg(target_os = "linux")]
pub fn require_listening(listener: &UnixListener) -> Result<(), String> {
    let mut accepting: libc::c_int = 0;
    let mut length = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: `accepting` and `length` are live locals of the sizes passed.
    let read = unsafe {
        libc::getsockopt(
            std::os::fd::AsRawFd::as_raw_fd(listener),
            libc::SOL_SOCKET,
            libc::SO_ACCEPTCONN,
            (&mut accepting as *mut libc::c_int).cast(),
            &mut length,
        )
    };
    if read != 0 || accepting == 0 {
        return Err("it is not a listening socket".to_string());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn require_listening(_listener: &UnixListener) -> Result<(), String> {
    Ok(())
}
