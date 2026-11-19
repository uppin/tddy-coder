//! Acceptance: a start addressed at a **derived host** — a host the daemon listed as one it
//! reaches — runs the session's agent there, through the real `HostServiceImpl` and the real
//! `DaemonSessionHost` of one daemon.
//!
//! It proves the two halves fit: what `ListDerivedHosts` answers for a host — its parent daemon
//! and its target — is what `StartSession` takes as `daemon_instance_id` and `target`, with
//! nothing translated in between. That is the request the Host field of the web form sends.
//!
//! The derived host is an alias in an operator's `~/.ssh/config` on a temp path. The machine
//! behind it is a stand-in `ssh` executable first on this process's `PATH`, as in
//! `remote_target_session_acceptance.rs`: it runs the remote command on this machine with the
//! target's `/home/dev` mapped onto a temp directory, so nothing opens a connection. The agent is
//! `claude.yaml` under another id, its binary a stub that records each run.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-derived-hosts.md.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tddy_testing_commons::remote_target::{write_executable, StandInSshTarget};

use pretty_assertions::assert_eq;
use tddy_core::session_metadata::read_session_metadata;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_host_service::host_private_key::HostUserFiles;
use tddy_host_service::HostServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::host::{
    DerivedHost, HostService as HostServiceTrait, ListDerivedHostsRequest,
};
use tddy_service::proto::session::{
    SessionService as SessionServiceTrait, SignalSessionRequest, StartSessionRequest,
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

const TEST_PROJECT_ID: &str = "derived-host-project";
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
    async fn the_listed_derived_host(&self, label: &str) -> DerivedHost {
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

    /// Start an `acme-cli` session addressed at `host` the way the Host field addresses it: the
    /// host's parent as the daemon, the host's target as the target.
    async fn start_on(&self, host: &DerivedHost) -> String {
        self.sessions
            .start_session(Request::direct(StartSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                project_id: TEST_PROJECT_ID.to_string(),
                session_type: SESSION_TYPE.to_string(),
                model: MODEL.to_string(),
                branch_worktree_intent: "new_branch_from_base".to_string(),
                new_branch_name: BRANCH.to_string(),
                daemon_instance_id: host.parent_instance_id.clone(),
                target: host.target.clone(),
                ..Default::default()
            }))
            .await
            .expect("a start addressed at a listed derived host succeeds")
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
async fn a_start_addressed_at_a_listed_derived_host_runs_on_that_host() {
    // Given a daemon that lists buildbox among the hosts it reaches
    let daemon = a_daemon_reaching_an_ssh_host();
    let buildbox = daemon.the_listed_derived_host(DERIVED_HOST_ALIAS).await;

    // When a session is started on the host's parent daemon, with the host's target
    let session_id = daemon.start_on(&buildbox).await;
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
