//! The hosts a daemon can reach through a remote shell, behind `ListDerivedHosts`.
//!
//! A derived host is a machine with no daemon of its own that the serving daemon — its *parent* —
//! can run a session's agent on. It is a view over a [`SessionTarget`], not a connection: the
//! parent owns the session, serves its terminal and carries every call about it.
//!
//! One [`DerivedHostSource`] per kind of host. [`SshConfigDerivedHosts`] is the first: the explicit
//! aliases of an OS user's `~/.ssh/config`, as [`crate::ssh_config`] reads them.
//!
//! # Honesty
//!
//! As [`crate::ssh_config`]: a daemon that reaches no hosts lists `OK` and empty. A source that
//! could not be read is an `Err` carrying the reason, which the handler answers as `FAILED` — never
//! as no hosts.
//!
//! # Listed live
//!
//! Nothing here is persisted or probed. The SSH config is the record; a copy in
//! `known-hosts.json` would be a second one to go stale.
//!
//! [`SessionTarget`]: tddy_service::proto::session::SessionTarget

use std::sync::Arc;

use async_trait::async_trait;

pub use tddy_service::proto::host::{DerivedHost, DerivedHostKind};

use crate::host_private_key::HostUserFiles;

/// The segment of a derived host's id that names its kind, e.g. `ssh` in `daemon-a/ssh/buildbox`.
///
/// `None` for [`DerivedHostKind::Unspecified`]: a host of no kind has no id.
#[must_use]
pub fn kind_segment(kind: DerivedHostKind) -> Option<&'static str> {
    match kind {
        DerivedHostKind::Ssh => Some("ssh"),
        DerivedHostKind::Unspecified => None,
    }
}

/// The id of the SSH host `alias` reached from the daemon `parent_instance_id`:
/// `<parent_instance_id>/ssh/<alias>`.
///
/// `parent_instance_id` is the parent as `KnownHostEntry.instance_id` names it — the value
/// per-host RPCs are keyed on — so the id's first segment addresses the daemon that reaches the
/// host.
#[must_use]
pub fn ssh_derived_host_id(parent_instance_id: &str, alias: &str) -> String {
    format!("{parent_instance_id}/ssh/{alias}")
}

/// One kind of host a daemon derives: where they are listed from, and what kind they are.
#[async_trait]
pub trait DerivedHostSource: Send + Sync {
    /// The kind every host of this source is.
    fn kind(&self) -> DerivedHostKind;

    /// The hosts `os_user` can reach from this daemon, in the source's own order, each with its
    /// id, its label, its parent and the target that starts a session on it.
    ///
    /// `Ok` and empty is a user who reaches none. `Err` is a source that could not be read, with
    /// the reason an operator is shown.
    async fn list(&self, os_user: &str) -> Result<Vec<DerivedHost>, String>;
}

/// The explicit `Host` aliases of an OS user's `~/.ssh/config` as derived hosts of one daemon.
///
/// Each alias is a host of kind [`DerivedHostKind::Ssh`], labelled with the alias, identified by
/// [`ssh_derived_host_id`] and targeted by `SshTarget { config_host: alias }`.
pub struct SshConfigDerivedHosts {
    files: Arc<dyn HostUserFiles>,
    parent_instance_id: String,
}

impl SshConfigDerivedHosts {
    /// A source reading configs through `files`, whose hosts are derived from the daemon
    /// `parent_instance_id`.
    #[must_use]
    pub fn new(files: Arc<dyn HostUserFiles>, parent_instance_id: impl Into<String>) -> Self {
        Self {
            files,
            parent_instance_id: parent_instance_id.into(),
        }
    }
}

#[async_trait]
impl DerivedHostSource for SshConfigDerivedHosts {
    fn kind(&self) -> DerivedHostKind {
        DerivedHostKind::Ssh
    }

    async fn list(&self, os_user: &str) -> Result<Vec<DerivedHost>, String> {
        let files = Arc::clone(&self.files);
        let user = os_user.to_string();
        let listed = tokio::task::spawn_blocking(move || {
            crate::ssh_config::list_ssh_config_hosts(files.as_ref(), &user)
        })
        .await
        .map_err(|_| "listing SSH config hosts on this host did not complete".to_string())?;
        let aliases = match listed {
            Ok(hosts) => hosts.aliases,
            Err(crate::ssh_config::SshConfigHostsError::Unreadable(reason)) => {
                return Err(reason);
            }
        };
        Ok(aliases
            .into_iter()
            .map(|alias| DerivedHost {
                id: ssh_derived_host_id(&self.parent_instance_id, &alias),
                label: alias.clone(),
                kind: DerivedHostKind::Ssh as i32,
                parent_instance_id: self.parent_instance_id.clone(),
                target: Some(tddy_service::proto::session::SessionTarget {
                    kind: Some(tddy_service::proto::session::session_target::Kind::Ssh(
                        tddy_service::proto::session::SshTarget { config_host: alias },
                    )),
                }),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use pretty_assertions::assert_eq;
    use tddy_service::proto::session::{session_target, SessionTarget, SshTarget};

    use super::*;
    use crate::host_private_key::UserFilesUnder;

    const PARENT: &str = "daemon-a";
    const OPERATOR: &str = "testdev";

    /// A home whose `~/.ssh/config` is `config`, with the storage that backs it.
    fn a_home_with_config(config: &str) -> (tempfile::TempDir, PathBuf) {
        let storage = tempfile::tempdir().expect("operator home storage");
        let home = storage.path().join("home").join(OPERATOR);
        std::fs::create_dir_all(home.join(".ssh")).expect("operator has ~/.ssh");
        std::fs::write(home.join(".ssh").join("config"), config).expect("ssh config");
        (storage, home)
    }

    fn the_source_over(home: &Path) -> SshConfigDerivedHosts {
        SshConfigDerivedHosts::new(Arc::new(UserFilesUnder::home(home)), PARENT)
    }

    fn an_ssh_host(alias: &str) -> DerivedHost {
        DerivedHost {
            id: format!("{PARENT}/ssh/{alias}"),
            label: alias.to_string(),
            kind: DerivedHostKind::Ssh as i32,
            parent_instance_id: PARENT.to_string(),
            target: Some(SessionTarget {
                kind: Some(session_target::Kind::Ssh(SshTarget {
                    config_host: alias.to_string(),
                })),
            }),
        }
    }

    /// A reader for which every `~/.ssh/config` is denied, whoever runs the test.
    struct ConfigDenied;

    impl HostUserFiles for ConfigDenied {
        fn home_dir(&self, os_user: &str) -> Result<PathBuf, String> {
            Ok(PathBuf::from("/home").join(os_user))
        }

        fn read_as_user(&self, _os_user: &str, _path: &Path) -> Result<Vec<u8>, String> {
            Err("permission denied".to_string())
        }

        fn list_files_as_user(&self, _os_user: &str, _dir: &Path) -> Result<Vec<PathBuf>, String> {
            Err("permission denied".to_string())
        }
    }

    #[test]
    fn an_ssh_hosts_id_is_its_parent_then_ssh_then_its_alias() {
        assert_eq!(
            ssh_derived_host_id("daemon-a", "buildbox"),
            "daemon-a/ssh/buildbox"
        );
    }

    #[test]
    fn the_same_alias_under_two_parents_has_two_ids() {
        assert_ne!(
            ssh_derived_host_id("daemon-a", "buildbox"),
            ssh_derived_host_id("daemon-b", "buildbox")
        );
    }

    #[test]
    fn the_ssh_kind_is_the_ssh_segment_and_no_kind_has_none() {
        assert_eq!(
            (
                kind_segment(DerivedHostKind::Ssh),
                kind_segment(DerivedHostKind::Unspecified),
            ),
            (Some("ssh"), None)
        );
    }

    #[test]
    fn the_ssh_config_source_is_of_the_ssh_kind() {
        // Given
        let (_storage, home) = a_home_with_config("");

        // When
        let kind = the_source_over(&home).kind();

        // Then
        assert_eq!(kind, DerivedHostKind::Ssh);
    }

    #[tokio::test]
    async fn lists_each_explicit_alias_as_an_ssh_host_targeting_that_alias() {
        // Given a config naming two machines and a catch-all
        let (_storage, home) = a_home_with_config(
            "Host buildbox\n    HostName 10.0.0.7\nHost gpu-rig\nHost *\n    ForwardAgent no\n",
        );

        // When
        let listed = the_source_over(&home).list(OPERATOR).await;

        // Then each explicit alias is a host of its own, in the config's order
        assert_eq!(
            listed,
            Ok(vec![an_ssh_host("buildbox"), an_ssh_host("gpu-rig")])
        );
    }

    #[tokio::test]
    async fn a_user_with_no_ssh_config_reaches_no_hosts() {
        // Given a home with no ~/.ssh at all
        let storage = tempfile::tempdir().expect("operator home storage");

        // When
        let listed = the_source_over(storage.path()).list(OPERATOR).await;

        // Then
        assert_eq!(listed, Ok(Vec::new()));
    }

    #[tokio::test]
    async fn a_config_that_cannot_be_read_is_an_error_carrying_the_reason() {
        // Given
        let source = SshConfigDerivedHosts::new(Arc::new(ConfigDenied), PARENT);

        // When
        let listed = source.list(OPERATOR).await;

        // Then
        assert_eq!(listed, Err("permission denied".to_string()));
    }
}
