//! `ListDerivedHosts`: the real handler over an operator's home on a temp path. The hosts a daemon
//! reaches are the explicit aliases of that operator's `~/.ssh/config`; nothing is connected to.
//!
//! The one stand-in is the OS seam ([`HostUserFiles`]): reading a file *as another OS user* is the
//! step a test cannot perform, and denying a read is the failure it cannot stage with real
//! permissions without depending on who runs it.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-derived-hosts.md

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_host_service::host_private_key::HostUserFiles;
use tddy_host_service::test_util::{test_service, TEST_TOKEN};
use tddy_host_service::HostServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::host::{
    DerivedHost, DerivedHostKind, HostService, ListDerivedHostsRequest, ListDerivedHostsResponse,
    ListKnownHostsRequest, ProbeOutcome,
};
use tddy_service::proto::session::{session_target, SessionTarget, SshTarget};

/// An empty `daemon_instance_id` is the protocol's spelling for "the daemon serving this call".
const THIS_DAEMON: &str = "";
/// A listing reads one file; this only turns a hang into a failure.
const LISTING_WINDOW: Duration = Duration::from_secs(2);
/// What the OS says when the operator's config cannot be opened.
const PERMISSION_DENIED: &str = "permission denied";

/// The operator's files, read from a home on a temp path — whichever OS user is asked for.
struct AnOperatorHome {
    home: PathBuf,
    /// Whether `~/.ssh/config` can be opened at all.
    config_readable: bool,
}

impl HostUserFiles for AnOperatorHome {
    fn home_dir(&self, _os_user: &str) -> Result<PathBuf, String> {
        Ok(self.home.clone())
    }

    fn read_as_user(&self, _os_user: &str, path: &Path) -> Result<Vec<u8>, String> {
        if !self.config_readable && path == self.home.join(".ssh").join("config") {
            return Err(PERMISSION_DENIED.to_string());
        }
        std::fs::read(path).map_err(|e| e.to_string())
    }

    fn list_files_as_user(&self, _os_user: &str, dir: &Path) -> Result<Vec<PathBuf>, String> {
        Err(format!("{} is not listed by this stand-in", dir.display()))
    }
}

/// A daemon serving `HostService`, with the directories that back it.
struct ADaemon {
    service: HostServiceImpl,
    _home: tempfile::TempDir,
    _tddy_data: tempfile::TempDir,
}

fn a_daemon_over(ssh_config: &str, config_readable: bool) -> ADaemon {
    let home_storage = tempfile::tempdir().expect("operator home storage");
    let home = home_storage.path().to_path_buf();
    std::fs::create_dir_all(home.join(".ssh")).expect("the operator has ~/.ssh");
    std::fs::write(home.join(".ssh").join("config"), ssh_config).expect("ssh config");
    let tddy_data = tempfile::tempdir().expect("tddy data dir");
    let service = test_service(tddy_data.path()).with_host_user_files(Arc::new(AnOperatorHome {
        home,
        config_readable,
    }));
    ADaemon {
        service,
        _home: home_storage,
        _tddy_data: tddy_data,
    }
}

/// A daemon whose operator's `~/.ssh/config` is `ssh_config`.
fn a_daemon_whose_operator_has_the_ssh_config(ssh_config: &str) -> ADaemon {
    a_daemon_over(ssh_config, true)
}

/// A daemon whose operator has an SSH config naming `buildbox` that this host cannot open.
fn a_daemon_that_cannot_read_its_operators_ssh_config() -> ADaemon {
    a_daemon_over("Host buildbox\n", false)
}

impl ADaemon {
    /// What this daemon answers when asked for the hosts it reaches.
    async fn derived_hosts(&self) -> ListDerivedHostsResponse {
        tokio::time::timeout(
            LISTING_WINDOW,
            self.service
                .list_derived_hosts(Request::direct(ListDerivedHostsRequest {
                    session_token: TEST_TOKEN.to_string(),
                    daemon_instance_id: THIS_DAEMON.to_string(),
                })),
        )
        .await
        .expect("a listing reads one file and must not hang")
        .expect("a listing is answered, whatever its outcome")
        .into_inner()
    }

    /// The id this daemon is listed under on the Hosts screen: its `KnownHostEntry.instance_id`,
    /// the value per-host RPCs are keyed on.
    async fn known_host_id(&self) -> String {
        self.service
            .list_known_hosts(Request::direct(ListKnownHostsRequest {
                session_token: TEST_TOKEN.to_string(),
            }))
            .await
            .expect("the known hosts are listed")
            .into_inner()
            .hosts
            .into_iter()
            .find(|host| host.is_local)
            .expect("the daemon serving the call is a known host")
            .instance_id
    }
}

/// The target that starts a session on the SSH host `alias`.
fn the_ssh_target(alias: &str) -> SessionTarget {
    SessionTarget {
        kind: Some(session_target::Kind::Ssh(SshTarget {
            config_host: alias.to_string(),
        })),
    }
}

/// What an operator is shown of a derived host and what a start is addressed with: its label, its
/// kind and its target.
fn offered(host: &DerivedHost) -> (String, i32, Option<SessionTarget>) {
    (host.label.clone(), host.kind, host.target.clone())
}

fn an_offered_ssh_host(alias: &str) -> (String, i32, Option<SessionTarget>) {
    (
        alias.to_string(),
        DerivedHostKind::Ssh as i32,
        Some(the_ssh_target(alias)),
    )
}

#[tokio::test]
async fn a_daemons_ssh_config_aliases_list_as_derived_hosts_with_their_targets() {
    // Given a daemon whose operator's config names two machines and a catch-all
    let daemon = a_daemon_whose_operator_has_the_ssh_config(
        "Host buildbox\n    HostName 10.0.0.7\nHost gpu-rig\nHost *\n    ForwardAgent no\n",
    );

    // When
    let listed = daemon.derived_hosts().await;

    // Then each explicit alias is an SSH host with the target that starts a session on it, in the
    // config's order
    assert_eq!(
        (
            listed.outcome,
            listed.failure_reason.as_str(),
            listed.hosts.iter().map(offered).collect::<Vec<_>>(),
        ),
        (
            ProbeOutcome::Ok as i32,
            "",
            vec![
                an_offered_ssh_host("buildbox"),
                an_offered_ssh_host("gpu-rig")
            ],
        )
    );
}

#[tokio::test]
async fn a_daemon_with_no_aliases_lists_empty_and_ok() {
    // Given a daemon whose operator's config names no machine
    let daemon = a_daemon_whose_operator_has_the_ssh_config("Host *\n    ForwardAgent no\n");

    // When
    let listed = daemon.derived_hosts().await;

    // Then it reaches no hosts, and says so as a listing that was made
    assert_eq!(
        listed,
        ListDerivedHostsResponse {
            outcome: ProbeOutcome::Ok as i32,
            failure_reason: String::new(),
            hosts: Vec::new(),
        }
    );
}

#[tokio::test]
async fn an_unreadable_ssh_config_fails_the_listing_with_its_reason() {
    // Given
    let daemon = a_daemon_that_cannot_read_its_operators_ssh_config();

    // When
    let listed = daemon.derived_hosts().await;

    // Then the listing failed for the reason the read failed, and invents no hosts
    assert_eq!(
        listed,
        ListDerivedHostsResponse {
            outcome: ProbeOutcome::Failed as i32,
            failure_reason: PERMISSION_DENIED.to_string(),
            hosts: Vec::new(),
        }
    );
}

#[tokio::test]
async fn a_derived_hosts_id_names_its_parent_and_its_alias() {
    // Given a daemon that reaches buildbox, and the id it is itself listed under
    let daemon = a_daemon_whose_operator_has_the_ssh_config("Host buildbox\n");
    let parent = daemon.known_host_id().await;

    // When
    let listed = daemon.derived_hosts().await;

    // Then the host names that daemon as its parent, and its id is the parent's, then its alias
    assert_eq!(
        listed
            .hosts
            .iter()
            .map(|host| (host.id.clone(), host.parent_instance_id.clone()))
            .collect::<Vec<_>>(),
        vec![(format!("{parent}/ssh/buildbox"), parent)]
    );
}
