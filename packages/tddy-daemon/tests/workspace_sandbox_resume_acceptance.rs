//! Acceptance: resuming and deleting a **sandboxed `workspace` session** after the daemon that
//! started it has gone away.
//!
//! PRD: `docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox.
//! Changeset: `docs/dev/1-WIP/2026-08-31-split-sandbox-resume.md`.
//!
//! A sandboxed workspace session's jail is a live process, but `sandbox: Some(true)` in
//! `.session.yaml` outlives it. After a daemon restart the session still says its tools must be
//! confined and this daemon holds nothing to confine them with — so a resume has to re-provision
//! the jail from the persisted flag and worktree path, and a delete has to tear down whatever the
//! old daemon left running. Both are the codebase half of a sandboxed split session's lifecycle,
//! and both are testable on one daemon with an injected provisioner, exactly the way
//! `workspace_tool_sandbox_acceptance.rs` proves the start-time contract.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_daemon::workspace_tool_sandbox::{
    WorkspaceSandbox, WorkspaceSandboxProvisioner, WorkspaceSandboxSpec,
};
use tddy_rpc::{Request, Status};
use tddy_sandbox::SandboxError;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest, ExecuteToolRequest,
    ExecuteToolResponse, ResumeSessionRequest, StartSessionRequest,
};

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a42";

/// The marker a jailed tool result carries. Its presence in a response proves the call went to the
/// jail; its absence proves it went to the host tool engine.
const JAIL_MARKER: &str = "ran-inside-the-workspace-jail";

/// The file a workspace jail's runner writes its pid to, so a daemon that comes back after a crash
/// can find and kill the orphan. The production jail writes this at provision; a test double writes
/// it here so the delete-time teardown is assertable without a kernel sandbox on the host running
/// the test.
const RUNNER_PID_FILE: &str = "runner.pid";

// ---------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------

/// One tool call the jail was asked to run.
#[derive(Debug, Clone, PartialEq, Eq)]
struct JailedCall {
    session_id: String,
    tool_name: String,
    args_json: String,
}

/// A jail that records what it was asked to run and answers with [`JAIL_MARKER`], touching no
/// filesystem at all — so a host worktree left unchanged is proof the call never reached the host
/// tool engine.
#[derive(Default)]
struct RecordingSandbox {
    calls: Mutex<Vec<JailedCall>>,
}

impl RecordingSandbox {
    fn calls(&self) -> Vec<JailedCall> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl WorkspaceSandbox for RecordingSandbox {
    async fn execute_tool(&self, req: &ExecuteToolRequest) -> ExecuteToolResponse {
        self.calls.lock().unwrap().push(JailedCall {
            session_id: req.session_id.clone(),
            tool_name: req.tool_name.clone(),
            args_json: req.args_json.clone(),
        });
        ExecuteToolResponse {
            result_json: serde_json::json!({ "marker": JAIL_MARKER, "tool": req.tool_name })
                .to_string(),
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        }
    }

    fn stop(&self) {}
}

/// A provisioner that hands out one [`RecordingSandbox`] and remembers every spec it was asked
/// for, so both "the jail was built" and "no jail was ever built" are assertable.
#[derive(Default)]
struct RecordingProvisioner {
    sandbox: Arc<RecordingSandbox>,
    provisioned: Mutex<Vec<WorkspaceSandboxSpec>>,
}

impl RecordingProvisioner {
    fn provisioned(&self) -> Vec<WorkspaceSandboxSpec> {
        self.provisioned.lock().unwrap().clone()
    }

    fn sandbox(&self) -> Arc<RecordingSandbox> {
        Arc::clone(&self.sandbox)
    }
}

#[async_trait]
impl WorkspaceSandboxProvisioner for RecordingProvisioner {
    async fn provision(
        &self,
        spec: &WorkspaceSandboxSpec,
    ) -> Result<Arc<dyn WorkspaceSandbox>, SandboxError> {
        self.provisioned.lock().unwrap().push(spec.clone());
        Ok(Arc::clone(&self.sandbox) as Arc<dyn WorkspaceSandbox>)
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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

fn register_project(sessions_base: &Path, repo_path: &Path) {
    tddy_daemon::project_storage::write_projects(
        &sessions_base.join("projects"),
        &[tddy_daemon::project_storage::ProjectData {
            project_id: PROJECT_ID.to_string(),
            name: "workspace-sandbox-resume".to_string(),
            git_url: String::new(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");
}

/// A daemon holding a registered project, ready to be asked for a workspace session.
struct CodebaseHost {
    service: ConnectionServiceImpl,
    sessions: tempfile::TempDir,
    _repo: tempfile::TempDir,
}

impl CodebaseHost {
    fn with_provisioner(provisioner: Arc<dyn WorkspaceSandboxProvisioner>) -> Self {
        let repo = a_git_repo_with_origin();
        let sessions = tempfile::tempdir().expect("sessions tempdir");
        register_project(sessions.path(), repo.path());
        let service = test_service(sessions.path().to_path_buf())
            .with_workspace_sandbox_provisioner(provisioner);
        Self {
            service,
            sessions,
            _repo: repo,
        }
    }

    async fn start_sandboxed_workspace(&self) -> String {
        self.service
            .start_session(Request::new(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_type: "workspace".to_string(),
                project_id: PROJECT_ID.to_string(),
                sandbox: true,
                ..Default::default()
            }))
            .await
            .expect("a sandboxed workspace start must succeed")
            .into_inner()
            .session_id
    }

    async fn resume(&self, session_id: &str) -> Result<(), Status> {
        self.service
            .resume_session(Request::new(ResumeSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|_| ())
    }

    async fn execute_tool(&self, session_id: &str, tool: &str, args: &str) -> ExecuteToolResponse {
        self.service
            .execute_tool(Request::new(a_tool_request(session_id, tool, args)))
            .await
            .expect("ExecuteTool must not fail at the RPC level")
            .into_inner()
    }

    fn worktree_of(&self, session_id: &str) -> PathBuf {
        PathBuf::from(
            self.metadata_of(session_id)
                .repo_path
                .expect("a workspace session must have a repo_path"),
        )
    }

    fn metadata_of(&self, session_id: &str) -> tddy_core::SessionMetadata {
        let dir = unified_session_dir_path(self.sessions.path(), session_id);
        tddy_core::read_session_metadata(&dir).expect(".session.yaml must be readable")
    }

    /// The same sessions base served by a **fresh** daemon with a fresh provisioner: a restart.
    /// Its `.session.yaml` files survive; its jails do not — which is how a session recorded as
    /// sandboxed ends up with no jail registered for it until a resume re-provisions one.
    fn restarted_with_provisioner(
        &self,
        provisioner: Arc<dyn WorkspaceSandboxProvisioner>,
    ) -> ConnectionServiceImpl {
        test_service(self.sessions.path().to_path_buf())
            .with_workspace_sandbox_provisioner(provisioner)
    }
}

fn a_tool_request(session_id: &str, tool: &str, args: &str) -> ExecuteToolRequest {
    ExecuteToolRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: session_id.to_string(),
        daemon_instance_id: String::new(),
        tool_name: tool.to_string(),
        args_json: args.to_string(),
    }
}

/// Run one tool call against a bare service (the restarted daemon) and unwrap its result, the way
/// [`CodebaseHost::execute_tool`] does for the host that owns the tempdir.
async fn execute_tool_on(
    service: &ConnectionServiceImpl,
    session_id: &str,
    tool: &str,
    args: &str,
) -> ExecuteToolResponse {
    service
        .execute_tool(Request::new(a_tool_request(session_id, tool, args)))
        .await
        .expect("ExecuteTool must not fail at the RPC level")
        .into_inner()
}

/// Delete a session against a bare service (the restarted daemon).
async fn delete_on(service: &ConnectionServiceImpl, session_id: &str) {
    service
        .delete_session(Request::new(DeleteSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
        }))
        .await
        .expect("DeleteSession must succeed");
}

/// The `marker` a jailed result carries, or `None` when the result did not come from the jail.
fn jail_marker_of(response: &ExecuteToolResponse) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&response.result_json)
        .ok()?
        .get("marker")?
        .as_str()
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Resume re-provisions the jail
// ---------------------------------------------------------------------------

/// After a daemon restart, a sandboxed workspace session's jail is gone from the registry but its
/// `sandbox: Some(true)` metadata survives. A `ResumeSession` must re-provision the jail from that
/// persisted flag and the worktree path, so a subsequent tool call is confined again rather than
/// refused (or quietly run on the host worktree).
#[tokio::test]
async fn a_resume_re_provisions_the_jail_for_a_sandboxed_workspace_session_after_a_restart() {
    // Given a sandboxed workspace session, and a daemon that has since restarted with a fresh
    // provisioner holding no jail for it
    let first = CodebaseHost::with_provisioner(Arc::new(RecordingProvisioner::default()));
    let session_id = first.start_sandboxed_workspace().await;
    let restarted_provisioner = Arc::new(RecordingProvisioner::default());
    let restarted = first.restarted_with_provisioner(Arc::clone(&restarted_provisioner) as Arc<_>);
    assert_eq!(
        restarted_provisioner.provisioned(),
        Vec::<WorkspaceSandboxSpec>::new(),
        "a restarted daemon holds no jail until a resume re-provisions one"
    );

    // When the session is resumed on the restarted daemon
    restarted
        .resume_session(Request::new(ResumeSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("a sandboxed workspace session must resume by re-provisioning its jail");

    // Then — the jail was re-provisioned around the session's own worktree, and a tool call now
    // routes through it instead of the host tool engine. The persisted flag is what every later
    // dispatch reads; a resume that did not re-provision would leave the call refused (or, worse,
    // run on the bare host worktree the session was sandboxed to avoid).
    let provisioned = restarted_provisioner.provisioned();
    assert_eq!(
        provisioned.len(),
        1,
        "exactly one jail must be re-provisioned"
    );
    assert_eq!(provisioned[0].session_id, session_id);
    assert_eq!(provisioned[0].worktree_path, first.worktree_of(&session_id));

    let response = execute_tool_on(
        &restarted,
        &session_id,
        "Write",
        r#"{"path":"after-resume.txt","contents":"confined"}"#,
    )
    .await;
    assert_eq!(jail_marker_of(&response).as_deref(), Some(JAIL_MARKER));
    assert_eq!(
        restarted_provisioner.sandbox().calls(),
        vec![JailedCall {
            session_id: session_id.clone(),
            tool_name: "Write".to_string(),
            args_json: r#"{"path":"after-resume.txt","contents":"confined"}"#.to_string(),
        }]
    );
    assert!(
        !first
            .worktree_of(&session_id)
            .join("after-resume.txt")
            .exists(),
        "the re-provisioned jail must own this write; the host worktree must be untouched"
    );
}

/// The control: a sandboxed workspace session whose jail is still registered (no restart) is
/// already confined, so a resume must not pay for a second jail — it reuses the one it has. A resume
/// that re-provisioned unconditionally would leak a jail per resume.
#[tokio::test]
async fn a_resume_reuses_the_existing_jail_when_the_sandboxed_workspace_session_was_not_restarted()
{
    // Given a sandboxed workspace session whose jail is still in this daemon's registry
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);
    let session_id = host.start_sandboxed_workspace().await;
    assert_eq!(provisioner.provisioned().len(), 1);

    // When the session is resumed on the same daemon
    host.resume(&session_id)
        .await
        .expect("a sandboxed workspace session with a live jail must resume");

    // Then — no second jail was provisioned: the live one is the one a resumed dispatch routes
    // through, and re-provisioning would orphan the first.
    assert_eq!(
        provisioner.provisioned().len(),
        1,
        "a resume must reuse the live jail, not provision another"
    );

    let response = host
        .execute_tool(
            &session_id,
            "Write",
            r#"{"path":"after-resume.txt","contents":"confined"}"#,
        )
        .await;
    assert_eq!(jail_marker_of(&response).as_deref(), Some(JAIL_MARKER));
}

// ---------------------------------------------------------------------------
// Delete after a restart tears down the orphaned jail process
// ---------------------------------------------------------------------------

/// A jail that stands in for a real sandbox runner: it spawns a long-lived `sleep` child and
/// persists its pid under the session's sandbox tree, so a daemon that comes back after a crash can
/// find the orphan. `stop()` kills the child (the in-registry delete path); `Drop` does **not** — a
/// daemon that crashes leaves its jail runner behind, which is exactly the orphan this PR's
/// delete-time teardown is for.
struct LingeringProcessSandbox {
    child: Mutex<Option<std::process::Child>>,
}

#[async_trait]
impl WorkspaceSandbox for LingeringProcessSandbox {
    async fn execute_tool(&self, _req: &ExecuteToolRequest) -> ExecuteToolResponse {
        // The delete test never dispatches a tool; the jail exists only so its process lifetime is
        // observable.
        ExecuteToolResponse::default()
    }

    fn stop(&self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A provisioner that hands out one [`LingeringProcessSandbox`], spawning a real `sleep` child and
/// recording its pid under `<session_dir>/sandbox/runner.pid` — the contract the delete-time
/// teardown reads to kill an orphaned jail runner after a restart.
#[derive(Default)]
struct LingeringProcessProvisioner {
    spawned: Mutex<Option<u32>>,
}

impl LingeringProcessProvisioner {
    /// The pid of the child the provisioner spawned, captured before the daemon that owned it goes
    /// away so a test can observe whether the delete reached it.
    fn spawned_pid(&self) -> u32 {
        self.spawned
            .lock()
            .unwrap()
            .expect("a sandboxed workspace session must have provisioned a jail first")
    }
}

#[async_trait]
impl WorkspaceSandboxProvisioner for LingeringProcessProvisioner {
    async fn provision(
        &self,
        spec: &WorkspaceSandboxSpec,
    ) -> Result<Arc<dyn WorkspaceSandbox>, SandboxError> {
        let sandbox_dir = spec.session_dir.join("sandbox");
        std::fs::create_dir_all(&sandbox_dir).map_err(|e| {
            SandboxError::Io(format!("create sandbox dir {}: {e}", sandbox_dir.display()))
        })?;
        let child = std::process::Command::new("sleep")
            .arg("120")
            .spawn()
            .map_err(|e| SandboxError::Io(format!("spawn lingering jail runner: {e}")))?;
        let pid = child.id();
        std::fs::write(sandbox_dir.join(RUNNER_PID_FILE), pid.to_string())
            .map_err(|e| SandboxError::Io(format!("write runner pid: {e}")))?;
        *self.spawned.lock().unwrap() = Some(pid);
        Ok(Arc::new(LingeringProcessSandbox {
            child: Mutex::new(Some(child)),
        }) as Arc<dyn WorkspaceSandbox>)
    }
}

/// Whether the process `pid` is still alive, by asking the kernel to signal nothing at it.
fn pid_is_alive(pid: u32) -> bool {
    // SAFETY: `kill(pid, 0)` performs no signal delivery; it only checks for permission and
    // existence. ESRCH means the process is gone.
    let ret = unsafe { libc::kill(pid as i32, 0) };
    if ret == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// Kills `pid` on drop if it is still alive, so a test that fails before the delete reaches the
/// orphan does not leak a `sleep` process into the host running the suite.
struct OrphanGuard(u32);
impl Drop for OrphanGuard {
    fn drop(&mut self) {
        if pid_is_alive(self.0) {
            unsafe { libc::kill(self.0 as i32, libc::SIGKILL) };
        }
    }
}

/// Deleting a sandboxed workspace session whose jail is still in this daemon's registry already
/// stops the jail (proven in `workspace_tool_sandbox_acceptance.rs`). After a restart the registry
/// is empty and the jail runner the old daemon spawned is an orphan; `DeleteSession` must still
/// reach it — reading the pid the jail persisted — so no confined process outlives the session it
/// was built for.
#[tokio::test]
async fn deleting_a_sandboxed_workspace_session_after_a_restart_kills_the_orphaned_jail_runner() {
    // Given a sandboxed workspace session whose jail is a real lingering process, and a daemon that
    // has since restarted with no jail registered for the session
    let provisioner = Arc::new(LingeringProcessProvisioner::default());
    let first = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);
    let session_id = first.start_sandboxed_workspace().await;
    let orphan_pid = provisioner.spawned_pid();
    let _guard = OrphanGuard(orphan_pid);
    assert!(
        pid_is_alive(orphan_pid),
        "the sandboxed workspace session must have spawned a jail runner"
    );
    let restarted = first.restarted_with_provisioner(Arc::new(RecordingProvisioner::default()));

    // When the session is deleted on the restarted daemon
    delete_on(&restarted, &session_id).await;

    // Then — the orphaned jail runner is gone. A delete that only stopped jails in its own registry
    // would leave this process running against a checkout that no longer exists, for the rest of
    // the host's life.
    assert!(
        !pid_is_alive(orphan_pid),
        "deleting a sandboxed workspace session must kill the orphaned jail runner, not just \
         remove its directory"
    );
}
