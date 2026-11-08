//! One daemon socket per mapped OS user (`#agent-tools` 31/32), derived from `users.yaml`.
//!
//! It replaces the two hand-kept lists `services[].host_sockets` and
//! `services[].agent_tool_sockets`: the daemon serves its host-session services and its agent-tool
//! roster on the one socket, so each user costs one descriptor and the path is never declared — it
//! is `<users.sockets_dir>/<os user>/daemon.sock`, announced as `user.<os user>`
//! ([`crate::socket::user_socket_fd_name`]).
//!
//! The socket is created exactly as a host socket was ([`crate::host_socket`]): owned by that user,
//! directory `0700`, socket `0600`, every directory above it writable by root alone.
//!
//! **Rebuilt, not rebound.** A changed `users.yaml` rebuilds the set ([`UserListeners::rebuild`]):
//! a user who is still mapped keeps the very listener they had, so a client blocked in `connect`
//! across the daemon's restart is accepted afterwards; a user no longer mapped loses theirs; a new
//! user gets one.

use std::collections::BTreeMap;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::users_file::UsersFile;

/// File name of a user's daemon socket inside their directory.
pub const USER_SOCKET_FILE: &str = "daemon.sock";

/// `<sockets_dir>/<os user>/daemon.sock`: where `os_user`'s daemon socket is bound.
pub fn user_socket_path(sockets_dir: &Path, os_user: &str) -> PathBuf {
    // TODO(user-mapping-in-supervisor): implement
    let _ = os_user;
    sockets_dir.to_path_buf()
}

/// The listening daemon sockets for every OS user of a mapping, keyed by OS user.
#[derive(Debug, Clone, Default)]
pub struct UserListeners {
    by_user: BTreeMap<String, Arc<UnixListener>>,
}

impl UserListeners {
    /// Create and bind one socket per OS user `users` names, for `service`.
    pub fn bind(
        service: &str,
        sockets_dir: &Path,
        users: &UsersFile,
    ) -> anyhow::Result<UserListeners> {
        // TODO(user-mapping-in-supervisor): implement
        let _ = (service, sockets_dir, users);
        Ok(UserListeners::default())
    }

    /// The set for `next`: an OS user both mappings name keeps this set's listener (the same
    /// `Arc`), a user only `next` names is bound afresh, and a user only this set has is dropped.
    pub fn rebuild(
        &self,
        service: &str,
        sockets_dir: &Path,
        next: &UsersFile,
    ) -> anyhow::Result<UserListeners> {
        // TODO(user-mapping-in-supervisor): implement
        let _ = (service, sockets_dir, next);
        Ok(UserListeners::default())
    }

    /// `os_user`'s listener, if the mapping names them.
    pub fn listener_for(&self, os_user: &str) -> Option<&Arc<UnixListener>> {
        self.by_user.get(os_user)
    }

    /// `(os user, listener)` in OS-user order — the order they are handed over in.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Arc<UnixListener>)> {
        self.by_user
            .iter()
            .map(|(user, listener)| (user.as_str(), listener))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The account the tests run as: the one user a non-root test can create a socket for.
    fn this_user() -> String {
        // SAFETY: `getpwuid` returns a pointer into static storage that is read at once.
        unsafe {
            let entry = libc::getpwuid(libc::geteuid());
            assert!(!entry.is_null(), "the test account has a passwd entry");
            std::ffi::CStr::from_ptr((*entry).pw_name)
                .to_string_lossy()
                .into_owned()
        }
    }

    fn a_mapping_of(rows: &[(&str, &str)]) -> UsersFile {
        let yaml: String = rows
            .iter()
            .map(|(login, os_user)| format!("- {{github_user: {login}, os_user: {os_user}}}\n"))
            .collect();
        UsersFile::from_yaml(&yaml).expect("a valid mapping")
    }

    #[test]
    fn a_mapped_os_user_gets_one_daemon_socket_at_its_derived_path() {
        // Given a mapping naming this account under two GitHub logins
        let workspace = tempfile::TempDir::new().expect("create a workspace");
        let sockets_dir = workspace.path().join("users");
        let me = this_user();
        let users = a_mapping_of(&[("work", &me), ("home", &me)]);

        // When the supervisor binds the mapping's sockets
        let listeners =
            UserListeners::bind("tddy-daemon", &sockets_dir, &users).expect("bind the sockets");

        // Then there is exactly one, at the derived path, and it accepts
        let path = user_socket_path(&sockets_dir, &me);
        assert_eq!(path, sockets_dir.join(&me).join(USER_SOCKET_FILE));
        assert_eq!(
            listeners.iter().map(|(user, _)| user).collect::<Vec<_>>(),
            vec![me.as_str()]
        );
        let _client = std::os::unix::net::UnixStream::connect(&path).expect("connect");
        listeners
            .listener_for(&me)
            .expect("a listener for the mapped user")
            .accept()
            .expect("accept");
    }

    #[test]
    fn a_rebuild_keeps_the_listener_of_a_user_the_new_mapping_still_names() {
        // Given this account's socket, bound for the first mapping
        let workspace = tempfile::TempDir::new().expect("create a workspace");
        let sockets_dir = workspace.path().join("users");
        let me = this_user();
        let before = UserListeners::bind("tddy-daemon", &sockets_dir, &a_mapping_of(&[("a", &me)]))
            .expect("bind the sockets");

        // When the mapping changes only in its GitHub logins
        let after = before
            .rebuild(
                "tddy-daemon",
                &sockets_dir,
                &a_mapping_of(&[("a", &me), ("b", &me)]),
            )
            .expect("rebuild the sockets");

        // Then the user keeps the very same listener — its accept queue survives the restart
        let (was, is) = (before.listener_for(&me), after.listener_for(&me));
        assert!(
            matches!((was, is), (Some(was), Some(is)) if Arc::ptr_eq(was, is)),
            "an unchanged user's descriptor must be kept"
        );
    }

    #[test]
    fn a_rebuild_drops_the_listener_of_a_user_no_longer_mapped() {
        // Given this account's socket
        let workspace = tempfile::TempDir::new().expect("create a workspace");
        let sockets_dir = workspace.path().join("users");
        let me = this_user();
        let before = UserListeners::bind("tddy-daemon", &sockets_dir, &a_mapping_of(&[("a", &me)]))
            .expect("bind the sockets");
        assert!(before.listener_for(&me).is_some(), "bound for the mapping");

        // When the mapping no longer names it
        let after = before
            .rebuild("tddy-daemon", &sockets_dir, &UsersFile::default())
            .expect("rebuild the sockets");

        // Then
        assert!(after.listener_for(&me).is_none());
    }
}
