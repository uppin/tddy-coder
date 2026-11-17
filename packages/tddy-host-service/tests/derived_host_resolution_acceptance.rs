//! `HostServiceImpl::resolve_derived_host`: a derived host's id, named by an orchestrator's
//! `start_session`, resolved for the caller into the parent a start is addressed to and the
//! target its agent runs on (`#remote-shell` 11/11).
//!
//! The same stand-ins as the listings' own tests: an operator's home on a temp path for the SSH
//! aliases, the caller's vault and their deployments for the Coder workspaces, a fixed reading of
//! the `coder` CLI. Nothing is connected to.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-16-remote-shell-orchestrator-target-start.md

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_credentials::SecretString;
use tddy_host_service::coder_derived_hosts::{CoderAccountRef, CoderAccountSource};
use tddy_host_service::coder_workspaces::{CoderCredentialSource, CoderCredentials};
use tddy_host_service::host_private_key::HostUserFiles;
use tddy_host_service::host_tooling::{
    CoderCliStatus, GitIdentity, GithubCliStatus, HostTooling, HostToolingProbe, ProbeOutcome,
};
use tddy_host_service::ssh_agent::AgentStatus;
use tddy_host_service::test_util::{
    a_coder_vault_of, a_coder_workspace, coder_deployments, test_service, TEST_TOKEN, TEST_USER,
};
use tddy_host_service::{DerivedHostResolutionError, HostServiceImpl, ResolvedDerivedHost};
use tddy_rpc::Request;
use tddy_service::proto::host::{HostService, ListDerivedHostsRequest};
use tddy_service::proto::session::{session_target, CoderTarget, SessionTarget, SshTarget};

/// The signed-in person's account on their employer's deployment.
const WORK: &str = "coder-3f1c9a52@coder.example.com";
const WORK_URL: &str = "https://coder.example.com";

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

/// A tooling probe on a host that has the `coder` CLI.
struct AHostWithTheCoderCli;

impl HostToolingProbe for AHostWithTheCoderCli {
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
            coder_cli: CoderCliStatus {
                installed: true,
                version: Some("v2.33.5".to_string()),
            },
        }
    }
}

/// A daemon whose operator reaches `buildbox` over SSH and whose signed-in person has a Work
/// account with a running `devbox` and a stopped `gpu-rig`.
struct ADaemon {
    service: HostServiceImpl,
    _home: tempfile::TempDir,
    _tddy_data: tempfile::TempDir,
}

fn a_daemon_reaching_buildbox_and_adas_workspaces() -> ADaemon {
    let home_storage = tempfile::tempdir().expect("operator home storage");
    let home = home_storage.path().to_path_buf();
    std::fs::create_dir_all(home.join(".ssh")).expect("the operator has ~/.ssh");
    std::fs::write(home.join(".ssh").join("config"), "Host buildbox\n").expect("ssh config");
    let tddy_data = tempfile::tempdir().expect("tddy data dir");
    let vault = Arc::new(a_coder_vault_of(TEST_USER).holding(
        CoderAccountRef::new(WORK, "Work"),
        CoderCredentials::new(WORK_URL, SecretString::new("token-for-work".to_string())),
    ));
    let service = test_service(tddy_data.path())
        .with_host_user_files(Arc::new(AnOperatorHome { home }))
        .with_host_tooling(Arc::new(AHostWithTheCoderCli))
        .with_coder_accounts(Arc::clone(&vault) as Arc<dyn CoderAccountSource>)
        .with_coder_credentials(vault as Arc<dyn CoderCredentialSource>)
        .with_coder_workspaces(Arc::new(coder_deployments().listing(
            WORK_URL,
            vec![
                a_coder_workspace("ada", "devbox", "running"),
                a_coder_workspace("ada", "gpu-rig", "stopped"),
            ],
        )));
    ADaemon {
        service,
        _home: home_storage,
        _tddy_data: tddy_data,
    }
}

impl ADaemon {
    /// The ids this daemon lists, in the listing's order — what an agent is told it can name.
    async fn listed_ids(&self) -> Vec<String> {
        self.service
            .list_derived_hosts(Request::direct(ListDerivedHostsRequest {
                session_token: TEST_TOKEN.to_string(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("the hosts are listed")
            .into_inner()
            .hosts
            .into_iter()
            .map(|host| host.id)
            .collect()
    }

    /// The id the listing gives the host whose label starts with `label`.
    async fn id_of(&self, label: &str) -> String {
        let listed = self
            .service
            .list_derived_hosts(Request::direct(ListDerivedHostsRequest {
                session_token: TEST_TOKEN.to_string(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("the hosts are listed")
            .into_inner();
        listed
            .hosts
            .into_iter()
            .find(|host| host.label.starts_with(label))
            .unwrap_or_else(|| panic!("{label} is listed"))
            .id
    }

    async fn resolve(&self, id: &str) -> Result<ResolvedDerivedHost, DerivedHostResolutionError> {
        self.service.resolve_derived_host(TEST_TOKEN, id).await
    }
}

#[tokio::test]
async fn a_listed_derived_host_resolves_to_its_parent_and_target() {
    // Given a daemon that reaches buildbox and ada's running devbox
    let daemon = a_daemon_reaching_buildbox_and_adas_workspaces();
    let buildbox = daemon.id_of("buildbox").await;
    let devbox = daemon.id_of("devbox").await;

    // When each is resolved for the caller
    let resolved = (
        daemon.resolve(&buildbox).await,
        daemon.resolve(&devbox).await,
    );

    // Then each is the start's address: this daemon as the parent, the host as the target
    let parent = buildbox
        .split('/')
        .next()
        .expect("an id starts with its parent")
        .to_string();
    assert_eq!(
        resolved,
        (
            Ok(ResolvedDerivedHost {
                parent_instance_id: parent.clone(),
                target: SessionTarget {
                    kind: Some(session_target::Kind::Ssh(SshTarget {
                        config_host: "buildbox".to_string(),
                    })),
                },
            }),
            Ok(ResolvedDerivedHost {
                parent_instance_id: parent,
                target: SessionTarget {
                    kind: Some(session_target::Kind::Coder(CoderTarget {
                        account_id: WORK.to_string(),
                        owner: "ada".to_string(),
                        workspace: "devbox".to_string(),
                        agent: String::new(),
                    })),
                },
            }),
        )
    );
}

#[tokio::test]
async fn an_unknown_derived_host_is_refused_naming_the_listed_ones() {
    // Given
    let daemon = a_daemon_reaching_buildbox_and_adas_workspaces();

    // When the agent names the alias it guessed from a wildcard
    let resolved = daemon.resolve("workstation-1/ssh/coder.remote-dev").await;

    // Then the refusal carries every id it could have named instead
    assert_eq!(
        resolved,
        Err(DerivedHostResolutionError::Unknown {
            id: "workstation-1/ssh/coder.remote-dev".to_string(),
            listed: daemon.listed_ids().await,
        })
    );
}

#[tokio::test]
async fn an_unusable_derived_host_is_refused_with_its_reason() {
    // Given ada's gpu-rig, which the deployment reports stopped
    let daemon = a_daemon_reaching_buildbox_and_adas_workspaces();
    let gpu_rig = daemon.id_of("gpu-rig").await;

    // When
    let resolved = daemon.resolve(&gpu_rig).await;

    // Then it is refused with the listing's own reason, not started on a stopped machine
    assert_eq!(
        resolved,
        Err(DerivedHostResolutionError::Unusable {
            id: gpu_rig,
            reason: "the workspace is stopped".to_string(),
        })
    );
}
