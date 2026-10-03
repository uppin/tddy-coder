//! Acceptance: a `ConversationWorktree` request relayed out of a jail is served by the host's
//! bridge — the route an in-jail `tddy-tools` takes over the sandbox session channel, which has no
//! session token and is bound to its session by the runner instead.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-isolated-edits.md

use std::path::Path;
use std::process::Command;

use pretty_assertions::assert_eq;
use prost::Message as _;
use serde_json::json;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_rpc::test_util::test_service;
use tddy_rpc::{Code, Request, RpcResult};
use tddy_service::proto::exec_tools::{
    conversation_worktree_request::Op, ConversationWorktreeRequest, ConversationWorktreeResponse,
    ExecToolService, ExecuteToolRequest, PullOp,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::a_session_metadata;

const SESSION_ID: &str = "host-bridge-session";
const COORDINATE: (&str, &str) = ("exec_tools.ExecToolService", "ConversationWorktree");

struct ADaemonWithAGitSession {
    _sessions: tempfile::TempDir,
    session_dir: std::path::PathBuf,
    _worktree: tempfile::TempDir,
    worktree: std::path::PathBuf,
    daemon: tddy_daemon_rpc::test_util::TestDaemon,
}

fn a_daemon_with_a_git_session() -> ADaemonWithAGitSession {
    let sessions = tempfile::tempdir().expect("sessions dir");
    let worktree_dir = tempfile::tempdir().expect("worktree dir");
    let worktree = worktree_dir.path().to_path_buf();
    for args in [
        &["init", "-q", "-b", "master"][..],
        &["config", "user.email", "developer@example.com"][..],
        &["config", "user.name", "Developer"][..],
        &["commit", "-q", "--allow-empty", "-m", "initial"][..],
    ] {
        git(&worktree, args);
    }
    let session_dir = unified_session_dir_path(sessions.path(), SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let metadata = a_session_metadata()
        .with_session_id(SESSION_ID)
        .with_project_id("proj-1")
        .with_repo_path(worktree.to_str().expect("utf-8"))
        .build();
    tddy_core::write_session_metadata(&session_dir, &metadata).expect("session metadata");
    let daemon = test_service(sessions.path().to_path_buf());
    daemon.as_arc().install_sandbox_rpc_bridge();
    ADaemonWithAGitSession {
        _sessions: sessions,
        session_dir,
        _worktree: worktree_dir,
        worktree,
        daemon,
    }
}

impl ADaemonWithAGitSession {
    async fn a_conversation_that_wrote(&self, conversation_id: &str) {
        self.daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                tool_name: "Write".to_string(),
                args_json: json!({ "path": "src/new.rs", "contents": "pub fn new() {}\n" })
                    .to_string(),
                daemon_instance_id: String::new(),
                conversation_id: conversation_id.to_string(),
            }))
            .await
            .expect("the conversation writes");
    }

    /// What the jail's runner relays: no token, the jail's own session id.
    async fn relayed_pull(
        &self,
        conversation_id: &str,
    ) -> Result<serde_json::Value, tddy_rpc::Status> {
        self.pull_naming_session(SESSION_ID, conversation_id).await
    }

    /// A pull sent to the bridge of the jail built for `SESSION_ID`, naming `session_id` — what
    /// a process inside the jail that bypassed the runner's rewrite could send.
    async fn pull_naming_session(
        &self,
        session_id: &str,
        conversation_id: &str,
    ) -> Result<serde_json::Value, tddy_rpc::Status> {
        let request = ConversationWorktreeRequest {
            session_token: String::new(),
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: conversation_id.to_string(),
            op: Some(Op::Pull(PullOp {})),
        };
        let RpcResult::Unary(answer) = self
            .daemon
            .sandbox_rpc_handler(SESSION_ID, &self.session_dir)
            .handle_rpc(COORDINATE.0, COORDINATE.1, &request.encode_to_vec())
            .await
        else {
            panic!("ConversationWorktree is a unary call");
        };
        answer.map(|bytes| {
            let response = ConversationWorktreeResponse::decode(bytes.as_slice()).expect("decode");
            serde_json::from_str(&response.result_json).expect("result JSON")
        })
    }
}

#[tokio::test]
async fn a_relayed_pull_hands_the_conversations_work_to_the_session_worktree() {
    // Given
    let host = a_daemon_with_a_git_session();
    host.a_conversation_that_wrote("explore").await;

    // When
    let answer = host
        .relayed_pull("explore")
        .await
        .expect("the bridge serves it");

    // Then
    assert_eq!(
        (
            std::fs::read_to_string(host.worktree.join("src/new.rs")).expect("pulled"),
            answer["pulled"]["files"].clone(),
        ),
        (
            "pub fn new() {}\n".to_string(),
            json!({ "created": 1, "updated": 0, "removed": 0 }),
        )
    );
}

#[tokio::test]
async fn a_relayed_call_naming_an_unsafe_conversation_is_refused() {
    // Given
    let host = a_daemon_with_a_git_session();

    // When
    let refused = host
        .relayed_pull("../escape")
        .await
        .expect_err("an unsafe conversation id is refused");

    // Then
    assert_eq!(refused.code(), Code::InvalidArgument);
}

#[tokio::test]
async fn a_relayed_call_naming_another_session_is_refused() {
    // Given — the conversation has work the session worktree has not pulled
    let host = a_daemon_with_a_git_session();
    host.a_conversation_that_wrote("explore").await;

    // When
    let refused = host
        .pull_naming_session("another-session", "explore")
        .await
        .expect_err("a session other than the jail's own is refused");

    // Then
    assert_eq!(
        (refused.code(), host.worktree.join("src/new.rs").exists()),
        (Code::PermissionDenied, false)
    );
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
