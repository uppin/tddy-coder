//! `ListDerivedHosts`, for a signed-in person with Coder accounts: the real handler over stand-ins
//! for the vault that holds the accounts, the deployments that list their workspaces, and the
//! host's tooling probe. No deployment is reached and no `coder` is run: a listing never needs the
//! CLI, it only says whether a workspace can be used without it.
//!
//! The SSH hosts of the same listing come from an operator's home on a temp path, as in
//! `derived_hosts_acceptance.rs`.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-coder-hosts.md

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_credentials::SecretString;
use tddy_host_service::coder_derived_hosts::{CoderAccountRef, CoderAccountSource};
use tddy_host_service::coder_workspaces::{
    CoderCredentialSource, CoderCredentials, CoderWorkspaceListError,
};
use tddy_host_service::host_private_key::HostUserFiles;
use tddy_host_service::host_tooling::{
    CoderCliStatus, GitIdentity, GithubCliStatus, HostTooling, HostToolingProbe, ProbeOutcome,
};
use tddy_host_service::ssh_agent::AgentStatus;
use tddy_host_service::test_util::{
    a_coder_vault_of, a_coder_workspace, coder_deployments, test_service, ACoderVault,
    CoderDeployments, TEST_TOKEN, TEST_USER,
};
use tddy_host_service::HostServiceImpl;
use tddy_rpc::{Code, Request};
use tddy_service::proto::host::{
    DerivedHost, DerivedHostKind, HostService, ListDerivedHostsRequest, ListDerivedHostsResponse,
    ListKnownHostsRequest, ProbeOutcome as WireProbeOutcome,
};
use tddy_service::proto::session::{session_target, CoderTarget, SessionTarget};

/// An empty `daemon_instance_id` is the protocol's spelling for "the daemon serving this call".
const THIS_DAEMON: &str = "";
/// A listing is a vault read and a request per account; this only turns a hang into a failure.
const LISTING_WINDOW: Duration = Duration::from_secs(2);

/// The signed-in person's account on their employer's deployment.
const WORK: &str = "coder-3f1c9a52@coder.example.com";
const WORK_URL: &str = "https://coder.example.com";
/// Their account on a second deployment.
const LAB: &str = "coder-77aa01b3@coder.lab.example";
const LAB_URL: &str = "https://coder.lab.example";

/// The operator's files, read from a home on a temp path — whichever OS user is asked for.
struct AnOperatorHome {
    home: PathBuf,
}

impl HostUserFiles for AnOperatorHome {
    fn home_dir(&self, _os_user: &str) -> Result<PathBuf, String> {
        Ok(self.home.clone())
    }

    fn read_as_user(&self, _os_user: &str, path: &Path) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| e.to_string())
    }

    fn list_files_as_user(&self, _os_user: &str, dir: &Path) -> Result<Vec<PathBuf>, String> {
        Err(format!("{} is not listed by this stand-in", dir.display()))
    }
}

/// A tooling probe that answers with one fixed reading of the `coder` CLI.
struct AHostWhoseCoderCliIs(CoderCliStatus);

impl HostToolingProbe for AHostWhoseCoderCliIs {
    fn probe(&self, _os_user: &str, _github_hostname: Option<&str>) -> HostTooling {
        HostTooling {
            git: GitIdentity {
                outcome: ProbeOutcome::Ok,
                name_and_email: None,
            },
            github_cli: GithubCliStatus {
                outcome: ProbeOutcome::Ok,
                installed: false,
                authenticated: false,
                login: None,
            },
            ssh_agent: AgentStatus::unreachable(),
            remote_desktop: Vec::new(),
            coder_cli: self.0.clone(),
        }
    }
}

fn the_coder_cli_installed() -> CoderCliStatus {
    CoderCliStatus {
        installed: true,
        version: Some("v2.16.0".to_string()),
    }
}

fn credentials_at(url: &str) -> CoderCredentials {
    CoderCredentials::new(url, SecretString::new(format!("token-for-{url}")))
}

/// The open vault of the person [`TEST_TOKEN`] signs in, holding their `Work` and `Lab` accounts.
fn a_vault_holding_work_and_lab() -> ACoderVault {
    a_coder_vault_of(TEST_USER)
        .holding(CoderAccountRef::new(WORK, "Work"), credentials_at(WORK_URL))
        .holding(CoderAccountRef::new(LAB, "Lab"), credentials_at(LAB_URL))
}

/// A daemon serving `HostService`, with the directories that back it.
struct ADaemon {
    service: HostServiceImpl,
    _home: tempfile::TempDir,
    _tddy_data: tempfile::TempDir,
}

/// What a daemon is built from: its operator's SSH config, the signed-in person's vault, the
/// deployments their accounts are on, and whether the host has the `coder` CLI.
struct ADaemonSetup {
    ssh_config: &'static str,
    vault: ACoderVault,
    deployments: CoderDeployments,
    coder_cli: CoderCliStatus,
}

/// A daemon whose operator has no SSH hosts, with the `coder` CLI installed, over `vault` and
/// `deployments`.
fn a_daemon_reaching(vault: ACoderVault, deployments: CoderDeployments) -> ADaemon {
    a_daemon(ADaemonSetup {
        ssh_config: "",
        vault,
        deployments,
        coder_cli: the_coder_cli_installed(),
    })
}

fn a_daemon(setup: ADaemonSetup) -> ADaemon {
    let home_storage = tempfile::tempdir().expect("operator home storage");
    let home = home_storage.path().to_path_buf();
    std::fs::create_dir_all(home.join(".ssh")).expect("the operator has ~/.ssh");
    std::fs::write(home.join(".ssh").join("config"), setup.ssh_config).expect("ssh config");
    let tddy_data = tempfile::tempdir().expect("tddy data dir");
    let vault = Arc::new(setup.vault);
    let service = test_service(tddy_data.path())
        .with_host_user_files(Arc::new(AnOperatorHome { home }))
        .with_host_tooling(Arc::new(AHostWhoseCoderCliIs(setup.coder_cli)))
        .with_coder_accounts(Arc::clone(&vault) as Arc<dyn CoderAccountSource>)
        .with_coder_credentials(vault as Arc<dyn CoderCredentialSource>)
        .with_coder_workspaces(Arc::new(setup.deployments));
    ADaemon {
        service,
        _home: home_storage,
        _tddy_data: tddy_data,
    }
}

impl ADaemon {
    /// What this daemon answers when the signed-in person asks for the hosts it reaches, or the
    /// code and message it refused with.
    async fn derived_hosts(&self) -> Result<ListDerivedHostsResponse, (Code, String)> {
        tokio::time::timeout(
            LISTING_WINDOW,
            self.service
                .list_derived_hosts(Request::direct(ListDerivedHostsRequest {
                    session_token: TEST_TOKEN.to_string(),
                    daemon_instance_id: THIS_DAEMON.to_string(),
                })),
        )
        .await
        .expect("a listing over stand-ins must not hang")
        .map(tddy_rpc::Response::into_inner)
        .map_err(|status| (status.code, status.message))
    }

    /// The id this daemon is listed under on the Hosts screen: its `KnownHostEntry.instance_id`.
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

/// The target that starts a session on `ada`'s workspace `workspace` of the account `account_id`.
fn the_coder_target(account_id: &str, workspace: &str) -> SessionTarget {
    SessionTarget {
        kind: Some(session_target::Kind::Coder(CoderTarget {
            account_id: account_id.to_string(),
            owner: "ada".to_string(),
            workspace: workspace.to_string(),
            agent: String::new(),
        })),
    }
}

/// `ada`'s running workspace `workspace` of the account `account_id`, as `parent` lists it.
fn a_usable_coder_host(
    parent: &str,
    account_id: &str,
    account_label: &str,
    workspace: &str,
) -> DerivedHost {
    DerivedHost {
        id: format!("{parent}/coder/{account_id}/ada/{workspace}"),
        label: format!("{workspace} ({account_label})"),
        kind: DerivedHostKind::Coder as i32,
        parent_instance_id: parent.to_string(),
        target: Some(the_coder_target(account_id, workspace)),
        usable: true,
        unusable_reason: String::new(),
    }
}

/// A listing that was made, of `hosts`.
fn listed_ok(hosts: Vec<DerivedHost>) -> ListDerivedHostsResponse {
    ListDerivedHostsResponse {
        outcome: WireProbeOutcome::Ok as i32,
        failure_reason: String::new(),
        hosts,
    }
}

/// Whether a listed host can be used, and why not: its label, `usable`, `unusable_reason`.
type Usability = (String, bool, String);

/// The usability of each host of `listing`, in its order.
fn usability(
    listing: Result<ListDerivedHostsResponse, (Code, String)>,
) -> Result<Vec<Usability>, (Code, String)> {
    listing.map(|listing| {
        listing
            .hosts
            .into_iter()
            .map(|host| (host.label, host.usable, host.unusable_reason))
            .collect()
    })
}

#[tokio::test]
async fn each_coder_accounts_workspaces_list_as_derived_hosts() {
    // Given a person with two Coder accounts, each with a running workspace
    let daemon = a_daemon_reaching(
        a_vault_holding_work_and_lab(),
        coder_deployments()
            .listing(
                WORK_URL,
                vec![a_coder_workspace("ada", "devbox", "running")],
            )
            .listing(LAB_URL, vec![a_coder_workspace("ada", "gpu", "running")]),
    );
    let parent = daemon.known_host_id().await;

    // When
    let listed = daemon.derived_hosts().await;

    // Then each workspace is a Coder host of this daemon, named with its account, usable, and
    // carrying the target that starts a session on it — accounts in the vault's order
    assert_eq!(
        listed,
        Ok(listed_ok(vec![
            a_usable_coder_host(&parent, WORK, "Work", "devbox"),
            a_usable_coder_host(&parent, LAB, "Lab", "gpu"),
        ]))
    );
}

#[tokio::test]
async fn a_stopped_workspace_lists_as_unusable_with_its_status() {
    // Given an account with one running and one stopped workspace
    let daemon = a_daemon_reaching(
        a_vault_holding_work_and_lab(),
        coder_deployments()
            .listing(
                WORK_URL,
                vec![
                    a_coder_workspace("ada", "devbox", "running"),
                    a_coder_workspace("ada", "scratch", "stopped"),
                ],
            )
            .listing(LAB_URL, Vec::new()),
    );

    // When
    let listed = usability(daemon.derived_hosts().await);

    // Then the stopped workspace is listed too, as one that cannot be used, saying it is stopped
    assert_eq!(
        listed,
        Ok(vec![
            ("devbox (Work)".to_string(), true, String::new()),
            (
                "scratch (Work)".to_string(),
                false,
                "the workspace is stopped".to_string()
            ),
        ])
    );
}

#[tokio::test]
async fn a_daemon_without_the_coder_cli_lists_workspaces_as_unusable_and_says_why() {
    // Given a running workspace, and a daemon whose host has no coder executable
    let daemon = a_daemon(ADaemonSetup {
        ssh_config: "",
        vault: a_vault_holding_work_and_lab(),
        deployments: coder_deployments()
            .listing(
                WORK_URL,
                vec![a_coder_workspace("ada", "devbox", "running")],
            )
            .listing(LAB_URL, Vec::new()),
        coder_cli: CoderCliStatus::not_installed(),
    });

    // When
    let listed = usability(daemon.derived_hosts().await);

    // Then the workspace is listed — the listing needs no CLI — and says what is missing to use it
    assert_eq!(
        listed,
        Ok(vec![(
            "devbox (Work)".to_string(),
            false,
            "the coder CLI is not installed on this daemon".to_string()
        )])
    );
}

#[tokio::test]
async fn a_locked_vault_fails_the_coder_listing_and_leaves_the_ssh_hosts_listed() {
    // Given a daemon that reaches buildbox over SSH, and a person whose vault is locked
    let daemon = a_daemon(ADaemonSetup {
        ssh_config: "Host buildbox\n    HostName 10.0.0.7\n",
        vault: a_vault_holding_work_and_lab().locked(),
        deployments: coder_deployments(),
        coder_cli: the_coder_cli_installed(),
    });

    // When
    let listed = daemon.derived_hosts().await.map(|listing| {
        (
            listing.outcome,
            listing.failure_reason,
            listing
                .hosts
                .into_iter()
                .map(|host| (host.label, host.kind))
                .collect::<Vec<_>>(),
        )
    });

    // Then the listing failed, saying the vault is locked — not that there are no workspaces —
    // and the SSH host is still offered
    assert_eq!(
        listed,
        Ok((
            WireProbeOutcome::Failed as i32,
            "the vault holding this Coder account is locked: unlock it and list again".to_string(),
            vec![("buildbox".to_string(), DerivedHostKind::Ssh as i32)],
        ))
    );
}

#[tokio::test]
async fn one_accounts_rejected_token_does_not_hide_another_accounts_workspaces() {
    // Given Work's deployment rejecting its token, and Lab's listing a running workspace
    let daemon = a_daemon_reaching(
        a_vault_holding_work_and_lab(),
        coder_deployments()
            .failing(
                WORK_URL,
                CoderWorkspaceListError::Rejected("API key is invalid.".to_string()),
            )
            .listing(LAB_URL, vec![a_coder_workspace("ada", "gpu", "running")]),
    );
    let parent = daemon.known_host_id().await;

    // When
    let listed = daemon.derived_hosts().await;

    // Then the listing was made: Work is one host that cannot be used, saying its token was
    // rejected and targeting nothing, and Lab's workspace is offered as usual
    assert_eq!(
        listed,
        Ok(listed_ok(vec![
            DerivedHost {
                id: format!("{parent}/coder/{WORK}"),
                label: "Work".to_string(),
                kind: DerivedHostKind::Coder as i32,
                parent_instance_id: parent.clone(),
                target: None,
                usable: false,
                unusable_reason:
                    "the Coder deployment rejected this account's token: API key is invalid."
                        .to_string(),
            },
            a_usable_coder_host(&parent, LAB, "Lab", "gpu"),
        ]))
    );
}

#[tokio::test]
async fn same_named_workspaces_of_two_accounts_have_different_ids() {
    // Given two accounts, each with a workspace ada/devbox
    let daemon = a_daemon_reaching(
        a_vault_holding_work_and_lab(),
        coder_deployments()
            .listing(
                WORK_URL,
                vec![a_coder_workspace("ada", "devbox", "running")],
            )
            .listing(LAB_URL, vec![a_coder_workspace("ada", "devbox", "running")]),
    );
    let parent = daemon.known_host_id().await;

    // When
    let listed = daemon.derived_hosts().await.map(|listing| {
        listing
            .hosts
            .into_iter()
            .map(|host| (host.id, host.label))
            .collect::<Vec<_>>()
    });

    // Then each is a host of its own: its id and its label name its account
    assert_eq!(
        listed,
        Ok(vec![
            (
                format!("{parent}/coder/{WORK}/ada/devbox"),
                "devbox (Work)".to_string()
            ),
            (
                format!("{parent}/coder/{LAB}/ada/devbox"),
                "devbox (Lab)".to_string()
            ),
        ])
    );
}
