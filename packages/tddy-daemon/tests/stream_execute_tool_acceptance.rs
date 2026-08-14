//! Acceptance: `StreamExecuteTool` — carrying a tool result in bounded frames.
//!
//! The unary `ExecuteTool` returns `result_json` as one string. Over LiveKit any payload above
//! `MAX_CHUNK_FRAME_BYTES` (60 000) is chunk-framed, and reassembly is best-effort and index-keyed:
//! a lost frame wedges the call permanently with **no error**. A `Read` of a large file or a broad
//! `Grep` crosses that threshold on day one of a split session, which is why the split path needs a
//! streaming variant rather than the unary call.
//!
//! The discipline is the one `StreamReadHostDocument` already proves: a frame budget with headroom
//! for the envelope, pinned by an assert so nobody raises it past what the transport can carry.
//!
//! PRD: `docs/ft/daemon/remote-managed-worktree.md`

use std::path::Path;

use futures_util::StreamExt;
use tddy_daemon::connection_service::EXEC_TOOL_FRAME_BYTES;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ExecuteToolRequest, StartSessionRequest,
};

const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a39";

/// Comfortably more than one frame, so the reassembly path is genuinely exercised.
const LARGE_FILE_BYTES: usize = EXEC_TOOL_FRAME_BYTES * 3 + 1_024;

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
            name: "stream-exec-tool".to_string(),
            git_url: String::new(),
            main_repo_path: repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
        }],
    )
    .expect("register project");
}

/// A workspace session plus the service that owns it — the unit the exec tools run against.
struct Workspace {
    service: tddy_daemon::connection_service::ConnectionServiceImpl,
    session_id: String,
    worktree: std::path::PathBuf,
    _repo: tempfile::TempDir,
    _sessions: tempfile::TempDir,
}

async fn a_workspace_session() -> Workspace {
    let repo = a_git_repo_with_origin();
    let sessions = tempfile::tempdir().unwrap();
    register_project(sessions.path(), repo.path());
    let service = test_service(sessions.path().to_path_buf());

    let started = service
        .start_session(Request::new(StartSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: PROJECT_ID.to_string(),
            session_type: "workspace".to_string(),
            ..Default::default()
        }))
        .await
        .expect("workspace session must start")
        .into_inner();

    let session_dir = tddy_core::session_lifecycle::unified_session_dir_path(
        sessions.path(),
        &started.session_id,
    );
    let worktree = std::path::PathBuf::from(
        tddy_core::read_session_metadata(&session_dir)
            .expect("session metadata")
            .repo_path
            .expect("workspace worktree"),
    );

    Workspace {
        service,
        session_id: started.session_id,
        worktree,
        _repo: repo,
        _sessions: sessions,
    }
}

fn a_read_request(session_id: &str, path: &str) -> ExecuteToolRequest {
    ExecuteToolRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: session_id.to_string(),
        tool_name: "Read".to_string(),
        args_json: serde_json::json!({ "path": path }).to_string(),
        daemon_instance_id: String::new(),
    }
}

/// What a drained `StreamExecuteTool` response actually carried.
///
/// `frame_sizes` is kept because reassembling correctly is only half the contract: a handler that
/// returned the whole result in one oversized frame would reassemble byte-for-byte and still
/// reintroduce the silent chunk-wedge this RPC exists to remove. A test that only concatenates
/// cannot tell those apart.
struct DrainedResult {
    result_json: String,
    is_error: bool,
    error_message: String,
    frame_sizes: Vec<usize>,
}

async fn drain_result(
    mut stream: impl futures_util::Stream<
            Item = Result<tddy_service::proto::connection::ExecuteToolChunk, tddy_rpc::Status>,
        > + Unpin,
) -> DrainedResult {
    let mut bytes: Vec<u8> = Vec::new();
    let mut frame_sizes: Vec<usize> = Vec::new();
    let mut is_error = false;
    let mut error_message = String::new();
    let mut saw_last = false;
    while let Some(frame) = stream.next().await {
        let frame = frame.expect("every frame must decode");
        assert!(
            !saw_last,
            "no frame may follow the one marked `last` — a consumer stops reading there"
        );
        frame_sizes.push(frame.result_chunk.len());
        bytes.extend_from_slice(&frame.result_chunk);
        if frame.last {
            saw_last = true;
            is_error = frame.is_error;
            error_message = frame.error_message;
        }
    }
    assert!(
        saw_last,
        "the stream must end with a frame marked `last`; a stream that just stops is \
         indistinguishable from a truncated one"
    );
    DrainedResult {
        result_json: String::from_utf8(bytes).expect("result_json must be valid UTF-8"),
        is_error,
        error_message,
        frame_sizes,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The budget's relationship to the transport limit is already pinned at compile time in production
/// (`connection_service.rs`, with stricter headroom), so restating it here would prove nothing a
/// build does not already prove. What no compile-time assert can check is whether the handler
/// *honours* the budget when it splits a real result — which is the next test.

#[tokio::test]
async fn a_result_larger_than_one_frame_reassembles_byte_for_byte() {
    // Given a file several frames long in the session's worktree
    let workspace = a_workspace_session().await;
    let content = "abcdefghij".repeat(LARGE_FILE_BYTES / 10);
    std::fs::write(workspace.worktree.join("large.txt"), &content).expect("seed large file");

    // When it is read over the streaming RPC
    let stream = workspace
        .service
        .stream_execute_tool(Request::new(a_read_request(
            &workspace.session_id,
            "large.txt",
        )))
        .await
        .expect("StreamExecuteTool must be accepted")
        .into_inner();
    let drained = drain_result(Box::pin(stream)).await;

    // Then — every byte arrived, in order
    assert!(
        !drained.is_error,
        "the read must succeed; error was '{}'",
        drained.error_message
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&drained.result_json).expect("reassembled result must be valid JSON");
    assert_eq!(
        parsed["content"].as_str().expect("content field"),
        content,
        "the reassembled file content must match what was written"
    );

    // And it arrived as *bounded* frames. Reassembly alone is not the contract: a handler that
    // returned this result in one oversized frame would satisfy every assertion above and put the
    // payload straight back into the transport's chunk-framing, where a lost frame wedges the call
    // with no error. That is the failure this RPC was added to remove.
    assert!(
        drained.frame_sizes.len() >= 4,
        "a result of {} bytes must span several frames at a {EXEC_TOOL_FRAME_BYTES}-byte budget; \
         got {} frame(s): {:?}",
        content.len(),
        drained.frame_sizes.len(),
        drained.frame_sizes
    );
    assert!(
        drained
            .frame_sizes
            .iter()
            .all(|size| *size <= EXEC_TOOL_FRAME_BYTES),
        "no frame may exceed the budget of {EXEC_TOOL_FRAME_BYTES} bytes; got {:?}",
        drained.frame_sizes
    );
}

#[tokio::test]
async fn a_streamed_tool_result_equals_the_unary_result_for_the_same_call() {
    // Given a file small enough to fit in a single frame either way
    let workspace = a_workspace_session().await;
    std::fs::write(
        workspace.worktree.join("small.txt"),
        "just a little content\n",
    )
    .expect("seed small file");

    // When the same call is made over both RPCs
    let unary = workspace
        .service
        .execute_tool(Request::new(a_read_request(
            &workspace.session_id,
            "small.txt",
        )))
        .await
        .expect("unary ExecuteTool")
        .into_inner();
    let stream = workspace
        .service
        .stream_execute_tool(Request::new(a_read_request(
            &workspace.session_id,
            "small.txt",
        )))
        .await
        .expect("StreamExecuteTool")
        .into_inner();
    let drained = drain_result(Box::pin(stream)).await;

    // Then — the streaming variant is a transport change, not a semantic one.
    // Both outcomes are asserted before comparing them: two calls that failed the same way would
    // both carry an empty `result_json`, and an equality check alone would certify "streaming
    // matches unary" for a pair where neither actually read the file.
    assert!(
        !drained.is_error,
        "the streamed read must succeed; error was '{}'",
        drained.error_message
    );
    assert!(
        !unary.is_error,
        "the unary read must succeed; error was '{}'",
        unary.error_message
    );
    assert_eq!(
        drained.result_json, unary.result_json,
        "the streamed result must be identical to the unary one for the same tool call"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&drained.result_json).expect("result must be valid JSON");
    assert_eq!(
        parsed["content"].as_str().expect("content field"),
        "just a little content\n",
        "both calls must have returned the seeded file, not a shared empty result"
    );
}

#[tokio::test]
async fn a_tool_error_is_reported_on_the_final_frame_rather_than_as_a_stream_error() {
    // Given a workspace session
    let workspace = a_workspace_session().await;

    // When an unknown tool is invoked
    let stream = workspace
        .service
        .stream_execute_tool(Request::new(ExecuteToolRequest {
            tool_name: "NoSuchTool".to_string(),
            ..a_read_request(&workspace.session_id, "irrelevant.txt")
        }))
        .await
        .expect("an unknown tool name must open the stream, not refuse it")
        .into_inner();
    let drained = drain_result(Box::pin(stream)).await;

    // Then — matching unary `ExecuteTool`'s contract: a tool failure is a *result*, and only routing
    // or auth failures are RPC errors. An agent must be able to tell those apart.
    assert!(
        drained.is_error,
        "an unknown tool must be reported as a tool error"
    );
    assert!(
        drained.error_message.contains("NoSuchTool"),
        "the error must name the tool that was not found; got '{}'",
        drained.error_message
    );
}
