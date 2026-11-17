//! Acceptance: the hooks of an agent that runs on a **target** reach the daemon through the
//! real `DaemonSessionHost`, with nothing of tddy's on the target.
//!
//! As in `remote_target_session_acceptance.rs` (whose fixture this follows), the target is a
//! stand-in `ssh` first on this process's `PATH`, a temp directory stands for its filesystem,
//! and the agent is `claude.yaml` dropped in under another id. The stub agent fires its
//! `SessionStart` hook as Claude Code does: the hook command installed in its worktree's
//! `.claude/settings.local.json`, run under `sh -c` with the event's payload on its stdin.
//! Nothing calls the follower, the log creation or its removal directly: the session is started,
//! stopped and deleted through the service, and the daemon's own follower delivers the hook.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-remote-hooks.md.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_core::session_metadata::read_session_metadata;
use tddy_rpc::Request;
use tddy_service::proto::session::{
    session_target, DeleteSessionRequest, SessionService as SessionServiceTrait, SessionTarget,
    SignalSessionRequest, SshTarget, StartSessionRequest,
};
use tddy_session_lifecycle::claude_cli_session::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_testing_commons::wait::eventually_awaiting;

mod agent_registry_support;
mod cli_harness_support;
use agent_registry_support::builtin_yaml_as;
use cli_harness_support::{
    a_daemon_config, create_test_repo_with_origin, register_project, session_host, VALID_TOKEN,
};

const TEST_PROJECT_ID: &str = "remote-hooks-project";
const AGENT_ID: &str = "acme";
const SESSION_TYPE: &str = "acme-cli";
const MODEL: &str = "acme-1";
const BRANCH: &str = "acme-work";
const TARGET_ALIAS: &str = "buildbox";
/// The status a `SessionStart` hook sets on its session.
const STARTED: &str = "Started";

/// A safety net for a hook to reach the daemon, not a prediction of how long it takes.
const OBSERVE: Duration = Duration::from_secs(30);

fn write_executable(path: &Path, script: &str) {
    // A freshly written executable pays a first-execution scan on this OS (~2.4 s measured on
    // macOS, against ~20 ms afterwards): a stand-in or stub written here and then timed from
    // its first real exec would spend that scan inside whatever window the test measures. So
    // the script carries a warm-up guard as its first line, and is run once here, guarded —
    // no runs are recorded, no pid files written — to pay the scan before any launch is timed.
    let script = match script.strip_prefix("#!/bin/sh\n") {
        Some(body) => {
            format!("#!/bin/sh\nif [ -n \"$TDDY_FIXTURE_WARMUP\" ]; then exit 0; fi\n{body}")
        }
        None => script.to_string(),
    };
    std::fs::write(path, script).expect("the script is written");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("the script is made executable");
    let warmed = std::process::Command::new(path)
        .env("TDDY_FIXTURE_WARMUP", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("the freshly written script warms up");
    assert!(
        warmed.status.success(),
        "the warm-up of {} exited {}: {}",
        path.display(),
        warmed.status,
        String::from_utf8_lossy(&warmed.stderr)
    );
}

/// The target a session's agent runs on: a stand-in `ssh` first on this process's `PATH`, and
/// the directory standing for the target's filesystem.
struct ATarget {
    root: PathBuf,
    previous_path: Option<std::ffi::OsString>,
    _dir: tempfile::TempDir,
}

impl ATarget {
    /// A target that runs what it is asked to, as `remote_target_session_acceptance.rs`'s does.
    fn reachable() -> Self {
        let dir = tempfile::tempdir().expect("a directory standing for the target");
        let root = std::fs::canonicalize(dir.path()).expect("the target's directory resolves");
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("a directory for the stand-in ssh");
        write_executable(
            &bin.join("ssh"),
            &format!(
                r#"#!/bin/sh
for command; do :; done
command=$(printf '%s' "$command" | /usr/bin/sed 's#/home/dev#{root}/home/dev#g')
export HOME='{root}'
exec 3<&0
/bin/sh -c "$command" <&3 3<&- &
remote=$!
trap 'kill -HUP "$remote" 2>/dev/null' HUP TERM
wait "$remote"
exit $?
"#,
                root = root.display()
            ),
        );
        let previous_path = std::env::var_os("PATH");
        let mut path = std::ffi::OsString::from(&bin);
        if let Some(previous) = &previous_path {
            path.push(":");
            path.push(previous);
        }
        std::env::set_var("PATH", path);
        Self {
            root,
            previous_path,
            _dir: dir,
        }
    }

    /// Where the target keeps the worktree of `session_id`: under the checkout resolved from
    /// the target user's home — the stub's `$HOME` is this directory, and the project's empty
    /// git URL names `repo` under `.tddy/repos`.
    fn worktree_of(&self, session_id: &str) -> PathBuf {
        self.root
            .join(".tddy/repos/repo/.worktrees")
            .join(session_id)
    }

    /// Where the hook log of `session_id` lives on the target.
    fn hook_log_of(&self, session_id: &str) -> PathBuf {
        self.worktree_of(session_id)
            .join(tddy_core::REMOTE_HOOK_LOG)
    }
}

impl Drop for ATarget {
    fn drop(&mut self) {
        match &self.previous_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
    }
}

/// A stub agent that, as Claude Code does on a `SessionStart`, hands the event's payload to the
/// hook command installed in its worktree — read from its `.claude/settings.local.json` — then
/// stays up. It fires once it has had time to be recorded as started, so the daemon's own
/// record of the session (which carries the hook token the report is checked against) is the
/// one written at the start.
fn an_agent_that_fires_its_start_hook() -> String {
    r#"#!/bin/sh
PATH=/usr/bin:/bin
export PATH
/bin/sleep 3
command=$(/usr/bin/python3 -c "import json; print(json.load(open('.claude/settings.local.json'))['hooks']['SessionStart'][0]['hooks'][0]['command'])")
printf '%s' '{"hook_event_name":"SessionStart","session_id":"c-41","source":"startup"}' | /bin/sh -c "$command"
printf 'ACME UP\n'
while :; do /bin/sleep 0.1; done
"#
    .to_string()
}

/// A daemon with one registered project, the `acme` agent installed, and a reachable target.
struct ADaemonWithATarget {
    service: Arc<DaemonSessionHost>,
    manager: Arc<CliSessionManager>,
    sessions_base: PathBuf,
    target: ATarget,
    _repo_dir: tempfile::TempDir,
    _sessions_dir: tempfile::TempDir,
    _stub_dir: tempfile::TempDir,
}

fn a_daemon_with_a_reachable_target() -> ADaemonWithATarget {
    let target = ATarget::reachable();
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
    let stub = stub_bin.join(AGENT_ID);
    write_executable(&stub, &an_agent_that_fires_its_start_hook());

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
        manager,
        sessions_base,
        target,
        _repo_dir: repo_dir,
        _sessions_dir: sessions_dir,
        _stub_dir: stub_dir,
    }
}

/// A start of an `acme-cli` session whose agent runs on the ssh target.
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
    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_base.join("sessions").join(session_id)
    }

    /// A started session whose agent runs on the target.
    async fn a_session_on_the_target(&self) -> String {
        self.service
            .start_session(Request::direct(a_start_on_the_target()))
            .await
            .expect("a start on a reachable target succeeds")
            .into_inner()
            .session_id
    }

    /// Stop `session_id` through the service, and wait for its terminal (and exit hook) to end.
    async fn stop(&self, session_id: &str) {
        self.service
            .signal_session(Request::direct(SignalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                signal: tddy_service::proto::session::Signal::Sigterm as i32,
                control_token: String::new(),
            }))
            .await
            .expect("the session is signalled");
        eventually_awaiting("the session's terminal to end", OBSERVE, || async {
            match self.manager.get(session_id).await {
                None => Ok(()),
                Some(_) => Err("the terminal is still registered".to_string()),
            }
        })
        .await;
        self.manager.exit_hook_settled(session_id).await;
    }

    async fn delete(&self, session_id: &str) {
        self.service
            .delete_session(Request::direct(DeleteSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("the session is deleted");
    }

    /// The session's recorded activity status, once a hook has set it.
    async fn activity_once_a_hook_arrives(&self, session_id: &str) -> Option<String> {
        let session_dir = self.session_dir(session_id);
        let hook_log = self.target.hook_log_of(session_id);
        eventually_awaiting(
            "a hook to change the session's activity",
            OBSERVE,
            || async {
                let status = read_session_metadata(&session_dir)
                    .ok()
                    .and_then(|metadata| metadata.activity_status);
                match status.clone().filter(|status| !status.is_empty()) {
                    Some(status) => Ok(Some(status)),
                    None => Err(format!(
                        "the session's activity so far: {status:?}; its hook log holds: {:?}",
                        std::fs::read_to_string(&hook_log).unwrap_or_default()
                    )),
                }
            },
        )
        .await
    }
}

#[tokio::test]
#[serial_test::serial]
async fn a_hook_fired_by_an_agent_on_a_target_reaches_the_daemon_through_the_service() {
    // Given a session started through the service, whose agent on the target fires its
    // SessionStart hook as it starts
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon.a_session_on_the_target().await;
    assert_eq!(
        daemon.target.hook_log_of(&session_id).is_file(),
        true,
        "the session's hook log is created on the target"
    );

    // When the hook fires — nothing in this test calls the follower
    let activity = daemon.activity_once_a_hook_arrives(&session_id).await;

    // Then the session has started, as a local SessionStart hook would have reported it
    assert_eq!(activity, Some(STARTED.to_string()));
    daemon.stop(&session_id).await;
}

#[tokio::test]
#[serial_test::serial]
async fn deleting_a_target_session_through_the_service_removes_its_hook_log() {
    // Given a target session whose hook log is on the target and being followed
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = daemon.a_session_on_the_target().await;
    let hook_log = daemon.target.hook_log_of(&session_id);
    assert_eq!(hook_log.is_file(), true, "the hook log is on the target");
    daemon.stop(&session_id).await;

    // When the session is deleted through the service
    daemon.delete(&session_id).await;

    // Then its hook log is gone from the target
    assert_eq!(hook_log.exists(), false);
}
