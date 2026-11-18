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

use pretty_assertions::assert_eq;
use tddy_rpc::Request;
use tddy_service::proto::host::DerivedHost;
use tddy_service::proto::session::{SessionService as SessionServiceTrait, StartSessionRequest};

mod agent_registry_support;
mod cli_harness_support;
mod derived_host_support;
use cli_harness_support::VALID_TOKEN;
use derived_host_support::{
    a_daemon_reaching_an_ssh_host, ADaemonReachingAnSshHost, BRANCH, DERIVED_HOST_ALIAS, MODEL,
    SESSION_TYPE,
};

const TEST_PROJECT_ID: &str = "derived-host-project";

impl ADaemonReachingAnSshHost {
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
}

#[tokio::test]
#[serial_test::serial]
async fn a_start_addressed_at_a_listed_derived_host_runs_on_that_host() {
    // Given a daemon that lists buildbox among the hosts it reaches
    let daemon = a_daemon_reaching_an_ssh_host(TEST_PROJECT_ID);
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
