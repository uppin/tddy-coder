//! Closing a session whose agent runs on a **target** — `PreviewCloseSession` and `CloseSession`
//! through the real `DaemonSessionHost` (docs/ft/daemon/session-liveness.md § Closing a session):
//! the worktree is read, and removed, on the target, through the session's remote shell; a target
//! that cannot be reached is reported, and nothing is done.
//!
//! The target is the stand-in `ssh` of `remote_target_session_acceptance.rs`: a script first on
//! this process's `PATH` that runs each remote command on this machine with the target's
//! `/home/dev` mapped under a temp directory, so the worktree "there" is observable here. Real git
//! runs on both sides. Serial: `PATH` is process-wide.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_rpc::{Request, Status};
use tddy_service::proto::session::{
    session_target, CloseSessionPreview, CloseSessionRequest, DeleteSessionRequest,
    PreviewCloseSessionRequest, SessionService as SessionServiceTrait, SessionTarget, SshTarget,
    StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::remote_target::{write_executable, StandInSshTarget};
use tddy_testing_commons::wait::eventually;

mod agent_registry_support;
#[allow(dead_code)] // the resume check is the resume suites'
mod agent_run_support;
mod cli_harness_support;
mod close_events_support;

use agent_registry_support::builtin_yaml_as;
use agent_run_support::{read_agent_runs, AnAgentRun};
use cli_harness_support::{
    a_daemon_config, create_test_repo_with_origin, register_project, session_host, VALID_TOKEN,
};
use close_events_support::CloseEvents;

const TEST_PROJECT_ID: &str = "close-target-project";
/// The agent: `claude.yaml` under this id, so its sessions launch through the generic CLI path.
const AGENT_ID: &str = "acme";
const SESSION_TYPE: &str = "acme-cli";
const MODEL: &str = "acme-1";
/// The branch a start names: `claude.yaml` declares `branch_name_required`.
const BRANCH: &str = "acme-close-work";
/// The ssh `Host` alias the target is known by.
const TARGET_ALIAS: &str = "buildbox";
/// What the target's transport says once it can no longer be reached.
const UNREACHABLE: &str = "ssh: connect to host buildbox port 22: Connection refused";

/// A safety net for an agent to be observed on the target, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(20);

/// A stub agent: appends one line per run — its pid, its working directory and its arguments — to
/// `runs`, then stays up until it is hung up.
fn a_stub_agent(runs: &std::path::Path) -> String {
    format!(
        r#"#!/bin/sh
{{ printf '%s\t%s' "$$" "$(/bin/pwd -P)"; for arg in "$@"; do printf '\t%s' "$arg"; done; printf '\n'; }} >> '{runs}'
while :; do /bin/sleep 0.1; done
"#,
        runs = runs.display()
    )
}

/// A daemon with one registered project and the `acme` agent installed on its host and on a
/// target its sessions' agents run on.
struct ADaemonWithATarget {
    service: Arc<DaemonSessionHost>,
    sessions_base: PathBuf,
    runs: PathBuf,
    target: StandInSshTarget,
    _manager: Arc<CliSessionManager>,
    _repo_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
}

fn a_daemon_with_a_reachable_target() -> ADaemonWithATarget {
    let target = StandInSshTarget::behind_ssh();
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
    write_executable(&target.agents_dir().join(AGENT_ID), &a_stub_agent(&runs));

    let manager = Arc::new(CliSessionManager::new());
    let service = Arc::new(session_host(
        a_daemon_config(),
        SESSION_TYPE,
        &stub,
        sessions_base.clone(),
        Arc::clone(&manager),
    ));
    service.install_sandbox_rpc_bridge();
    ADaemonWithATarget {
        service,
        sessions_base,
        runs,
        target,
        _manager: manager,
        _repo_dir: repo_dir,
        _sessions_dir: sessions_dir,
        _stub_dir: stub_dir,
    }
}

/// A start of an `acme-cli` session whose agent runs on the ssh host [`TARGET_ALIAS`].
fn a_start_on_the_target() -> StartSessionRequest {
    StartSessionRequest {
        session_token: VALID_TOKEN.to_string(),
        project_id: TEST_PROJECT_ID.to_string(),
        session_type: SESSION_TYPE.to_string(),
        model: MODEL.to_string(),
        branch_worktree_intent: "new_branch_from_base".to_string(),
        new_branch_name: BRANCH.to_string(),
        target: Some(SessionTarget {
            kind: Some(session_target::Kind::Ssh(SshTarget {
                config_host: TARGET_ALIAS.to_string(),
            })),
        }),
        ..Default::default()
    }
}

impl ADaemonWithATarget {
    /// A started session whose agent runs on the target, once its agent runs there.
    async fn a_running_session_on_the_target(&self) -> (String, AnAgentRun) {
        let session_id = self
            .service
            .start_session(Request::direct(a_start_on_the_target()))
            .await
            .expect("a start on a reachable target succeeds")
            .into_inner()
            .session_id;
        let worktree = self.target.worktree_of(&session_id);
        let agent = eventually(
            &format!("the agent to run in {}", worktree.display()),
            OBSERVE,
            || {
                read_agent_runs(&self.runs)
                    .into_iter()
                    .find(|run| run.cwd == worktree)
                    .ok_or_else(|| "the agent has not run on the target yet".to_string())
            },
        )
        .await;
        (session_id, agent)
    }

    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_base.join("sessions").join(session_id)
    }

    /// The agent left `file` in `session_id`'s worktree on the target, untracked.
    fn agent_leaves_a_file_on_the_target(&self, session_id: &str, file: &str) {
        std::fs::write(
            self.target.worktree_of(session_id).join(file),
            "left by the agent\n",
        )
        .unwrap();
    }

    /// The target's transport refuses every connection from now on.
    fn the_target_becomes_unreachable(&self) {
        write_executable(
            &self.target.root.join("bin/ssh"),
            &format!("#!/bin/sh\necho '{UNREACHABLE}' >&2\nexit 255\n"),
        );
    }

    /// The session's worktree was removed on the target, outside the daemon.
    fn worktree_is_removed_on_the_target(&self, session_id: &str) {
        let worktree = self.target.worktree_of(session_id);
        let clone = worktree
            .parent()
            .and_then(|worktrees| worktrees.parent())
            .expect("the worktree sits in its clone's .worktrees")
            .to_path_buf();
        let removed = std::process::Command::new("git")
            .arg("-C")
            .arg(&clone)
            .args(["worktree", "remove", "--force"])
            .arg(&worktree)
            .status()
            .expect("git runs");
        assert!(removed.success(), "the worktree is removed on the target");
    }

    async fn preview_of_closing(&self, session_id: &str) -> Result<CloseSessionPreview, Status> {
        self.service
            .preview_close_session(Request::direct(PreviewCloseSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|preview| preview.into_inner())
    }

    async fn previews_closing(&self, session_id: &str) -> CloseSessionPreview {
        self.preview_of_closing(session_id)
            .await
            .expect("PreviewCloseSession must succeed")
    }

    /// Close `session_id` confirming `fingerprint`, and answer every event the close streamed.
    async fn closes_confirming(&self, session_id: &str, fingerprint: &str) -> CloseEvents {
        let stream = self
            .service
            .close_session(Request::direct(CloseSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                confirmed: true,
                confirmed_fingerprint: fingerprint.to_string(),
            }))
            .await
            .expect("CloseSession must start")
            .into_inner();
        CloseEvents::read_to_end(stream).await
    }

    async fn delete_of(&self, session_id: &str) -> Result<(), Status> {
        self.service
            .delete_session(Request::direct(DeleteSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|_| ())
    }
}

/// Wait until `agent`'s process is gone.
async fn the_agent_has_ended(agent: &AnAgentRun) {
    eventually("the agent on the target to end", OBSERVE, || {
        (!agent.is_alive())
            .then_some(())
            .ok_or_else(|| format!("pid {} is still alive", agent.pid))
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_reads_a_dirty_worktree_on_the_target() {
    // Given a session on the target whose agent left a file in its worktree there
    let daemon = a_daemon_with_a_reachable_target();
    let (session_id, _agent) = daemon.a_running_session_on_the_target().await;
    daemon.agent_leaves_a_file_on_the_target(&session_id, "notes.md");

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then the worktree is the target's, named by its host, and lists the file
    let worktree = &preview.worktrees[0];
    let files: Vec<(&str, &str)> = worktree
        .dirty_files
        .iter()
        .map(|file| (file.path.as_str(), file.change.as_str()))
        .collect();
    assert_eq!(
        (
            preview.needs_confirmation,
            preview.worktrees.len(),
            worktree.host.as_str(),
            worktree.unavailable_reason.as_str(),
            files.contains(&("notes.md", "untracked")),
        ),
        (true, 1, TARGET_ALIAS, "", true),
        "{preview:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_confirmed_close_removes_the_worktree_on_the_target() {
    // Given a dirty session on the target, and its preview
    let daemon = a_daemon_with_a_reachable_target();
    let (session_id, agent) = daemon.a_running_session_on_the_target().await;
    daemon.agent_leaves_a_file_on_the_target(&session_id, "notes.md");
    let worktree = daemon.target.worktree_of(&session_id);
    let preview = daemon.previews_closing(&session_id).await;

    // When the close confirms it
    let events = daemon
        .closes_confirming(&session_id, &preview.fingerprint)
        .await;

    // Then the agent on the target ended, and its worktree there is gone
    the_agent_has_ended(&agent).await;
    assert_eq!(
        (
            events.outcome().ok,
            worktree.exists(),
            daemon
                .session_dir(&session_id)
                .join(".session.yaml")
                .is_file(),
        ),
        (true, false, true),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_preview_of_a_session_whose_target_is_unreachable_says_why_and_needs_confirmation() {
    // Given a session on a target that can no longer be reached
    let daemon = a_daemon_with_a_reachable_target();
    let (session_id, _agent) = daemon.a_running_session_on_the_target().await;
    daemon.the_target_becomes_unreachable();

    // When
    let preview = daemon.previews_closing(&session_id).await;

    // Then
    assert_eq!(
        (
            preview.needs_confirmation,
            preview.worktrees.len(),
            preview.worktrees[0]
                .unavailable_reason
                .contains("Connection refused"),
        ),
        (true, 1, true),
        "{preview:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_confirmed_close_of_a_session_whose_target_is_unreachable_does_nothing() {
    // Given a session on a target that can no longer be reached, and the preview saying so
    let daemon = a_daemon_with_a_reachable_target();
    let (session_id, agent) = daemon.a_running_session_on_the_target().await;
    let worktree = daemon.target.worktree_of(&session_id);
    daemon.the_target_becomes_unreachable();
    let preview = daemon.previews_closing(&session_id).await;

    // When the close confirms that preview
    let events = daemon
        .closes_confirming(&session_id, &preview.fingerprint)
        .await;

    // Then it asks again, saying why, and nothing was ended or removed
    assert_eq!(
        (
            events.needs_confirmation().worktrees[0]
                .unavailable_reason
                .contains("Connection refused"),
            worktree.is_dir(),
            agent.is_alive(),
        ),
        (true, true, true),
        "{events:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_target_session_whose_worktree_was_already_removed_succeeds() {
    // Given a session on the target whose worktree there was removed, its agent ended
    let daemon = a_daemon_with_a_reachable_target();
    let (session_id, agent) = daemon.a_running_session_on_the_target().await;
    // SAFETY: a plain kill(2) of the stub this test started.
    unsafe { libc::kill(agent.pid, libc::SIGKILL) };
    the_agent_has_ended(&agent).await;
    daemon.worktree_is_removed_on_the_target(&session_id);

    // When
    let deleted = daemon.delete_of(&session_id).await;

    // Then a worktree already gone counts as removed
    assert_eq!(
        (
            deleted.map_err(|s| s.message().to_string()),
            daemon.session_dir(&session_id).exists()
        ),
        (Ok(()), false)
    );
}
