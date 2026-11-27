//! Acceptance: an orchestrator's start naming a derived host, as the daemon answers it
//! (`#remote-shell` 11/11).
//!
//! Three behaviours of the same resolution the project-home start handler performs
//! (`DaemonProjectHome::start_session` resolving `StartSessionArgs.target` through
//! `HostServiceImpl::resolve_derived_host` before it builds the request):
//!
//! - a start naming a listed SSH host runs on its parent, with the host's target — the request
//!   the handler forwards once resolution has answered, driven here against the real
//!   `DaemonSessionHost` as in `derived_host_start_acceptance`;
//! - a Coder host is resolved with the **caller's own** account: two callers sharing one daemon
//!   and one OS user each get their own workspace's target, never the other's;
//! - an unusable host is refused at resolution, with its reason, before any start is issued.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-16-remote-shell-orchestrator-target-start.md

use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_credentials::SecretString;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_host_service::coder_derived_hosts::{CoderAccountRef, CoderAccountSource};
use tddy_host_service::coder_workspaces::{
    CoderCredentialError, CoderCredentialSource, CoderCredentials, CoderWorkspaceSource,
};
use tddy_host_service::host_tooling::{
    CoderCliStatus, GitIdentity, GithubCliStatus, HostTooling, HostToolingProbe, ProbeOutcome,
};
use tddy_host_service::ssh_agent::AgentStatus;
use tddy_host_service::test_util::{a_coder_workspace, coder_deployments};
use tddy_host_service::{DerivedHostResolutionError, HostServiceImpl, ResolvedDerivedHost};
use tddy_rpc::Request;
use tddy_service::proto::host::{HostService as HostServiceTrait, ListDerivedHostsRequest};
use tddy_service::proto::session::{
    session_target, SessionService as SessionServiceTrait, StartSessionRequest,
};

mod agent_registry_support;
mod cli_harness_support;
mod derived_host_support;
use cli_harness_support::{current_os_user, VALID_TOKEN};
use derived_host_support::{
    a_daemon_reaching_an_ssh_host, ADaemonReachingAnSshHost, AnOperatorHome, BRANCH,
    DERIVED_HOST_ALIAS, MODEL, SESSION_TYPE,
};

const TEST_PROJECT_ID: &str = "orchestrator-target-project";

impl ADaemonReachingAnSshHost {
    /// Start an `acme-cli` session addressed as `resolved` directs — its parent as the daemon,
    /// its target as the target — the request the project-home handler forwards for a start
    /// whose `target` named this host.
    async fn start_resolved(&self, resolved: &ResolvedDerivedHost) -> String {
        self.sessions
            .start_session(Request::direct(StartSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                project_id: TEST_PROJECT_ID.to_string(),
                session_type: SESSION_TYPE.to_string(),
                model: MODEL.to_string(),
                branch_worktree_intent: "new_branch_from_base".to_string(),
                new_branch_name: BRANCH.to_string(),
                daemon_instance_id: resolved.parent_instance_id.clone(),
                target: Some(resolved.target.clone()),
                ..Default::default()
            }))
            .await
            .expect("a start addressed by a resolved derived host succeeds")
            .into_inner()
            .session_id
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_start_naming_a_derived_host_runs_on_its_parent_with_the_hosts_target() {
    // Given a daemon that lists buildbox among the hosts it reaches
    let daemon = a_daemon_reaching_an_ssh_host(TEST_PROJECT_ID);
    let listed = daemon.the_listed_derived_host(DERIVED_HOST_ALIAS).await;

    // When the listed id is resolved for the caller, as a start naming it is
    let resolved = daemon
        .hosts
        .resolve_derived_host(VALID_TOKEN, &listed.id)
        .await
        .expect("a listed derived host resolves for the caller");

    // Then resolution answers the listed host's own parent and target, nothing composed
    assert_eq!(
        (resolved.parent_instance_id.clone(), resolved.target.clone()),
        (
            listed.parent_instance_id.clone(),
            listed
                .target
                .clone()
                .expect("a listed host carries its target"),
        )
    );

    // When a session is started addressed as the resolution directs
    let session_id = daemon.start_resolved(&resolved).await;
    let agent_runs_in = daemon.where_the_agent_runs(&session_id).await;
    let recorded_target = daemon.recorded_target_of(&session_id);
    daemon.stop(&session_id).await;

    // Then the agent ran in the session's worktree on that host, and the session records it
    assert_eq!(
        (agent_runs_in, recorded_target),
        (
            daemon.machine.worktree_of(&session_id),
            Some(tddy_core::SessionTarget::ssh(DERIVED_HOST_ALIAS)),
        )
    );
}

// ---------------------------------------------------------------------------
// The caller's own Coder account — resolution at `HostServiceImpl` level, over
// per-caller doubles: one vault serves one person, so the double is keyed by
// subject, exactly as a daemon wired to every user's vault is.
// ---------------------------------------------------------------------------

/// Ada's session token, and Grace's: two callers of one daemon.
const TOKEN_ADA: &str = "token-ada";
const TOKEN_GRACE: &str = "token-grace";
/// Each caller's own Coder account, on their own deployment.
const ADA_ACCOUNT: &str = "ada-work@coder.example.com";
const GRACE_ACCOUNT: &str = "grace-work@coder.example.com";
const ADA_URL: &str = "https://ada.coder.example.com";
const GRACE_URL: &str = "https://grace.coder.example.com";

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
            ios_simulators: Vec::new(),
        }
    }
}

/// Every caller's Coder accounts and credentials, each served only to its owner: the accounts
/// listing and the credential read a daemon wired to the vaults answers, keyed by subject
/// because one vault serves one person.
struct TheCallersCoderAccounts {
    accounts: Vec<(String, CoderAccountRef, CoderCredentials)>,
}

impl CoderAccountSource for TheCallersCoderAccounts {
    fn accounts_of(&self, subject: &str) -> Result<Vec<CoderAccountRef>, CoderCredentialError> {
        Ok(self
            .accounts
            .iter()
            .filter(|(owner, _, _)| owner == subject)
            .map(|(_, account, _)| account.clone())
            .collect())
    }
}

impl CoderCredentialSource for TheCallersCoderAccounts {
    fn credentials_for(
        &self,
        subject: &str,
        account_id: &str,
    ) -> Result<CoderCredentials, CoderCredentialError> {
        self.accounts
            .iter()
            .find(|(owner, account, _)| owner == subject && account.account_id == account_id)
            .map(|(_, _, credentials)| credentials.clone())
            .ok_or_else(|| CoderCredentialError::NoSuchAccount(account_id.to_string()))
    }
}

/// One caller of the Coder world: their token, who it signs in as, their account and what its
/// deployment lists for them.
struct ACoderCaller {
    token: &'static str,
    subject: &'static str,
    account_id: &'static str,
    url: &'static str,
    workspaces: Vec<tddy_host_service::coder_workspaces::CoderWorkspace>,
}

/// A daemon whose callers each have their own Coder account: both map to this machine's OS
/// user, its operator home has no SSH config (so no SSH host pollutes the listing), and each
/// deployment answers only its own caller's listing.
struct ACoderWorld {
    service: HostServiceImpl,
    _config_dir: tempfile::TempDir,
    _tddy_data: tempfile::TempDir,
    _home: tempfile::TempDir,
}

fn a_coder_world(callers: Vec<ACoderCaller>) -> ACoderWorld {
    let os_user = current_os_user();
    let config_dir = tempfile::tempdir().expect("daemon config storage");
    std::fs::write(
        config_dir.path().join("daemon.yaml"),
        "allowed_tools:\n  - path: /bin/true\n    label: true\n",
    )
    .expect("daemon config");
    let users_yaml = callers
        .iter()
        .map(|caller| {
            format!(
                "- github_user: \"{}\"\n  os_user: \"{os_user}\"\n",
                caller.subject
            )
        })
        .collect::<String>();
    std::fs::write(config_dir.path().join("users.yaml"), users_yaml).expect("daemon users");
    let config = DaemonConfig::load(&config_dir.path().join("daemon.yaml"))
        .expect("the daemon config loads");

    let tokens: Vec<(String, String)> = callers
        .iter()
        .map(|caller| (caller.token.to_string(), caller.subject.to_string()))
        .collect();
    let resolver = Arc::new(move |token: &str| {
        tokens
            .iter()
            .find(|(known, _)| known == token)
            .map(|(_, subject)| subject.clone())
    });

    let accounts = Arc::new(TheCallersCoderAccounts {
        accounts: callers
            .iter()
            .map(|caller| {
                (
                    caller.subject.to_string(),
                    CoderAccountRef::new(caller.account_id, caller.subject),
                    CoderCredentials::new(
                        caller.url,
                        SecretString::new(format!("token-for-{}", caller.subject)),
                    ),
                )
            })
            .collect(),
    });
    let mut deployments = coder_deployments();
    for caller in &callers {
        deployments = deployments.listing(caller.url, caller.workspaces.clone());
    }

    let home = tempfile::tempdir().expect("operator home storage");
    let tddy_data = tempfile::tempdir().expect("tddy data dir");
    let service = HostServiceImpl::new(config, tddy_data.path(), resolver)
        .with_host_user_files(Arc::new(AnOperatorHome {
            home: home.path().to_path_buf(),
        }))
        .with_host_tooling(Arc::new(AHostWithTheCoderCli))
        .with_coder_accounts(Arc::clone(&accounts) as Arc<dyn CoderAccountSource>)
        .with_coder_credentials(accounts as Arc<dyn CoderCredentialSource>)
        .with_coder_workspaces(Arc::new(deployments) as Arc<dyn CoderWorkspaceSource>);
    ACoderWorld {
        service,
        _config_dir: config_dir,
        _tddy_data: tddy_data,
        _home: home,
    }
}

impl ACoderWorld {
    /// The id `token`'s listing gives the host whose label starts with `label`.
    async fn listed_id(&self, token: &str, label: &str) -> String {
        self.service
            .list_derived_hosts(Request::direct(ListDerivedHostsRequest {
                session_token: token.to_string(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("the hosts are listed")
            .into_inner()
            .hosts
            .into_iter()
            .find(|host| host.label.starts_with(label))
            .unwrap_or_else(|| panic!("{label} is listed"))
            .id
    }
}

#[tokio::test]
async fn a_coder_derived_host_is_resolved_with_the_callers_own_account() {
    // Given one daemon whose two callers each have their own Coder account, and each account's
    // deployment lists that caller's own running devbox
    let world = a_coder_world(vec![
        ACoderCaller {
            token: TOKEN_ADA,
            subject: "ada",
            account_id: ADA_ACCOUNT,
            url: ADA_URL,
            workspaces: vec![a_coder_workspace("ada", "devbox", "running")],
        },
        ACoderCaller {
            token: TOKEN_GRACE,
            subject: "grace",
            account_id: GRACE_ACCOUNT,
            url: GRACE_URL,
            workspaces: vec![a_coder_workspace("grace", "devbox", "running")],
        },
    ]);
    let adas_devbox = world.listed_id(TOKEN_ADA, "devbox").await;
    let graces_devbox = world.listed_id(TOKEN_GRACE, "devbox").await;

    // When each caller resolves their own listed devbox
    let resolved = (
        world
            .service
            .resolve_derived_host(TOKEN_ADA, &adas_devbox)
            .await
            .expect("ada's devbox resolves for ada"),
        world
            .service
            .resolve_derived_host(TOKEN_GRACE, &graces_devbox)
            .await
            .expect("grace's devbox resolves for grace"),
    );

    // Then each target names the caller's own account and the caller as owner — resolution read
    // the caller's vault, not a shared or the other caller's one
    let coder_target_of = |resolved: &ResolvedDerivedHost| match &resolved.target.kind {
        Some(session_target::Kind::Coder(target)) => (
            target.account_id.clone(),
            target.owner.clone(),
            target.workspace.clone(),
        ),
        other => panic!("a Coder host resolves to a Coder target, not {other:?}"),
    };
    assert_eq!(
        (coder_target_of(&resolved.0), coder_target_of(&resolved.1)),
        (
            (
                ADA_ACCOUNT.to_string(),
                "ada".to_string(),
                "devbox".to_string()
            ),
            (
                GRACE_ACCOUNT.to_string(),
                "grace".to_string(),
                "devbox".to_string()
            ),
        )
    );
}

#[tokio::test]
async fn a_start_naming_an_unusable_derived_host_is_refused_with_its_reason() {
    // Given ada's gpu-rig, which her deployment reports stopped
    let world = a_coder_world(vec![ACoderCaller {
        token: TOKEN_ADA,
        subject: "ada",
        account_id: ADA_ACCOUNT,
        url: ADA_URL,
        workspaces: vec![a_coder_workspace("ada", "gpu-rig", "stopped")],
    }]);
    let gpu_rig = world.listed_id(TOKEN_ADA, "gpu-rig").await;

    // When it is resolved, as a start naming it resolves it before any request is built
    let resolved = world
        .service
        .resolve_derived_host(TOKEN_ADA, &gpu_rig)
        .await;

    // Then the refusal is the resolution's own, carrying the listing's reason. No StartSession
    // follows a refused resolution — the project-home handler builds its request only from a
    // resolved host — so no session is started on a stopped workspace; the error is the whole
    // outcome asserted here.
    assert_eq!(
        resolved,
        Err(DerivedHostResolutionError::Unusable {
            id: gpu_rig,
            reason: "the workspace is stopped".to_string(),
        })
    );
}
