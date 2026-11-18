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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tddy_testing_commons::remote_target::{write_executable, StandInSshTarget};

use pretty_assertions::assert_eq;
use tddy_core::session_metadata::read_session_metadata;
use tddy_credentials::SecretString;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_host_service::coder_derived_hosts::{CoderAccountRef, CoderAccountSource};
use tddy_host_service::coder_workspaces::{
    CoderCredentialError, CoderCredentialSource, CoderCredentials, CoderWorkspaceSource,
};
use tddy_host_service::host_private_key::HostUserFiles;
use tddy_host_service::host_tooling::{
    CoderCliStatus, GitIdentity, GithubCliStatus, HostTooling, HostToolingProbe, ProbeOutcome,
};
use tddy_host_service::ssh_agent::AgentStatus;
use tddy_host_service::test_util::{a_coder_workspace, coder_deployments};
use tddy_host_service::{DerivedHostResolutionError, HostServiceImpl, ResolvedDerivedHost};
use tddy_rpc::Request;
use tddy_service::proto::host::{HostService as HostServiceTrait, ListDerivedHostsRequest};
use tddy_service::proto::session::{
    session_target, SessionService as SessionServiceTrait, SignalSessionRequest,
    StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

mod agent_registry_support;
mod cli_harness_support;
use agent_registry_support::builtin_yaml_as;
use cli_harness_support::{
    a_daemon_config, create_test_repo_with_origin, current_os_user, register_project, session_host,
    VALID_TOKEN,
};

const TEST_PROJECT_ID: &str = "orchestrator-target-project";
/// The agent: `claude.yaml` under this id, so its sessions launch through the generic CLI path.
const AGENT_ID: &str = "acme";
const SESSION_TYPE: &str = "acme-cli";
const MODEL: &str = "acme-1";
/// The branch a start names: `claude.yaml` declares `branch_name_required`.
const BRANCH: &str = "acme-work";
/// The ssh `Host` alias the derived host is known by.
const DERIVED_HOST_ALIAS: &str = "buildbox";

/// A safety net for an agent to be observed on the host, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(20);

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

/// A stub agent: appends one line per run — its pid and its working directory — to `runs`, prints
/// a banner, then stays up until it is hung up.
fn a_stub_agent(runs: &Path) -> String {
    format!(
        r#"#!/bin/sh
printf '%s\t%s\n' "$$" "$(/bin/pwd -P)" >> '{runs}'
printf 'ACME UP\n'
while :; do /bin/sleep 0.1; done
"#,
        runs = runs.display()
    )
}

/// The host service of the daemon configured by `config`, reading its operator's files from
/// `operator_home`, and serving [`VALID_TOKEN`] as the session host does.
fn a_host_service(
    config: DaemonConfig,
    tddy_data_dir: &Path,
    operator_home: &Path,
) -> HostServiceImpl {
    let resolved_user = current_os_user();
    HostServiceImpl::new(
        config,
        tddy_data_dir,
        Arc::new(move |token| (token == VALID_TOKEN).then(|| resolved_user.clone())),
    )
    .with_host_user_files(Arc::new(AnOperatorHome {
        home: operator_home.to_path_buf(),
    }))
}

/// One daemon — its host service and its session host over one config — with a registered
/// project, the `acme` agent installed, and an operator whose SSH config names
/// [`DERIVED_HOST_ALIAS`], a machine it reaches.
struct ADaemonReachingAnSshHost {
    hosts: HostServiceImpl,
    sessions: Arc<DaemonSessionHost>,
    manager: Arc<CliSessionManager>,
    sessions_base: PathBuf,
    runs: PathBuf,
    machine: StandInSshTarget,
    _repo_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
    _operator_home: tempfile::TempDir,
}

fn a_daemon_reaching_an_ssh_host() -> ADaemonReachingAnSshHost {
    let machine = StandInSshTarget::behind_ssh();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_dir = tempfile::tempdir().unwrap();
    let sessions_base = std::fs::canonicalize(sessions_dir.path()).unwrap();
    register_project(
        &sessions_base.join("projects"),
        repo_dir.path(),
        TEST_PROJECT_ID,
    );
    let tools = sessions_base.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(tools.join("acme.yaml"), builtin_yaml_as("claude", AGENT_ID)).unwrap();

    let stub_dir = tempfile::tempdir().unwrap();
    let stub_bin = std::fs::canonicalize(stub_dir.path()).unwrap();
    let runs = stub_bin.join("runs");
    let stub = stub_bin.join(AGENT_ID);
    write_executable(&stub, &a_stub_agent(&runs));
    // A target start runs the agent's bare command, found on the target's own `PATH`.
    write_executable(&machine.agents_dir().join(AGENT_ID), &a_stub_agent(&runs));

    let operator_home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(operator_home.path().join(".ssh")).unwrap();
    std::fs::write(
        operator_home.path().join(".ssh").join("config"),
        format!("Host {DERIVED_HOST_ALIAS}\n    HostName 10.0.0.7\n"),
    )
    .unwrap();

    let config = a_daemon_config();
    let hosts = a_host_service(config.clone(), &sessions_base, operator_home.path());
    let manager = Arc::new(CliSessionManager::new());
    let sessions = Arc::new(session_host(
        config,
        SESSION_TYPE,
        &stub,
        sessions_base.clone(),
        Arc::clone(&manager),
    ));
    // As the daemon runtime does: a jailed start reaches its host through this bridge.
    sessions.install_sandbox_rpc_bridge();
    ADaemonReachingAnSshHost {
        hosts,
        sessions,
        manager,
        sessions_base,
        runs,
        machine,
        _repo_dir: repo_dir,
        _sessions_dir: sessions_dir,
        _stub_dir: stub_dir,
        _operator_home: operator_home,
    }
}

impl ADaemonReachingAnSshHost {
    /// The derived host this daemon lists under `label`.
    async fn the_listed_derived_host(&self, label: &str) -> tddy_service::proto::host::DerivedHost {
        self.hosts
            .list_derived_hosts(Request::direct(ListDerivedHostsRequest {
                session_token: VALID_TOKEN.to_string(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("the daemon lists the hosts it reaches")
            .into_inner()
            .hosts
            .into_iter()
            .find(|host| host.label == label)
            .expect("the operator's ssh alias is listed as a derived host")
    }

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

    /// The directory the agent of `session_id` runs in, once it runs anywhere.
    async fn where_the_agent_runs(&self, session_id: &str) -> PathBuf {
        eventually(
            &format!("a run of session {session_id}'s agent"),
            OBSERVE,
            || {
                std::fs::read_to_string(&self.runs)
                    .unwrap_or_default()
                    .lines()
                    .filter_map(|line| line.split_once('\t'))
                    .map(|(_pid, cwd)| PathBuf::from(cwd))
                    .next()
                    .ok_or_else(|| "the agent has not run yet".to_string())
            },
        )
        .await
    }

    /// The target `session_id` recorded in its `.session.yaml`.
    fn recorded_target_of(&self, session_id: &str) -> Option<tddy_core::SessionTarget> {
        read_session_metadata(&self.sessions_base.join("sessions").join(session_id))
            .expect(".session.yaml is written")
            .target
    }

    /// Stop `session_id` as an operator does: signal it, and wait for its terminal to end.
    async fn stop(&self, session_id: &str) {
        self.sessions
            .signal_session(Request::direct(SignalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                signal: tddy_service::proto::session::Signal::Sigterm as i32,
                control_token: String::new(),
            }))
            .await
            .expect("the session is signalled");
        eventually_awaiting(
            &format!("session {session_id}'s terminal to end"),
            OBSERVE,
            || async {
                match self.manager.get(session_id).await {
                    None => Ok(()),
                    Some(_) => Err("the terminal is still registered".to_string()),
                }
            },
        )
        .await;
        self.manager.exit_hook_settled(session_id).await;
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_start_naming_a_derived_host_runs_on_its_parent_with_the_hosts_target() {
    // Given a daemon that lists buildbox among the hosts it reaches
    let daemon = a_daemon_reaching_an_ssh_host();
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
