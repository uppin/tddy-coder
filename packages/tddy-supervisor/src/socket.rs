//! Where the privileged listener comes from.
//!
//! Socket activation matters here for the same reason it does for the daemon: systemd can create
//! `/run/tddy-supervisor.sock` with the right owner and mode before the service starts, so nothing
//! has to bind in `/run` at runtime. The resolution itself is pure so both branches are testable.

use std::os::fd::RawFd;
use std::path::{Path, PathBuf};

/// First file descriptor systemd passes to an activated service.
pub const SD_LISTEN_FDS_START: RawFd = 3;

/// The most listeners one service is handed: descriptor 3 (its own socket) plus its host sockets.
/// The supervisor keeps this many descriptors, from 3 up, occupied so nothing it opens can land on a
/// slot a child's listener is about to be copied onto.
pub const MAX_HANDED_OVER_LISTENERS: usize = 16;

/// `LISTEN_FDNAMES` entry of descriptor 3, the service's own socket.
pub const SERVICE_SOCKET_FD_NAME: &str = "connection";

/// Prefix of the `LISTEN_FDNAMES` entry of a per-OS-user host socket: `host-session.<os user>`.
/// Descriptor `3 + i` is the one named by the `i`th entry; names are colon-separated, which is why a
/// user name that is not one plain segment is refused in the config.
pub const HOST_SESSION_FD_NAME_PREFIX: &str = "host-session.";

/// The `LISTEN_FDNAMES` entry of `os_user`'s host socket.
pub fn host_session_fd_name(os_user: &str) -> String {
    format!("{HOST_SESSION_FD_NAME_PREFIX}{os_user}")
}

/// The per-OS-user host sockets a service was handed, as `(os user, descriptor)`.
///
/// Read from the same `LISTEN_PID` / `LISTEN_FDS` / `LISTEN_FDNAMES` the service's own listener is
/// announced with. Anything that does not hold together yields no sockets rather than a guess: the
/// variables belong to another process (`LISTEN_PID`), or the names do not account for exactly
/// `LISTEN_FDS` descriptors. A descriptor adopted for the wrong user would serve one user's sessions
/// on another's socket, so a mismatch has to mean "none".
pub fn resolve_host_session_fds(
    my_pid: u32,
    listen_pid: Option<&str>,
    listen_fds: Option<&str>,
    listen_fdnames: Option<&str>,
) -> Vec<(String, RawFd)> {
    let handed_to_us = listen_pid.and_then(|pid| pid.parse::<u32>().ok()) == Some(my_pid);
    let count = listen_fds.and_then(|count| count.parse::<usize>().ok());
    let (true, Some(count), Some(names)) = (handed_to_us, count, listen_fdnames) else {
        return Vec::new();
    };
    let names: Vec<&str> = names.split(':').collect();
    if names.len() != count || count > MAX_HANDED_OVER_LISTENERS {
        return Vec::new();
    }
    names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            let user = name.strip_prefix(HOST_SESSION_FD_NAME_PREFIX)?;
            (!user.is_empty()).then(|| (user.to_string(), SD_LISTEN_FDS_START + index as RawFd))
        })
        .collect()
}

/// Where the supervisor's listening socket comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketSource {
    /// Adopt a listener systemd already created and handed over.
    Activated(RawFd),
    /// Bind this path ourselves.
    SelfBind(PathBuf),
}

/// Decide whether to adopt an inherited listener or bind `fallback`.
///
/// `listen_pid` and `listen_fds` are the raw `LISTEN_PID` / `LISTEN_FDS` values. Checking
/// `LISTEN_PID` against our own pid is not paranoia: the variables are inherited by children, so a
/// process that is *not* the activated service can see them and would otherwise adopt fd 3 —
/// whatever fd 3 happens to be for it.
pub fn resolve_socket_source(
    my_pid: u32,
    listen_pid: Option<&str>,
    listen_fds: Option<&str>,
    fallback: &Path,
) -> SocketSource {
    let handed_to_us = listen_pid.and_then(|pid| pid.parse::<u32>().ok()) == Some(my_pid);
    // A value we cannot read is not an activation. Assuming one listener from an unparseable count
    // would mean adopting whatever fd 3 happens to be.
    let listeners = listen_fds
        .and_then(|count| count.parse::<u32>().ok())
        .filter(|count| *count >= 1);

    match (handed_to_us, listeners) {
        (true, Some(_)) => SocketSource::Activated(SD_LISTEN_FDS_START),
        _ => SocketSource::SelfBind(fallback.to_path_buf()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MY_PID: u32 = 4242;

    fn the_configured_path() -> PathBuf {
        PathBuf::from("/run/tddy-supervisor.sock")
    }

    #[test]
    fn adopts_the_listener_systemd_handed_to_this_process() {
        // Given
        let source = resolve_socket_source(MY_PID, Some("4242"), Some("1"), &the_configured_path());

        // Then
        assert_eq!(source, SocketSource::Activated(SD_LISTEN_FDS_START));
    }

    #[test]
    fn binds_the_configured_path_when_systemd_handed_the_listener_to_another_process() {
        // Given the LISTEN_* variables we inherited belong to our parent, not to us.
        let source = resolve_socket_source(MY_PID, Some("1"), Some("1"), &the_configured_path());

        // Then
        assert_eq!(source, SocketSource::SelfBind(the_configured_path()));
    }

    #[test]
    fn binds_the_configured_path_when_no_listener_was_handed_over() {
        // Given
        let source = resolve_socket_source(MY_PID, None, None, &the_configured_path());

        // Then
        assert_eq!(source, SocketSource::SelfBind(the_configured_path()));
    }

    #[test]
    fn names_the_descriptor_of_every_host_socket_a_service_was_handed() {
        // Given a service socket at fd 3 and two host sockets after it
        let names = "connection:host-session.alice:host-session.bob";

        // When
        let sockets = resolve_host_session_fds(MY_PID, Some("4242"), Some("3"), Some(names));

        // Then
        assert_eq!(
            sockets,
            vec![("alice".to_string(), 4), ("bob".to_string(), 5)]
        );
    }

    #[test]
    fn finds_no_host_sockets_when_the_variables_describe_another_process() {
        // Given a LISTEN_PID that is the parent's, not ours
        let names = "connection:host-session.alice";

        // Then
        assert_eq!(
            resolve_host_session_fds(MY_PID, Some("1"), Some("2"), Some(names)),
            Vec::new()
        );
    }

    #[test]
    fn finds_no_host_sockets_when_the_names_do_not_account_for_every_descriptor() {
        // Given three descriptors announced but two names
        let names = "connection:host-session.alice";

        // Then — which descriptor belongs to whom is unknowable, so none is adopted.
        assert_eq!(
            resolve_host_session_fds(MY_PID, Some("4242"), Some("3"), Some(names)),
            Vec::new()
        );
    }

    #[test]
    fn finds_no_host_sockets_when_the_descriptors_are_not_named() {
        assert_eq!(
            resolve_host_session_fds(MY_PID, Some("4242"), Some("1"), None),
            Vec::new()
        );
    }

    #[test]
    fn ignores_a_descriptor_that_is_not_a_host_socket() {
        // Given only the service's own socket
        assert_eq!(
            resolve_host_session_fds(MY_PID, Some("4242"), Some("1"), Some("connection")),
            Vec::new()
        );
    }

    #[test]
    fn binds_the_configured_path_when_systemd_reports_zero_listeners() {
        // Given
        let source = resolve_socket_source(MY_PID, Some("4242"), Some("0"), &the_configured_path());

        // Then
        assert_eq!(source, SocketSource::SelfBind(the_configured_path()));
    }

    #[test]
    fn binds_the_configured_path_when_the_listener_count_is_not_a_number() {
        // Given
        let source =
            resolve_socket_source(MY_PID, Some("4242"), Some("many"), &the_configured_path());

        // Then — an unparseable count is treated as no activation rather than assumed to be one.
        assert_eq!(source, SocketSource::SelfBind(the_configured_path()));
    }

    #[test]
    fn binds_the_configured_path_when_the_listen_pid_is_not_a_number() {
        // Given
        let source = resolve_socket_source(MY_PID, Some("me"), Some("1"), &the_configured_path());

        // Then
        assert_eq!(source, SocketSource::SelfBind(the_configured_path()));
    }
}
