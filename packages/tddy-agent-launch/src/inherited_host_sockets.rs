//! Host-session sockets the supervisor created and handed this daemon.
//!
//! Under `tddy-supervisor` the daemon is an unprivileged child and cannot `chown` a socket to a
//! session's OS user. So `supervisor.yaml` declares those users (`host_sockets:`), the root
//! supervisor creates each socket — owned by that user, `0600`, in their `0700` directory — and
//! passes the listeners down after the daemon's own listener (descriptor 3), announced in
//! `LISTEN_FDNAMES` as `host-session.<os user>` (see `tddy_supervisor::socket`).
//!
//! **Who can connect.** Only that user, and root: the file is theirs and closed to everybody else.
//! The daemon is neither, and does not need to be — a file's mode is checked when somebody
//! `connect`s to the path, never when the holder of a listener `accept`s on it.
//!
//! **What is adopted.** Only what checks out here, whatever the variables claim: an open, listening
//! unix socket bound to a path, whose file is a socket owned by the very user it is named for and
//! closed to group and others. A descriptor that fails any of that is closed and its user is left
//! to the explicit refusal [`super::host_session_socket::HostSessionSockets`] gives an unlisted one;
//! it is never bound again by this daemon under a different owner.

use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

/// A listener the supervisor created for one OS user, vetted and ready to serve.
#[derive(Debug)]
pub struct InheritedHostSocket {
    pub os_user: String,
    pub listener: UnixListener,
    pub path: PathBuf,
}

/// The host sockets this process was handed, according to its own environment.
///
/// Call it once, before anything else reads or clears the `LISTEN_*` variables — the daemon's own
/// listener adoption removes them.
pub fn adopt_from_environment() -> Vec<InheritedHostSocket> {
    let variable = |name: &str| std::env::var(name).ok();
    adopt_from_variables(
        std::process::id(),
        variable("LISTEN_PID").as_deref(),
        variable("LISTEN_FDS").as_deref(),
        variable("LISTEN_FDNAMES").as_deref(),
    )
}

/// [`adopt_from_environment`] over explicit values of `LISTEN_PID`, `LISTEN_FDS` and `LISTEN_FDNAMES`.
pub fn adopt_from_variables(
    my_pid: u32,
    listen_pid: Option<&str>,
    listen_fds: Option<&str>,
    listen_fdnames: Option<&str>,
) -> Vec<InheritedHostSocket> {
    adopt_descriptors(host_session_fds(
        my_pid,
        listen_pid,
        listen_fds,
        listen_fdnames,
    ))
}

/// Adopt each `(os user, descriptor)` that is that user's host socket; close the rest.
pub fn adopt_descriptors(descriptors: Vec<(String, RawFd)>) -> Vec<InheritedHostSocket> {
    descriptors
        .into_iter()
        .filter_map(|(os_user, fd)| adopt_descriptor(os_user, fd))
        .collect()
}

/// First descriptor of a socket activation; the supervisor places the daemon's own listener there.
const LISTEN_FDS_START: RawFd = 3;

/// Prefix of a host socket's `LISTEN_FDNAMES` entry. The supervisor's contract, spelled again here
/// because the daemon does not link the supervisor; a test pins the two together.
const HOST_SESSION_FD_NAME_PREFIX: &str = "host-session.";

/// The most descriptors a handover carries; more is not the supervisor's doing.
const MAX_LISTEN_FDS: usize = 16;

/// `(os user, descriptor)` for each host socket the variables announce — nothing at all unless they
/// were set for this process and the names account for exactly `LISTEN_FDS` descriptors. The same
/// reading `tddy_supervisor::resolve_host_session_fds` makes, on purpose.
pub fn host_session_fds(
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
            let user = name.strip_prefix(HOST_SESSION_FD_NAME_PREFIX)?;
            (!user.is_empty()).then(|| (user.to_string(), LISTEN_FDS_START + index as RawFd))
        })
        .collect()
}

/// Take ownership of `fd` as `os_user`'s host socket, if it is one. Closed otherwise.
fn adopt_descriptor(os_user: String, fd: RawFd) -> Option<InheritedHostSocket> {
    // SAFETY: only reads the descriptor's flags; `EBADF` says it is not open, in which case there is
    // nothing to own and nothing may be closed.
    if unsafe { libc::fcntl(fd, libc::F_GETFD) } < 0 {
        log::error!(
            target: "tddy_daemon::host_session_socket",
            "descriptor {fd} announced as {os_user}'s host-session socket is not open; ignoring it"
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
            target: "tddy_daemon::host_session_socket",
            "{os_user}'s host-session socket (descriptor {fd}): cannot set close-on-exec: {}",
            std::io::Error::last_os_error()
        );
        return None;
    }
    let listener = UnixListener::from(owned);
    match vet(&os_user, &listener) {
        Ok(path) => Some(InheritedHostSocket {
            os_user,
            listener,
            path,
        }),
        Err(why) => {
            log::error!(
                target: "tddy_daemon::host_session_socket",
                "not serving the inherited descriptor {fd} as {os_user}'s host-session socket: {why}"
            );
            None
        }
    }
}

/// The path `listener` is bound at, once it is shown to be `os_user`'s and nobody else's.
fn vet(os_user: &str, listener: &UnixListener) -> Result<PathBuf, String> {
    require_listening(listener)?;
    let address = listener
        .local_addr()
        .map_err(|e| format!("it is not a bound unix socket: {e}"))?;
    let path = address
        .as_pathname()
        .ok_or("it is not bound to a path")?
        .to_path_buf();
    let target = tddy_daemon_kernel::privilege_drop::resolve_pty_os_user(os_user)?;
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
fn require_listening(listener: &UnixListener) -> Result<(), String> {
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
fn require_listening(_listener: &UnixListener) -> Result<(), String> {
    Ok(())
}
