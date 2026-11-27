//! `ListCoderWorkspaces` and the `coder` CLI block of `GetHostTooling`: the real handler, a
//! stand-in for the vault that holds a Coder account, and a stand-in for the deployment. No Coder
//! deployment is reached; the HTTP request itself is the daemon's and is tested beside it.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-coder-shell.md

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_credentials::SecretString;
use tddy_host_service::coder_workspaces::{
    CoderCredentialError, CoderCredentialSource, CoderCredentials, CoderWorkspace,
    CoderWorkspaceListError, CoderWorkspaceSource,
};
use tddy_host_service::host_tooling::{
    CoderCliStatus, GitIdentity, GithubCliStatus, HostTooling, HostToolingProbe, ProbeOutcome,
};
use tddy_host_service::ssh_agent::AgentStatus;
use tddy_host_service::test_util::{test_service, TEST_TOKEN, TEST_USER};
use tddy_host_service::HostServiceImpl;
use tddy_rpc::{Code, Request};
use tddy_service::proto::host::{
    CoderWorkspace as ListedWorkspace, GetHostToolingRequest, HostCoderCli, HostService,
    ListCoderWorkspacesRequest, ListCoderWorkspacesResponse, ProbeOutcome as WireProbeOutcome,
};

/// An empty `daemon_instance_id` is the protocol's spelling for "the daemon serving this call".
const THIS_DAEMON: &str = "";
const AN_UNKNOWN_HOST: &str = "daemon-on-some-other-machine";
const ADAS_ACCOUNT: &str = "coder-3f1c9a52@coder.example.com";
const ADAS_DEPLOYMENT: &str = "https://coder.example.com";
const ADAS_TOKEN: &str = "adas-coder-session-token";
/// A listing is one vault read and one request; this only turns a hang into a failure.
const LISTING_WINDOW: Duration = Duration::from_secs(2);

fn adas_credentials() -> CoderCredentials {
    CoderCredentials::new(ADAS_DEPLOYMENT, SecretString::new(ADAS_TOKEN))
}

/// The vault side: answers every read with one fixed outcome, and records who asked for what.
struct AVault {
    answer: Result<CoderCredentials, CoderCredentialError>,
    asked: Mutex<Vec<(String, String)>>,
}

impl CoderCredentialSource for AVault {
    fn credentials_for(
        &self,
        subject: &str,
        account_id: &str,
    ) -> Result<CoderCredentials, CoderCredentialError> {
        self.asked
            .lock()
            .expect("the recorder is only held to push")
            .push((subject.to_string(), account_id.to_string()));
        self.answer.clone()
    }
}

fn a_vault_answering(answer: Result<CoderCredentials, CoderCredentialError>) -> Arc<AVault> {
    Arc::new(AVault {
        answer,
        asked: Mutex::new(Vec::new()),
    })
}

fn an_open_vault_holding_adas_account() -> Arc<AVault> {
    a_vault_answering(Ok(adas_credentials()))
}

/// The deployment side: answers every listing with one fixed outcome, and records the
/// credentials it was asked with.
struct ADeployment {
    answer: Result<Vec<CoderWorkspace>, CoderWorkspaceListError>,
    asked_with: Mutex<Vec<CoderCredentials>>,
}

#[async_trait]
impl CoderWorkspaceSource for ADeployment {
    async fn workspaces(
        &self,
        credentials: &CoderCredentials,
    ) -> Result<Vec<CoderWorkspace>, CoderWorkspaceListError> {
        self.asked_with
            .lock()
            .expect("the recorder is only held to push")
            .push(credentials.clone());
        self.answer.clone()
    }
}

fn a_deployment_answering(
    answer: Result<Vec<CoderWorkspace>, CoderWorkspaceListError>,
) -> Arc<ADeployment> {
    Arc::new(ADeployment {
        answer,
        asked_with: Mutex::new(Vec::new()),
    })
}

fn a_deployment_listing(workspaces: Vec<CoderWorkspace>) -> Arc<ADeployment> {
    a_deployment_answering(Ok(workspaces))
}

fn adas_workspace(name: &str, status: &str) -> CoderWorkspace {
    CoderWorkspace {
        owner: "ada".to_string(),
        name: name.to_string(),
        status: status.to_string(),
    }
}

fn listed(name: &str, status: &str, running: bool) -> ListedWorkspace {
    ListedWorkspace {
        owner: "ada".to_string(),
        name: name.to_string(),
        status: status.to_string(),
        running,
    }
}

fn a_failed_listing(reason: &str) -> ListCoderWorkspacesResponse {
    ListCoderWorkspacesResponse {
        outcome: WireProbeOutcome::Failed as i32,
        failure_reason: reason.to_string(),
        workspaces: Vec::new(),
    }
}

/// A host service reading accounts from `vault` and asking `deployment`, with the directory that
/// backs it.
fn a_host_with(
    vault: &Arc<AVault>,
    deployment: &Arc<ADeployment>,
) -> (tempfile::TempDir, HostServiceImpl) {
    let tddy_data = tempfile::tempdir().expect("tddy data dir");
    let service = test_service(tddy_data.path())
        .with_coder_credentials(Arc::clone(vault) as Arc<dyn CoderCredentialSource>)
        .with_coder_workspaces(Arc::clone(deployment) as Arc<dyn CoderWorkspaceSource>);
    (tddy_data, service)
}

fn adas_listing_request() -> ListCoderWorkspacesRequest {
    ListCoderWorkspacesRequest {
        session_token: TEST_TOKEN.to_string(),
        daemon_instance_id: THIS_DAEMON.to_string(),
        account_id: ADAS_ACCOUNT.to_string(),
    }
}

async fn workspaces_listed_by(
    service: &HostServiceImpl,
    request: ListCoderWorkspacesRequest,
) -> Result<ListCoderWorkspacesResponse, (Code, String)> {
    tokio::time::timeout(
        LISTING_WINDOW,
        service.list_coder_workspaces(Request::direct(request)),
    )
    .await
    .expect("a listing must not hang")
    .map(|response| response.into_inner())
    .map_err(|status| (status.code, status.message))
}

#[tokio::test]
async fn an_accounts_workspaces_list_with_their_status() {
    // Given Ada's Coder account in her open vault, and a deployment where she has a running
    // workspace and a stopped one
    let vault = an_open_vault_holding_adas_account();
    let deployment = a_deployment_listing(vec![
        adas_workspace("devbox", "running"),
        adas_workspace("scratch", "stopped"),
    ]);
    let (_storage, service) = a_host_with(&vault, &deployment);

    // When she lists that account's workspaces
    let listing = workspaces_listed_by(&service, adas_listing_request()).await;

    // Then both are listed, in the deployment's order, each with its status and whether it runs
    assert_eq!(
        listing,
        Ok(ListCoderWorkspacesResponse {
            outcome: WireProbeOutcome::Ok as i32,
            failure_reason: String::new(),
            workspaces: vec![
                listed("devbox", "running", true),
                listed("scratch", "stopped", false),
            ],
        })
    );
    // And the account was read as the session's own subject, and the deployment asked with
    // exactly what the vault holds
    assert_eq!(
        *vault.asked.lock().unwrap(),
        vec![(TEST_USER.to_string(), ADAS_ACCOUNT.to_string())]
    );
    assert_eq!(
        *deployment.asked_with.lock().unwrap(),
        vec![adas_credentials()]
    );
}

#[tokio::test]
async fn an_account_with_no_workspaces_lists_empty_and_ok() {
    // Given Ada's account on a deployment where she has no workspace
    let vault = an_open_vault_holding_adas_account();
    let deployment = a_deployment_listing(Vec::new());
    let (_storage, service) = a_host_with(&vault, &deployment);

    // When she lists that account's workspaces
    let listing = workspaces_listed_by(&service, adas_listing_request()).await;

    // Then the listing succeeded and is empty — not a failure
    assert_eq!(
        listing,
        Ok(ListCoderWorkspacesResponse {
            outcome: WireProbeOutcome::Ok as i32,
            failure_reason: String::new(),
            workspaces: Vec::new(),
        })
    );
}

#[tokio::test]
async fn a_locked_vault_fails_the_listing_and_says_so() {
    // Given a vault that is locked, and a deployment that would list a workspace
    let vault = a_vault_answering(Err(CoderCredentialError::VaultLocked));
    let deployment = a_deployment_listing(vec![adas_workspace("devbox", "running")]);
    let (_storage, service) = a_host_with(&vault, &deployment);

    // When the account's workspaces are listed
    let listing = workspaces_listed_by(&service, adas_listing_request()).await;

    // Then the listing failed, saying the vault is locked
    assert_eq!(
        listing,
        Ok(a_failed_listing(
            "the vault holding this Coder account is locked: unlock it and list again"
        ))
    );
    // And the deployment was never asked: there was no token to ask it with
    assert_eq!(
        *deployment.asked_with.lock().unwrap(),
        Vec::<CoderCredentials>::new()
    );
}

#[tokio::test]
async fn an_account_the_vault_does_not_hold_fails_the_listing_and_names_it() {
    // Given an open vault that holds no such Coder account
    let vault = a_vault_answering(Err(CoderCredentialError::NoSuchAccount(
        ADAS_ACCOUNT.to_string(),
    )));
    let deployment = a_deployment_listing(Vec::new());
    let (_storage, service) = a_host_with(&vault, &deployment);

    // When that account's workspaces are listed
    let listing = workspaces_listed_by(&service, adas_listing_request()).await;

    // Then the listing failed, naming the account — not an empty list of workspaces
    assert_eq!(
        listing,
        Ok(a_failed_listing(
            "the vault holds no Coder account coder-3f1c9a52@coder.example.com"
        ))
    );
}

#[tokio::test]
async fn a_token_the_deployment_rejects_fails_the_listing_and_says_so() {
    // Given Ada's account, and a deployment that no longer accepts its token
    let vault = an_open_vault_holding_adas_account();
    let deployment = a_deployment_answering(Err(CoderWorkspaceListError::Rejected(
        "API key is invalid.".to_string(),
    )));
    let (_storage, service) = a_host_with(&vault, &deployment);

    // When the account's workspaces are listed
    let listing = workspaces_listed_by(&service, adas_listing_request()).await;

    // Then the listing failed, saying the token was rejected, in the deployment's words
    assert_eq!(
        listing,
        Ok(a_failed_listing(
            "the Coder deployment rejected this account's token: API key is invalid."
        ))
    );
}

#[tokio::test]
async fn an_unreachable_deployment_fails_the_listing_and_says_so() {
    // Given Ada's account, and a deployment nothing answers at
    let vault = an_open_vault_holding_adas_account();
    let deployment = a_deployment_answering(Err(CoderWorkspaceListError::Unreachable(
        "connection refused".to_string(),
    )));
    let (_storage, service) = a_host_with(&vault, &deployment);

    // When the account's workspaces are listed
    let listing = workspaces_listed_by(&service, adas_listing_request()).await;

    // Then the listing failed, saying the deployment could not be reached
    assert_eq!(
        listing,
        Ok(a_failed_listing(
            "the Coder deployment could not be reached: connection refused"
        ))
    );
}

#[tokio::test]
async fn a_listing_without_a_session_is_refused_before_the_vault_is_read() {
    // Given Ada's account in her open vault
    let vault = an_open_vault_holding_adas_account();
    let deployment = a_deployment_listing(Vec::new());
    let (_storage, service) = a_host_with(&vault, &deployment);

    // When a caller with no session lists it
    let refused = workspaces_listed_by(
        &service,
        ListCoderWorkspacesRequest {
            session_token: "not-a-session".to_string(),
            ..adas_listing_request()
        },
    )
    .await;

    // Then the call is refused as unauthenticated, and nothing was read for it
    assert_eq!(
        refused.map_err(|(code, _message)| code),
        Err(Code::Unauthenticated)
    );
    assert_eq!(*vault.asked.lock().unwrap(), Vec::<(String, String)>::new());
}

#[tokio::test]
async fn a_listing_addressed_to_a_host_this_daemon_does_not_know_is_refused() {
    // Given Ada's account in her open vault on this daemon
    let vault = an_open_vault_holding_adas_account();
    let deployment = a_deployment_listing(Vec::new());
    let (_storage, service) = a_host_with(&vault, &deployment);

    // When the listing is addressed to a daemon that is not in the room
    let refused = workspaces_listed_by(
        &service,
        ListCoderWorkspacesRequest {
            daemon_instance_id: AN_UNKNOWN_HOST.to_string(),
            ..adas_listing_request()
        },
    )
    .await;

    // Then it is a bad address — not this daemon's vault answering under another host's name
    assert_eq!(
        refused.map_err(|(code, _message)| code),
        Err(Code::InvalidArgument)
    );
    assert_eq!(*vault.asked.lock().unwrap(), Vec::<(String, String)>::new());
}

/// A tooling probe that answers with one fixed reading.
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
            ios_simulators: Vec::new(),
        }
    }
}

async fn the_coder_cli_reported_by_a_host_where_it_is(
    status: CoderCliStatus,
) -> Option<HostCoderCli> {
    let tddy_data = tempfile::tempdir().expect("tddy data dir");
    let service =
        test_service(tddy_data.path()).with_host_tooling(Arc::new(AHostWhoseCoderCliIs(status)));
    service
        .get_host_tooling(Request::direct(GetHostToolingRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: THIS_DAEMON.to_string(),
            github_hostname: String::new(),
        }))
        .await
        .expect("a probed host is a tooling report")
        .into_inner()
        .coder_cli
}

#[tokio::test]
async fn the_tooling_report_says_whether_the_coder_cli_is_installed() {
    // Given one host with the coder CLI on its path, and one without

    // When each host's tooling is reported
    let with_the_cli = the_coder_cli_reported_by_a_host_where_it_is(CoderCliStatus {
        installed: true,
        version: Some("v2.16.0".to_string()),
    })
    .await;
    let without_the_cli =
        the_coder_cli_reported_by_a_host_where_it_is(CoderCliStatus::not_installed()).await;

    // Then the first says it is installed and at which version, the second that it is not
    assert_eq!(
        (with_the_cli, without_the_cli),
        (
            Some(HostCoderCli {
                installed: true,
                version: "v2.16.0".to_string(),
            }),
            Some(HostCoderCli {
                installed: false,
                version: String::new(),
            }),
        )
    );
}
