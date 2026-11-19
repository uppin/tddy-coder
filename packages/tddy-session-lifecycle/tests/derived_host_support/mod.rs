//! The derived-host daemon `derived_host_start_acceptance` and `orchestrator_target_start_acceptance`
//! both start sessions on: one daemon -- its host service and its session host over one config --
//! with a registered project, the `acme` agent installed on this host and on the stand-in machine,
//! and an operator whose SSH config names [`DERIVED_HOST_ALIAS`], a machine it reaches.
//!
//! Each suite addresses its start its own way (a listed host, a resolved one), so the start itself
//! stays in the suite; everything else here is used by both, so neither test binary carries dead code.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tddy_testing_commons::remote_target::{write_executable, StandInSshTarget};

use tddy_core::session_metadata::read_session_metadata;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_host_service::host_private_key::HostUserFiles;
use tddy_host_service::HostServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::host::{
    DerivedHost, HostService as HostServiceTrait, ListDerivedHostsRequest,
};
use tddy_service::proto::session::{SessionService as SessionServiceTrait, SignalSessionRequest};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::wait::{eventually, eventually_awaiting};

use crate::agent_registry_support::builtin_yaml_as;
use crate::cli_harness_support::{
    a_daemon_config, create_test_repo_with_origin, current_os_user, register_project, session_host,
    VALID_TOKEN,
};

/// The agent: `claude.yaml` under this id, so its sessions launch through the generic CLI path.
pub const AGENT_ID: &str = "acme";
pub const SESSION_TYPE: &str = "acme-cli";
pub const MODEL: &str = "acme-1";
/// The branch a start names: `claude.yaml` declares `branch_name_required`.
pub const BRANCH: &str = "acme-work";
/// The ssh `Host` alias the derived host is known by.
pub const DERIVED_HOST_ALIAS: &str = "buildbox";

/// A safety net for an agent to be observed on the host, not a prediction of how long it takes.
pub const OBSERVE: Duration = Duration::from_secs(20);

/// The operator's files, read from a home on a temp path — whichever OS user is asked for.
pub struct AnOperatorHome {
    pub home: PathBuf,
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
pub struct ADaemonReachingAnSshHost {
    pub hosts: HostServiceImpl,
    pub sessions: Arc<DaemonSessionHost>,
    pub manager: Arc<CliSessionManager>,
    pub sessions_base: PathBuf,
    pub runs: PathBuf,
    pub machine: StandInSshTarget,
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

pub fn a_daemon_reaching_an_ssh_host(project_id: &str) -> ADaemonReachingAnSshHost {
    let machine = StandInSshTarget::behind_ssh();
    let repo_dir = tempfile::tempdir().unwrap();
    create_test_repo_with_origin(repo_dir.path());
    let sessions_dir = tempfile::tempdir().unwrap();
    let sessions_base = std::fs::canonicalize(sessions_dir.path()).unwrap();
    register_project(&sessions_base.join("projects"), repo_dir.path(), project_id);
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
    pub async fn the_listed_derived_host(&self, label: &str) -> DerivedHost {
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

    /// The directory the agent of `session_id` runs in, once it runs anywhere.
    pub async fn where_the_agent_runs(&self, session_id: &str) -> PathBuf {
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
    pub fn recorded_target_of(&self, session_id: &str) -> Option<tddy_core::SessionTarget> {
        read_session_metadata(&self.sessions_base.join("sessions").join(session_id))
            .expect(".session.yaml is written")
            .target
    }

    /// Stop `session_id` as an operator does: signal it, and wait for its terminal to end.
    pub async fn stop(&self, session_id: &str) {
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
