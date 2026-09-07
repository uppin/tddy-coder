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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A desktop attached to a host.
///
/// Field names are the persisted JSON keys — renaming one drops that column for every already
/// attached desktop, so treat them as a format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// What the store keeps on disk.
///
/// A named wrapper rather than a bare map so a later field (a format version, say) can be added
/// without every already-written file becoming unreadable.
#[derive(Debug, Default, Serialize, Deserialize)]
struct TargetsFile {
    /// Daemon instance id → the desktops attached to that host. Ordered so the file a reviewer or
    /// an operator opens is stable between writes rather than reshuffled by hash order.
    #[serde(default)]
    hosts: BTreeMap<String, Vec<HostDesktopTarget>>,
}

/// A [`HostDesktopTargetStore`] persisted under one directory, alongside — never inside — the
/// per-session screen-sharing vault.
pub struct FileHostDesktopTargetStore {
    targets_path: PathBuf,
    /// Held across each read-modify-write. Two RPCs attaching a desktop at the same moment both
    /// read, both write, and the second silently drops the first's target without it.
    rewrite: Mutex<()>,
}

impl FileHostDesktopTargetStore {
    pub fn new(storage_dir: impl AsRef<Path>) -> Self {
        Self {
            targets_path: storage_dir.as_ref().join("host-desktop-targets.json"),
            rewrite: Mutex::new(()),
        }
    }

    /// The file's contents, or an empty set when nothing has been attached yet.
    ///
    /// A file that exists but does not parse is an **error**, never an empty set: the callers that
    /// go on to write would otherwise replace a damaged file with a file holding one target, and
    /// every other host's desktops would be gone with no way back.
    fn read_file(&self) -> Result<TargetsFile, String> {
        match std::fs::read(&self.targets_path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
                format!(
                    "{} is not readable as host desktop targets: {e}",
                    self.targets_path.display()
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(TargetsFile::default()),
            Err(e) => Err(format!("reading {}: {e}", self.targets_path.display())),
        }
    }

    /// Replace the file with `contents`.
    ///
    /// Plain [`tddy_core::atomic_file::write_atomic`], deliberately: a target is a label, a host, a
    /// port, a protocol and a username — an address, not a credential. The session-scoped vault's
    /// encrypted-file pattern exists because it stores a password; this store never sees one (a
    /// host desktop password is prompted, decrypted, used and dropped), so copying that pattern
    /// would buy nothing and add a key to manage. What is needed is the atomic half: a half-written
    /// file here reads as "this host has no desktops" and silently loses every attachment.
    fn write_file(&self, contents: &TargetsFile) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(contents)
            .map_err(|e| format!("encoding host desktop targets: {e}"))?;
        tddy_core::atomic_file::write_atomic_labelled(&self.targets_path, json)
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, ()> {
        self.rewrite.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl HostDesktopTargetStore for FileHostDesktopTargetStore {
    fn list(&self, daemon_instance_id: &str) -> Vec<HostDesktopTarget> {
        match self.read_file() {
            Ok(file) => file
                .hosts
                .get(daemon_instance_id)
                .cloned()
                .unwrap_or_default(),
            // The signature leaves nowhere to report this, so it goes to the log loudly rather
            // than reaching an operator as a host that quietly lost its desktops. Nothing here
            // rewrites the file, so the damaged one is still there to recover from.
            Err(e) => {
                error!("cannot list host desktop targets: {e}");
                Vec::new()
            }
        }
    }

    /// The id is assigned here, not accepted from the caller: it addresses a running bridge, and a
    /// caller-chosen one could collide with a target already attached to the same host.
    fn add(&self, daemon_instance_id: &str, target: HostDesktopTarget) -> Result<String, String> {
        let _rewriting = self.locked();
        let mut file = self.read_file()?;
        let target_id = Uuid::new_v4().to_string();
        file.hosts
            .entry(daemon_instance_id.to_string())
            .or_default()
            .push(HostDesktopTarget {
                target_id: target_id.clone(),
                ..target
            });
        self.write_file(&file)?;
        Ok(target_id)
    }

    fn remove(&self, daemon_instance_id: &str, target_id: &str) -> Result<(), String> {
        let _rewriting = self.locked();
        let mut file = self.read_file()?;
        let attached = file
            .hosts
            .get_mut(daemon_instance_id)
            .ok_or_else(|| not_attached(daemon_instance_id, target_id))?;
        let before = attached.len();
        attached.retain(|t| t.target_id != target_id);
        if attached.len() == before {
            return Err(not_attached(daemon_instance_id, target_id));
        }
        if attached.is_empty() {
            file.hosts.remove(daemon_instance_id);
        }
        self.write_file(&file)
    }
}

fn not_attached(daemon_instance_id: &str, target_id: &str) -> String {
    format!("host {daemon_instance_id} has no desktop target {target_id}")
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
