//! Where the privileged listener comes from.
//!
//! Socket activation matters here for the same reason it does for the daemon: systemd can create
//! `/run/tddy-supervisor.sock` with the right owner and mode before the service starts, so nothing
//! has to bind in `/run` at runtime. The resolution itself is pure so both branches are testable.

use std::os::fd::RawFd;
use std::path::{Path, PathBuf};

/// First file descriptor systemd passes to an activated service.
pub const SD_LISTEN_FDS_START: RawFd = 3;

/// The most listeners one service is handed: descriptor 3 (its own socket) plus one per mapped user.
/// The supervisor keeps this many descriptors, from 3 up, occupied so nothing it opens can land on a
/// slot a child's listener is about to be copied onto.
pub const MAX_HANDED_OVER_LISTENERS: usize = 16;

/// `LISTEN_FDNAMES` entry of descriptor 3, the service's own socket.
pub const SERVICE_SOCKET_FD_NAME: &str = "connection";

/// Prefix of the `LISTEN_FDNAMES` entry of a mapped OS user's one daemon socket (`#agent-tools`
/// 31/32): `user.<os user>`, the only per-user name the supervisor hands over. Descriptor `3 + i`
/// is the one named by the `i`th entry; names are colon-separated, which is why `users.yaml`
/// refuses an OS user that is not one plain segment. The daemon spells it again
/// (`tddy_daemon_kernel::agent_tool_sockets::USER_SOCKET_FD_NAME_PREFIX`) and a test pins the two.
pub const USER_SOCKET_FD_NAME_PREFIX: &str = "user.";

/// The `LISTEN_FDNAMES` entry of `os_user`'s daemon socket.
pub fn user_socket_fd_name(os_user: &str) -> String {
    format!("{USER_SOCKET_FD_NAME_PREFIX}{os_user}")
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
    fn names_a_users_one_daemon_socket_by_the_generic_prefix() {
        assert_eq!(
            (USER_SOCKET_FD_NAME_PREFIX, user_socket_fd_name("alice")),
            ("user.", "user.alice".to_string())
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
