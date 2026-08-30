//! Acceptance: workspace tool sandbox — the contracts that hold on every platform.
//!
//! PRD: `docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox.
//! Changeset: `docs/dev/1-WIP/2026-08-30-workspace-tool-sandbox.md`.
//!
//! What a sandboxed workspace session *routes* — which calls reach the jail, what is persisted,
//! what is refused, and in what order the start does its work — is decided by the daemon, not by
//! the kernel. Those contracts are proven here against an injected provisioner, so each one fails
//! for its own reason rather than for "this host has no Seatbelt".
//!
//! What the jail actually *confines* is the kernel's claim, not the daemon's, and is proven
//! against a real one: `workspace_tool_sandbox_seatbelt_acceptance.rs` (macOS Seatbelt) and
//! `tddy-e2e/tests/vm_workspace_tool_sandbox_acceptance.rs` (Linux cgroups, VM-backed).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::StreamExt;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_daemon::workspace_tool_sandbox::{
    WorkspaceSandbox, WorkspaceSandboxProvisioner, WorkspaceSandboxSpec,
};
use tddy_rpc::{Code, Request, Status};
use tddy_sandbox::SandboxError;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest, ExecuteToolRequest,
    ExecuteToolResponse, StartSessionRequest,
};

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a40";

/// The marker a jailed tool result carries. Its presence in a response proves the call went to the
/// jail; its absence proves it went to the host tool engine.
const JAIL_MARKER: &str = "ran-inside-the-workspace-jail";

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
    stops: AtomicUsize,
}

impl RecordingSandbox {
    fn calls(&self) -> Vec<JailedCall> {
        self.calls.lock().unwrap().clone()
    }

    fn stops(&self) -> usize {
        self.stops.load(Ordering::SeqCst)
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

    fn stop(&self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }
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

/// Stands in for a host whose OS has no sandbox backend at all.
struct UnsupportedPlatformProvisioner;

#[async_trait]
impl WorkspaceSandboxProvisioner for UnsupportedPlatformProvisioner {
    async fn provision(
        &self,
        _spec: &WorkspaceSandboxSpec,
    ) -> Result<Arc<dyn WorkspaceSandbox>, SandboxError> {
        Err(SandboxError::Unsupported {
            platform: "plan9".to_string(),
            message: "platform sandboxes are not available on this OS".to_string(),
        })
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
            name: "workspace-tool-sandbox".to_string(),
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

    async fn start(&self, req: WorkspaceStartBuilder) -> Result<String, Status> {
        self.service
            .start_session(Request::new(req.build()))
            .await
            .map(|resp| resp.into_inner().session_id)
    }

    async fn start_workspace(&self, sandbox: bool) -> Result<String, Status> {
        self.start(a_workspace_start().sandboxed(sandbox)).await
    }

    async fn execute_tool(&self, session_id: &str, tool: &str, args: &str) -> ExecuteToolResponse {
        self.service
            .execute_tool(Request::new(a_tool_request(session_id, tool, args)))
            .await
            .expect("ExecuteTool must not fail at the RPC level")
            .into_inner()
    }

    /// Drain `StreamExecuteTool` into the single result its frames carry.
    async fn stream_execute_tool(
        &self,
        session_id: &str,
        tool: &str,
        args: &str,
    ) -> ExecuteToolResponse {
        let stream = self
            .service
            .stream_execute_tool(Request::new(a_tool_request(session_id, tool, args)))
            .await
            .expect("StreamExecuteTool must not fail at the RPC level")
            .into_inner();
        let mut stream = Box::pin(stream);

        // The frames carry the result as bytes, split at a fixed size: a multi-byte character can
        // land across two of them, so they are joined before being read as a string.
        let mut result_bytes: Vec<u8> = Vec::new();
        let mut is_error = false;
        let mut error_message = String::new();
        while let Some(frame) = stream.next().await {
            let frame = frame.expect("no frame may carry an error");
            result_bytes.extend_from_slice(&frame.result_chunk);
            is_error |= frame.is_error;
            if !frame.error_message.is_empty() {
                error_message = frame.error_message;
            }
        }
        ExecuteToolResponse {
            result_json: String::from_utf8(result_bytes).expect("a tool result is UTF-8"),
            is_error,
            error_message,
            job_id: String::new(),
            job_running: false,
        }
    }

    fn worktree_of(&self, session_id: &str) -> PathBuf {
        PathBuf::from(
            self.metadata_of(session_id)
                .repo_path
                .expect("a workspace session must have a repo_path"),
        )
    }

    fn sandbox_flag_of(&self, session_id: &str) -> Option<bool> {
        self.metadata_of(session_id).sandbox
    }

    fn metadata_of(&self, session_id: &str) -> tddy_core::SessionMetadata {
        let dir = unified_session_dir_path(self.sessions.path(), session_id);
        tddy_core::read_session_metadata(&dir).expect(".session.yaml must be readable")
    }

    fn session_dir_exists(&self, session_id: &str) -> bool {
        unified_session_dir_path(self.sessions.path(), session_id).exists()
    }

    /// The same sessions base served by a **fresh** daemon: a restart. Its `.session.yaml` files
    /// survive, its jails do not — which is how a session recorded as sandboxed ends up with no
    /// jail registered for it.
    fn after_a_daemon_restart(&self) -> ConnectionServiceImpl {
        test_service(self.sessions.path().to_path_buf())
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

/// A workspace `StartSession` request, with only the fields a test cares about spelled out.
struct WorkspaceStartBuilder {
    sandbox: bool,
    semantic_index: bool,
    requested_session_id: String,
}

fn a_workspace_start() -> WorkspaceStartBuilder {
    WorkspaceStartBuilder {
        sandbox: false,
        semantic_index: false,
        requested_session_id: String::new(),
    }
}

impl WorkspaceStartBuilder {
    fn sandboxed(mut self, sandbox: bool) -> Self {
        self.sandbox = sandbox;
        self
    }

    fn with_semantic_index(mut self) -> Self {
        self.semantic_index = true;
        self
    }

    fn with_requested_session_id(mut self, id: &str) -> Self {
        self.requested_session_id = id.to_string();
        self
    }

    fn build(self) -> StartSessionRequest {
        StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_type: "workspace".to_string(),
            project_id: PROJECT_ID.to_string(),
            sandbox: self.sandbox,
            semantic_index: self.semantic_index,
            requested_session_id: self.requested_session_id,
            ..Default::default()
        }
    }
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
// The flag is accepted and persisted
// ---------------------------------------------------------------------------

/// A `workspace` start is the codebase host's half of a sandboxed session, so the flag it was
/// asked for has to survive in the metadata — it is what every later dispatch reads to decide
/// whether a tool goes to the jail.
#[tokio::test]
async fn a_workspace_session_started_with_sandbox_records_sandbox_true_in_its_metadata() {
    // Given
    let host = CodebaseHost::with_provisioner(Arc::new(RecordingProvisioner::default()));

    // When
    let session_id = host
        .start_workspace(true)
        .await
        .expect("a sandboxed workspace start must be accepted");

    // Then
    assert_eq!(host.sandbox_flag_of(&session_id), Some(true));
}

/// The control: a plain workspace session stays recognisably unsandboxed, or every later dispatch
/// would route through a jail nobody asked for.
#[tokio::test]
async fn a_workspace_session_started_without_sandbox_records_no_sandbox_in_its_metadata() {
    // Given
    let host = CodebaseHost::with_provisioner(Arc::new(RecordingProvisioner::default()));

    // When
    let session_id = host
        .start_workspace(false)
        .await
        .expect("an unsandboxed workspace start must be accepted");

    // Then
    assert_eq!(host.sandbox_flag_of(&session_id), None);
}

/// The jail holds the session's own checkout, at the path the session's metadata names — a jail
/// built around any other directory would confine the wrong tree.
#[tokio::test]
async fn the_jail_is_provisioned_around_the_sessions_own_worktree() {
    // Given
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);

    // When
    let session_id = host
        .start_workspace(true)
        .await
        .expect("start must succeed");

    // Then
    let provisioned = provisioner.provisioned();
    assert_eq!(
        provisioned.len(),
        1,
        "exactly one jail per sandboxed session"
    );
    assert_eq!(provisioned[0].session_id, session_id);
    assert_eq!(provisioned[0].worktree_path, host.worktree_of(&session_id));
    assert_eq!(
        provisioned[0].session_dir,
        unified_session_dir_path(host.sessions.path(), &session_id)
    );
}

/// An unsandboxed workspace session must not pay for a jail it will never dispatch through.
#[tokio::test]
async fn no_jail_is_provisioned_for_an_unsandboxed_workspace_session() {
    // Given
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);

    // When
    host.start_workspace(false)
        .await
        .expect("start must succeed");

    // Then
    assert_eq!(provisioner.provisioned(), vec![]);
}

// ---------------------------------------------------------------------------
// Dispatch reaches the jail, not the host tool engine
// ---------------------------------------------------------------------------

/// The whole feature in one assertion: the daemon serves `ExecuteTool` from the jail, and the host
/// worktree it would otherwise have written to is untouched.
#[tokio::test]
async fn execute_tool_on_a_sandboxed_workspace_session_runs_in_the_jail_not_on_the_host_worktree() {
    // Given
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);
    let session_id = host
        .start_workspace(true)
        .await
        .expect("start must succeed");

    // When
    let response = host
        .execute_tool(
            &session_id,
            "Write",
            r#"{"path":"from-the-jail.txt","contents":"hello"}"#,
        )
        .await;

    // Then
    assert_eq!(jail_marker_of(&response).as_deref(), Some(JAIL_MARKER));
    assert_eq!(
        provisioner.sandbox.calls(),
        vec![JailedCall {
            session_id: session_id.clone(),
            tool_name: "Write".to_string(),
            args_json: r#"{"path":"from-the-jail.txt","contents":"hello"}"#.to_string(),
        }]
    );
    assert!(
        !host
            .worktree_of(&session_id)
            .join("from-the-jail.txt")
            .exists(),
        "the host tool engine must not have run: the jail owns this write"
    );
}

/// `Shell` is the tool the engine's own path checks do not bound, so it is the one the jail exists
/// for. It takes the same route as every other tool.
#[tokio::test]
async fn a_shell_tool_on_a_sandboxed_workspace_session_is_dispatched_to_the_jail() {
    // Given
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);
    let session_id = host
        .start_workspace(true)
        .await
        .expect("start must succeed");

    // When
    let response = host
        .execute_tool(&session_id, "Shell", r#"{"command":"echo hi"}"#)
        .await;

    // Then
    assert_eq!(jail_marker_of(&response).as_deref(), Some(JAIL_MARKER));
    assert_eq!(
        provisioner.sandbox.calls(),
        vec![JailedCall {
            session_id,
            tool_name: "Shell".to_string(),
            args_json: r#"{"command":"echo hi"}"#.to_string(),
        }]
    );
}

/// The streaming sibling shares routing, auth and worktree resolution with the unary handler
/// precisely so the two cannot drift — including over which side of the jail boundary a tool runs.
#[tokio::test]
async fn stream_execute_tool_on_a_sandboxed_workspace_session_is_dispatched_to_the_jail() {
    // Given
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);
    let session_id = host
        .start_workspace(true)
        .await
        .expect("start must succeed");

    // When
    let response = host
        .stream_execute_tool(
            &session_id,
            "Write",
            r#"{"path":"streamed.txt","contents":"hello"}"#,
        )
        .await;

    // Then
    assert_eq!(jail_marker_of(&response).as_deref(), Some(JAIL_MARKER));
    assert_eq!(
        provisioner.sandbox.calls(),
        vec![JailedCall {
            session_id: session_id.clone(),
            tool_name: "Write".to_string(),
            args_json: r#"{"path":"streamed.txt","contents":"hello"}"#.to_string(),
        }]
    );
    assert!(
        !host.worktree_of(&session_id).join("streamed.txt").exists(),
        "the host tool engine must not have run for the streaming path either"
    );
}

/// The control that makes every assertion above mean something: without the flag, the same call
/// still runs on the host worktree and never reaches a jail.
#[tokio::test]
async fn execute_tool_on_an_unsandboxed_workspace_session_still_runs_on_the_host_worktree() {
    // Given
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);
    let session_id = host
        .start_workspace(false)
        .await
        .expect("start must succeed");

    // When
    let response = host
        .execute_tool(
            &session_id,
            "Write",
            r#"{"path":"on-the-host.txt","contents":"hello"}"#,
        )
        .await;

    // Then
    assert!(!response.is_error, "{}", response.error_message);
    assert_eq!(jail_marker_of(&response), None);
    assert_eq!(provisioner.sandbox.calls(), vec![]);
    assert!(
        host.worktree_of(&session_id)
            .join("on-the-host.txt")
            .exists(),
        "an unsandboxed workspace session writes straight to its worktree"
    );
}

// ---------------------------------------------------------------------------
// Refusal, with nothing left behind
// ---------------------------------------------------------------------------

/// A host that cannot build a jail cannot serve a sandboxed session. Refused rather than silently
/// downgraded: a session that came up running its tools on the bare host is indistinguishable from
/// the confined one that was asked for.
#[tokio::test]
async fn a_sandboxed_workspace_start_on_an_unsupported_platform_is_refused_with_failed_precondition(
) {
    // Given
    let host = CodebaseHost::with_provisioner(Arc::new(UnsupportedPlatformProvisioner));

    // When
    let status = host
        .start_workspace(true)
        .await
        .expect_err("a start that cannot be confined must not succeed");

    // Then
    assert_eq!(status.code, Code::FailedPrecondition);
    assert!(
        status.message.contains("plan9"),
        "the refusal must name the platform that has no sandbox, got: {}",
        status.message
    );
}

/// The refusal has to be atomic. A session directory surviving a start that answered with an error
/// is a session the operator can see, list and resume, whose tools were never confined.
#[tokio::test]
async fn a_sandboxed_workspace_start_that_cannot_be_confined_leaves_no_session_behind() {
    // Given
    let host = CodebaseHost::with_provisioner(Arc::new(UnsupportedPlatformProvisioner));
    let chosen_id = "019d105b-ac0f-78d3-9a89-409731145a41";

    // When
    host.start(
        a_workspace_start()
            .sandboxed(true)
            .with_requested_session_id(chosen_id),
    )
    .await
    .expect_err("a start that cannot be confined must not succeed");

    // Then
    assert!(
        !host.session_dir_exists(chosen_id),
        "the session directory must be unwound with the failed start"
    );
}

// ---------------------------------------------------------------------------
// Ordering: the index is built on the host worktree, before the jail exists
// ---------------------------------------------------------------------------

/// Indexing reads the host worktree directly, so it belongs before the jail rather than through
/// it. The observable consequence: an index that cannot be built stops the start early enough that
/// no jail was ever provisioned.
#[tokio::test]
async fn a_semantic_index_that_cannot_be_built_stops_the_start_before_any_jail_is_provisioned() {
    // Given a host with no embedder available, so indexing is the step that fails
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);

    // When
    host.start(a_workspace_start().sandboxed(true).with_semantic_index())
        .await
        .expect_err("a start whose index cannot be built must not succeed");

    // Then
    assert_eq!(
        provisioner.provisioned(),
        vec![],
        "the jail is provisioned after indexing, so a failed index provisions none"
    );
}

/// A jail is process-local; the `sandbox: true` in `.session.yaml` outlives it. After a restart the
/// session still says its tools must be confined and this daemon holds nothing to confine them
/// with — so the call is refused. Serving it from the bare host instead would be the one failure
/// nobody can see afterwards: the tool succeeds, the operator sees a sandboxed session, and
/// nothing records that the boundary was not there. Re-provisioning on resume is
/// `split-sandbox-resume`'s to add; until it exists, refusing is the only honest answer.
#[tokio::test]
async fn a_sandboxed_workspace_session_whose_jail_is_gone_is_refused_rather_than_run_on_the_host() {
    // Given a sandboxed workspace session, and a daemon that has since restarted
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);
    let session_id = host
        .start_workspace(true)
        .await
        .expect("start must succeed");
    let restarted = host.after_a_daemon_restart();

    // When
    let response = restarted
        .execute_tool(Request::new(a_tool_request(
            &session_id,
            "Write",
            r#"{"path":"after-restart.txt","contents":"hello"}"#,
        )))
        .await
        .expect("the call is answered, not dropped: a refusal is a result, not an RPC failure")
        .into_inner();

    // Then
    assert!(
        response.is_error,
        "a sandboxed session with no jail must be refused, got: {}",
        response.result_json
    );
    assert!(
        !host
            .worktree_of(&session_id)
            .join("after-restart.txt")
            .exists(),
        "the refusal must not have run the tool on the host worktree"
    );
}

/// The control: an *unsandboxed* workspace session is unaffected by a restart, so the refusal
/// above is about the missing jail and not about restarting.
#[tokio::test]
async fn an_unsandboxed_workspace_session_still_serves_its_tools_after_a_daemon_restart() {
    // Given
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);
    let session_id = host
        .start_workspace(false)
        .await
        .expect("start must succeed");
    let restarted = host.after_a_daemon_restart();

    // When
    let response = restarted
        .execute_tool(Request::new(a_tool_request(
            &session_id,
            "Write",
            r#"{"path":"after-restart.txt","contents":"hello"}"#,
        )))
        .await
        .expect("ExecuteTool must not fail at the RPC level")
        .into_inner();

    // Then
    assert!(!response.is_error, "{}", response.error_message);
    assert!(host
        .worktree_of(&session_id)
        .join("after-restart.txt")
        .exists());
}

/// The jail is a live process holding the worktree open, and the registry is the only thing keeping
/// it alive — so a jail still registered after its session is gone runs on against a deleted
/// checkout for the rest of the daemon's life. Deleting the session has to take it too.
#[tokio::test]
async fn deleting_a_sandboxed_workspace_session_stops_its_jail() {
    // Given
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);
    let session_id = host
        .start_workspace(true)
        .await
        .expect("start must succeed");
    assert_eq!(
        provisioner.sandbox.stops(),
        0,
        "a jail serving a live session must not have been stopped"
    );

    // When
    host.service
        .delete_session(Request::new(DeleteSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("DeleteSession must succeed");

    // Then
    assert_eq!(provisioner.sandbox.stops(), 1);
}

/// And the jail is forgotten, not merely stopped: a tool call arriving after the delete must not
/// find a registered jail whose process is gone.
#[tokio::test]
async fn a_deleted_sandboxed_workspace_session_no_longer_serves_tools_from_its_jail() {
    // Given
    let provisioner = Arc::new(RecordingProvisioner::default());
    let host = CodebaseHost::with_provisioner(Arc::clone(&provisioner) as Arc<_>);
    let session_id = host
        .start_workspace(true)
        .await
        .expect("start must succeed");
    host.service
        .delete_session(Request::new(DeleteSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.clone(),
        }))
        .await
        .expect("DeleteSession must succeed");

    // When
    let answered = host
        .service
        .execute_tool(Request::new(a_tool_request(
            &session_id,
            "Write",
            r#"{"path":"after-delete.txt","contents":"hello"}"#,
        )))
        .await;

    // Then — the session is gone, so its worktree no longer resolves and the call is refused
    // outright; either way no tool reached the jail that used to serve it.
    assert!(answered.is_err() || answered.expect("checked").into_inner().is_error);
    assert_eq!(provisioner.sandbox.calls(), vec![]);
}
