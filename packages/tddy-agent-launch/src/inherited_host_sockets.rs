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

use std::os::fd::RawFd;
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

/// Prefix of a host socket's `LISTEN_FDNAMES` entry. The supervisor's contract, spelled again here
/// because the daemon does not link the supervisor; a test pins the two together.
const HOST_SESSION_FD_NAME_PREFIX: &str = "host-session.";

/// `(os user, descriptor)` for each host socket the variables announce — nothing at all unless they
/// were set for this process and the names account for exactly `LISTEN_FDS` descriptors. The same
/// reading `tddy_supervisor::resolve_host_session_fds` makes, on purpose.
pub fn host_session_fds(
    my_pid: u32,
    listen_pid: Option<&str>,
    listen_fds: Option<&str>,
    listen_fdnames: Option<&str>,
) -> Vec<(String, RawFd)> {
    tddy_daemon_kernel::owner_socket::descriptors_named(
        HOST_SESSION_FD_NAME_PREFIX,
        my_pid,
        listen_pid,
        listen_fds,
        listen_fdnames,
    )
}

/// Take ownership of `fd` as `os_user`'s host socket, if it is one. Closed otherwise.
fn adopt_descriptor(os_user: String, fd: RawFd) -> Option<InheritedHostSocket> {
    let (listener, path) = tddy_daemon_kernel::owner_socket::adopt_descriptor(
        &os_user,
        fd,
        tddy_daemon_kernel::owner_socket::OwnerSocketKind::HOST_SESSION,
    )?;
    Some(InheritedHostSocket {
        os_user,
        listener,
        path,
    })
}
