//! A workspace jail whose tool channel died is rebuilt once, and an ordinary tool failure is not
//! mistaken for one.
//!
//! On 2026-09-26 a jailed session's channel closed at 14:00:14 and stayed closed. Every tool call
//! for the rest of that session — 54 of them — was refused with *"the tool call could not be run
//! in its jail (its channel is closed); refusing to run it on the host worktree instead"*. The
//! refusal is right: answering from the host a session was jailed away from would be worse than
//! failing. What was missing is the third option, rebuilding the jail.
//!
//! ## The discrimination this rests on
//!
//! A dead channel and a command that exited non-zero were reported identically, by an explicit
//! decision in `WorkspaceSandbox`'s own doc. A relaunch keyed on `is_error` would therefore
//! rebuild the jail every time a `Shell` call failed — which is most of them, on a working
//! session. So the trait reports a transport failure as its own kind, and
//! `a_tool_that_ran_and_failed_does_not_relaunch_the_jail` is the test that keeps it honest.
//!
//! The seam is `LocalExecTools::run_exec_tool_locally`: the only layer holding the sandbox
//! registry that also sits beneath all three ways a tool call arrives — the unary and streaming
//! RPC ports, and a roster agent's own turn loop. That last one is the path the 2026-09-26
//! incident's discovery agent used, and
//! it is the one driven here, through the private `local_agent_codebase_access` seam for the same
//! reason `workspace_sandbox_roster_dispatch_unit_tests` does.
//!
//! Feature: docs/ft/daemon/remote-codebase-mode.md § Workspace tool sandbox

use super::*;
use crate::test_util::{test_service, TestDaemon, TEST_TOKEN};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tddy_daemon_sandbox::workspace_tool_sandbox::{
    ToolDispatchOutcome, WorkspaceSandbox, WorkspaceSandboxProvisioner, WorkspaceSandboxSpec,
};
use tddy_sandbox::SandboxError;
use tddy_service::proto::exec_tools::ExecuteToolResponse;

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a44";
const AGENT_ID: &str = "explorer";
const A_CANARY: &str = "the-host-worktree-must-not-be-touched.txt";

/// The jail's own words when its channel has gone, verbatim.
const THE_CHANNEL_IS_CLOSED: &str = "its channel is closed";

/// How a jail behaves for one call.
#[derive(Clone, Copy, PartialEq, Eq)]
enum JailBehaviour {
    /// The channel is gone; the tool never ran.
    TransportFailure,
    /// The tool ran and said no — an ordinary failing command.
    ToolRanAndFailed,
    /// The tool ran and succeeded.
    ToolRanAndSucceeded,
}

/// A jail that behaves as scripted and records that it was stopped.
struct ScriptedSandbox {
    behaviour: JailBehaviour,
    calls: AtomicUsize,
    stopped: AtomicUsize,
}

impl ScriptedSandbox {
    fn new(behaviour: JailBehaviour) -> Self {
        Self {
            behaviour,
            calls: AtomicUsize::new(0),
            stopped: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn was_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst) > 0
    }
}

#[async_trait::async_trait]
impl WorkspaceSandbox for ScriptedSandbox {
    async fn execute_tool(&self, _req: &ExecuteToolRequest) -> ToolDispatchOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.behaviour {
            JailBehaviour::TransportFailure => ToolDispatchOutcome::TransportFailed(format!(
                "session x: the tool call could not be run in its jail \
                     ({THE_CHANNEL_IS_CLOSED})"
            )),
            JailBehaviour::ToolRanAndFailed => ToolDispatchOutcome::Ran(ExecuteToolResponse {
                result_json: serde_json::json!({ "exit_code": 1, "stderr": "no such file" })
                    .to_string(),
                is_error: true,
                error_message: "no such file".to_string(),
                job_id: String::new(),
                job_running: false,
            }),
            JailBehaviour::ToolRanAndSucceeded => ToolDispatchOutcome::Ran(ExecuteToolResponse {
                result_json: serde_json::json!({ "content": "served by the jail" }).to_string(),
                is_error: false,
                error_message: String::new(),
                job_id: String::new(),
                job_running: false,
            }),
        }
    }

    fn stop(&self) {
        self.stopped.fetch_add(1, Ordering::SeqCst);
    }
}

/// Hands out jails from a script, one per `provision`, and counts how many it was asked for.
struct ScriptedProvisioner {
    script: Mutex<Vec<Arc<ScriptedSandbox>>>,
    handed_out: Mutex<Vec<Arc<ScriptedSandbox>>>,
    provisions: AtomicUsize,
    /// When set, every provision after the first fails — a jail that cannot be rebuilt.
    refuse_to_rebuild: bool,
}

impl ScriptedProvisioner {
    fn serving(behaviours: &[JailBehaviour]) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(
                behaviours
                    .iter()
                    .rev()
                    .map(|b| Arc::new(ScriptedSandbox::new(*b)))
                    .collect(),
            ),
            handed_out: Mutex::new(Vec::new()),
            provisions: AtomicUsize::new(0),
            refuse_to_rebuild: false,
        })
    }

    fn serving_one_that_dies_and_refusing_to_rebuild() -> Arc<Self> {
        let mut provisioner = Self {
            script: Mutex::new(vec![Arc::new(ScriptedSandbox::new(
                JailBehaviour::TransportFailure,
            ))]),
            handed_out: Mutex::new(Vec::new()),
            provisions: AtomicUsize::new(0),
            refuse_to_rebuild: true,
        };
        provisioner.refuse_to_rebuild = true;
        Arc::new(provisioner)
    }

    fn provisions(&self) -> usize {
        self.provisions.load(Ordering::SeqCst)
    }

    fn jail(&self, nth: usize) -> Arc<ScriptedSandbox> {
        Arc::clone(
            self.handed_out
                .lock()
                .unwrap()
                .get(nth)
                .unwrap_or_else(|| panic!("no jail {nth} was ever provisioned")),
        )
    }
}

#[async_trait::async_trait]
impl WorkspaceSandboxProvisioner for ScriptedProvisioner {
    async fn provision(
        &self,
        _spec: &WorkspaceSandboxSpec,
    ) -> Result<Arc<dyn WorkspaceSandbox>, SandboxError> {
        let nth = self.provisions.fetch_add(1, Ordering::SeqCst);
        if self.refuse_to_rebuild && nth > 0 {
            return Err(SandboxError::Io(
                "the jail could not be rebuilt: no backend available".to_string(),
            ));
        }
        let next =
            self.script.lock().unwrap().pop().unwrap_or_else(|| {
                Arc::new(ScriptedSandbox::new(JailBehaviour::ToolRanAndSucceeded))
            });
        self.handed_out.lock().unwrap().push(Arc::clone(&next));
        Ok(next as Arc<dyn WorkspaceSandbox>)
    }
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .status()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(status.success(), "git {args:?} must succeed in {cwd:?}");
}

fn a_git_repo_with_origin() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("repo tempdir");
    let path = repo.path();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@t.com"]);
    run_git(path, &["config", "user.name", "Test"]);
    run_git(path, &["commit", "-q", "--allow-empty", "-m", "init"]);
    run_git(path, &["remote", "add", "origin", path.to_str().unwrap()]);
    run_git(path, &["push", "-q", "-u", "origin", "main"]);
    repo
}

struct SeededWorkspace {
    service: TestDaemon,
    provisioner: Arc<ScriptedProvisioner>,
    session_id: String,
    session_dir: PathBuf,
    worktree: PathBuf,
    _repo: tempfile::TempDir,
    _sessions: tempfile::TempDir,
}

async fn a_sandboxed_workspace_session_served_by(
    provisioner: Arc<ScriptedProvisioner>,
) -> SeededWorkspace {
    let repo = a_git_repo_with_origin();
    let sessions = tempfile::tempdir().expect("sessions tempdir");
    crate::project_storage::write_projects(
        &sessions.path().join("projects"),
        &[crate::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "jail-relaunch".to_string(),
            git_url: String::new(),
            main_repo_path: repo.path().display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");

    let service =
        test_service(sessions.path().to_path_buf())
            .with_workspace_sandbox_provisioner(
                Arc::clone(&provisioner) as Arc<dyn WorkspaceSandboxProvisioner>
            );

    let started = service
        .start_session(Request::direct(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_type: "workspace".to_string(),
            project_id: PROJECT_ID.to_string(),
            sandbox: true,
            ..Default::default()
        }))
        .await
        .expect("a workspace session must start")
        .into_inner();

    let session_dir = unified_session_dir_path(sessions.path(), &started.session_id);
    let worktree = PathBuf::from(
        tddy_core::read_session_metadata(&session_dir)
            .expect("session metadata")
            .repo_path
            .expect("workspace worktree"),
    );
    std::fs::write(worktree.join(A_CANARY), "untouched").expect("plant the canary");

    SeededWorkspace {
        service,
        provisioner,
        session_id: started.session_id,
        session_dir,
        worktree,
        _repo: repo,
        _sessions: sessions,
    }
}

impl SeededWorkspace {
    /// The path the 2026-09-26 incident took: a roster agent this daemon serves locally.
    fn agent_codebase_access(&self) -> tddy_discovery::subagent::CodebaseAccess {
        self.service.local_agent_codebase_access(
            &self.session_id,
            &self.session_dir,
            AGENT_ID,
            TEST_TOKEN,
        )
    }

    fn canary_is_untouched(&self) -> bool {
        std::fs::read_to_string(self.worktree.join(A_CANARY))
            .map(|c| c == "untouched")
            .unwrap_or(false)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// The want. A jail that died between calls is rebuilt, and the caller never sees the death.
#[tokio::test]
async fn a_tool_call_that_hits_a_dead_jail_relaunches_it_once_and_succeeds_on_the_retry() {
    // Given a session whose jail's channel has died, and a replacement that works
    let provisioner = ScriptedProvisioner::serving(&[
        JailBehaviour::TransportFailure,
        JailBehaviour::ToolRanAndSucceeded,
    ]);
    let workspace = a_sandboxed_workspace_session_served_by(provisioner).await;

    // When an agent reads a file
    let result = workspace
        .agent_codebase_access()
        .read("src/main.rs")
        .await
        .expect("the rebuilt jail answers");

    // Then the answer came from the replacement, which was provisioned for this call
    assert_eq!(
        result.get("content").and_then(|v| v.as_str()),
        Some("served by the jail")
    );
    assert_eq!(
        workspace.provisioner.provisions(),
        2,
        "one provision at start, one to replace the dead jail"
    );
    assert_eq!(
        workspace.provisioner.jail(1).calls(),
        1,
        "the retry ran once"
    );
}

/// The assertion that stops every failed command rebuilding the jail. Without the transport /
/// tool distinction this is what a naive `is_error` retry would break.
#[tokio::test]
async fn a_tool_that_ran_and_failed_does_not_relaunch_the_jail() {
    // Given a healthy jail whose commands fail
    let provisioner = ScriptedProvisioner::serving(&[JailBehaviour::ToolRanAndFailed]);
    let workspace = a_sandboxed_workspace_session_served_by(provisioner).await;

    // When an agent runs something that exits non-zero
    let result = workspace.agent_codebase_access().read("src/gone.rs").await;

    // Then the failure is the caller's to handle, and the jail was left alone
    assert!(result.is_err(), "a failing tool is still a failing tool");
    assert_eq!(
        workspace.provisioner.provisions(),
        1,
        "a non-zero exit is not a dead channel; rebuilding the jail on every failed command \
         would restart it constantly on a perfectly healthy session"
    );
}

/// Once, not in a loop. A jail that comes back dead is a broken host, not a transient.
#[tokio::test]
async fn a_second_transport_failure_is_reported_rather_than_relaunched_again() {
    // Given a jail that dies, and a replacement that is dead too
    let provisioner = ScriptedProvisioner::serving(&[
        JailBehaviour::TransportFailure,
        JailBehaviour::TransportFailure,
    ]);
    let workspace = a_sandboxed_workspace_session_served_by(provisioner).await;

    // When an agent reads a file
    let result = workspace.agent_codebase_access().read("src/main.rs").await;

    // Then the caller is told, after exactly one rebuild
    let error = result
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| panic!("a jail that stays dead must be reported"));
    assert!(
        error.contains(THE_CHANNEL_IS_CLOSED),
        "the caller must be told what actually failed, got: {error}"
    );
    assert_eq!(
        workspace.provisioner.provisions(),
        2,
        "one rebuild, then report — not a retry loop"
    );
}

/// A replaced jail is a `tddy-sandbox-runner` process. Leaving it running orphans a confined
/// process onto the host for the life of the daemon.
#[tokio::test]
async fn the_replaced_jail_is_stopped_when_it_is_rebuilt() {
    // Given a jail that dies and is replaced
    let provisioner = ScriptedProvisioner::serving(&[
        JailBehaviour::TransportFailure,
        JailBehaviour::ToolRanAndSucceeded,
    ]);
    let workspace = a_sandboxed_workspace_session_served_by(provisioner).await;

    // When the call goes through the rebuild
    workspace
        .agent_codebase_access()
        .read("src/main.rs")
        .await
        .expect("the rebuilt jail answers");

    // Then the dead one was torn down rather than left running
    assert!(
        workspace.provisioner.jail(0).was_stopped(),
        "the replaced jail must be stopped; an unreaped runner outlives the session it served"
    );
}

/// The invariant the whole refusal exists to protect, restated for the new path. A relaunch that
/// fails must fail — never quietly fall back to the checkout the session was jailed away from.
#[tokio::test]
async fn a_jailed_sessions_tools_never_reach_the_host_worktree_when_a_relaunch_fails() {
    // Given a jail that dies and cannot be rebuilt
    let provisioner = ScriptedProvisioner::serving_one_that_dies_and_refusing_to_rebuild();
    let workspace = a_sandboxed_workspace_session_served_by(provisioner).await;

    // When an agent tries to write to the worktree
    let result = workspace
        .agent_codebase_access()
        .write(A_CANARY, "written by the host tool engine")
        .await;

    // Then the call failed and the host's copy is exactly as it was
    assert!(result.is_err(), "a session with no jail runs nothing");
    assert!(
        workspace.canary_is_untouched(),
        "the host tool engine ran a jailed session's write; this is the one thing the refusal \
         exists to prevent, and a relaunch path must not open a way around it"
    );
}
