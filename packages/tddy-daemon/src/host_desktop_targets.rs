//! Desktop targets that belong to a **host** rather than to a coding session.
//!
//! Every `ScreenSharingService` request carries a `session_id`, and
//! [`crate::screen_sharing_vault`] keeps its credentials under the session directory. That is right
//! for a desktop attached to a piece of work, and wrong for a machine: a desktop outlives any
//! session on it, and deleting a session must not delete the host's target.
//!
//! # What this does not do
//!
//! It does not touch the bridges, the LiveKit republishing, or the browser overlay — all three are
//! reused unchanged. Host scope is an *addressing and storage* change. There is no browser-side
//! VNC/RDP client and there must never be one.
//!
//! # Credentials
//!
//! ⚠ The session-scoped precedent **stores** a credential, encrypted. This does not. A host desktop
//! password follows `#hosts-screen 6/8`'s posture — prompt, encrypt under the host's key, use, drop
//! — so the Hosts screen has one secret-handling model rather than two. That is a deliberate
//! inconsistency with the session path, and an arguable one; see the PRD.

use std::path::{Path, PathBuf};

/// A desktop attached to a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDesktopTarget {
    pub target_id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    /// Mirrors `screen_sharing.proto`'s `Protocol`. See [`DesktopProtocolId`].
    pub protocol: i32,
    pub username: String,
}

/// The `screen_sharing.proto` `Protocol` discriminants, named so a fixture reads as a protocol
/// rather than as a bare integer.
pub struct DesktopProtocolId;

impl DesktopProtocolId {
    pub const VNC: i32 = 1;
    pub const RDP: i32 = 2;
}

/// Stores desktop targets per host, separately from the session-scoped vault.
pub trait HostDesktopTargetStore: Send + Sync {
    /// Targets attached to `daemon_instance_id`.
    fn list(&self, daemon_instance_id: &str) -> Vec<HostDesktopTarget>;

    /// Attach a target to a host, returning its new id.
    fn add(&self, daemon_instance_id: &str, target: HostDesktopTarget) -> Result<String, String>;

    /// Detach a target from a host.
    fn remove(&self, daemon_instance_id: &str, target_id: &str) -> Result<(), String>;
}

/// A [`HostDesktopTargetStore`] persisted under one directory, alongside — never inside — the
/// per-session screen-sharing vault.
pub struct FileHostDesktopTargetStore {
    #[allow(dead_code)] // read once persistence lands (#hosts-screen 8/8 green)
    targets_path: PathBuf,
}

impl FileHostDesktopTargetStore {
    pub fn new(storage_dir: impl AsRef<Path>) -> Self {
        Self {
            targets_path: storage_dir.as_ref().join("host-desktop-targets.json"),
        }
    }
}

impl HostDesktopTargetStore for FileHostDesktopTargetStore {
    fn list(&self, _daemon_instance_id: &str) -> Vec<HostDesktopTarget> {
        // TODO(desktop-connect): implement
        unimplemented!("desktop-connect: list")
    }

    fn add(&self, _daemon_instance_id: &str, _target: HostDesktopTarget) -> Result<String, String> {
        // TODO(desktop-connect): implement
        unimplemented!("desktop-connect: add")
    }

    fn remove(&self, _daemon_instance_id: &str, _target_id: &str) -> Result<(), String> {
        // TODO(desktop-connect): implement
        unimplemented!("desktop-connect: remove")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_HOST: &str = "workstation-1";
    const ANOTHER_HOST: &str = "server-2";

    /// Builder so a test states only the field it is about.
    struct TargetBuilder {
        target: HostDesktopTarget,
    }

    fn a_desktop_target() -> TargetBuilder {
        TargetBuilder {
            target: HostDesktopTarget {
                target_id: String::new(),
                label: "dev box".to_string(),
                host: "127.0.0.1".to_string(),
                port: 5900,
                protocol: DesktopProtocolId::VNC,
                username: "ada".to_string(),
            },
        }
    }

    impl TargetBuilder {
        fn labelled(mut self, label: &str) -> Self {
            self.target.label = label.to_string();
            self
        }

        fn build(self) -> HostDesktopTarget {
            self.target
        }
    }

    /// A store over a fresh directory, plus the directory so a test can inspect what was written.
    fn a_target_store() -> (FileHostDesktopTargetStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (FileHostDesktopTargetStore::new(dir.path()), dir)
    }

    fn labels_for(store: &FileHostDesktopTargetStore, host: &str) -> Vec<String> {
        store.list(host).into_iter().map(|t| t.label).collect()
    }

    /// A desktop belongs to the machine it was attached to. Without this, adding one host's desktop
    /// would surface it on every other row — and connecting would reach the wrong machine.
    #[test]
    fn a_target_added_to_one_host_is_not_visible_on_another() {
        // Given a target attached to one host
        let (store, _dir) = a_target_store();
        store
            .add(A_HOST, a_desktop_target().labelled("dev box").build())
            .expect("attaching a target to a host");

        // When another host's targets are listed
        let elsewhere = labels_for(&store, ANOTHER_HOST);

        // Then the target belongs only to the host it was attached to
        assert_eq!(labels_for(&store, A_HOST), vec!["dev box".to_string()]);
        assert!(
            elsewhere.is_empty(),
            "a target must not leak onto another host, got {elsewhere:?}"
        );
    }

    /// Host scope and session scope are separate stores: deleting a session must not take a host's
    /// desktop with it, which is the whole reason this type exists rather than reusing the vault.
    #[test]
    fn a_host_scoped_target_is_not_written_into_the_session_vaults_file() {
        // Given a target attached to a host
        let (store, dir) = a_target_store();
        store
            .add(A_HOST, a_desktop_target().build())
            .expect("attaching a target to a host");

        // When the storage directory is inspected
        let session_vault = dir.path().join("vault.json");

        // Then nothing was written where the session-scoped vault keeps its own credentials
        assert!(
            !session_vault.exists(),
            "host targets must not share the session vault's file"
        );
    }

    #[test]
    fn a_removed_target_no_longer_appears_for_its_host() {
        // Given a host with one attached target
        let (store, _dir) = a_target_store();
        let target_id = store
            .add(A_HOST, a_desktop_target().build())
            .expect("attaching a target to a host");

        // When it is detached
        store
            .remove(A_HOST, &target_id)
            .expect("detaching it again");

        // Then the host has no targets left
        assert!(labels_for(&store, A_HOST).is_empty());
    }

    /// Two hosts can hold targets at once, each seeing only its own.
    #[test]
    fn each_host_lists_only_its_own_targets() {
        // Given a target on each of two hosts
        let (store, _dir) = a_target_store();
        store
            .add(A_HOST, a_desktop_target().labelled("dev box").build())
            .expect("attaching to the first host");
        store
            .add(
                ANOTHER_HOST,
                a_desktop_target().labelled("build server").build(),
            )
            .expect("attaching to the second host");

        // When each host is listed
        // Then each sees exactly its own
        assert_eq!(labels_for(&store, A_HOST), vec!["dev box".to_string()]);
        assert_eq!(
            labels_for(&store, ANOTHER_HOST),
            vec!["build server".to_string()]
        );
    }

    /// Removing a target that was never attached is an error, not a silent success — a caller that
    /// believes it detached something must not be told it did.
    #[test]
    fn removing_a_target_that_was_never_attached_is_reported_as_an_error() {
        // Given a host with no targets
        let (store, _dir) = a_target_store();

        // When a target that was never attached is removed
        let outcome = store.remove(A_HOST, "never-attached");

        // Then the caller is told, rather than being left to assume it worked
        assert!(outcome.is_err());
    }
}
