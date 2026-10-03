//! Acceptance: `ExecuteTool` carrying a `conversation_id` runs in that conversation's own
//! worktree, and `ConversationWorktree` hands the work back or removes it.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md

use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use serde_json::json;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_rpc::test_util::test_service;
use tddy_rpc::{Code, Request};
use tddy_service::proto::exec_tools::{
    conversation_worktree_request::Op, ConversationWorktreeRequest, ExecToolService,
    ExecuteToolRequest, PullOp, RemoveOp,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::a_session_metadata;

const SESSION_ID: &str = "conversation-worktree-session";

/// A daemon serving one session whose worktree is a real git checkout.
struct ADaemonWithASession {
    _sessions: tempfile::TempDir,
    _worktree: tempfile::TempDir,
    worktree: PathBuf,
    service: tddy_daemon_rpc::test_util::TestDaemon,
}

fn a_daemon_with_a_git_session() -> ADaemonWithASession {
    let sessions = tempfile::tempdir().expect("sessions dir");
    let worktree_dir = tempfile::tempdir().expect("worktree dir");
    let worktree = worktree_dir.path().to_path_buf();
    git(&worktree, &["init", "-q", "-b", "master"]);
    git(
        &worktree,
        &["config", "user.email", "developer@example.com"],
    );
    git(&worktree, &["config", "user.name", "Developer"]);
    std::fs::write(worktree.join("README.md"), "hello\n").expect("seed file");
    git(&worktree, &["add", "-A"]);
    git(&worktree, &["commit", "-q", "-m", "initial"]);

    let session_dir = unified_session_dir_path(sessions.path(), SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let metadata = a_session_metadata()
        .with_session_id(SESSION_ID)
        .with_project_id("proj-1")
        .with_repo_path(worktree.to_str().expect("utf-8"))
        .build();
    tddy_core::write_session_metadata(&session_dir, &metadata).expect("session metadata");

    let service = test_service(sessions.path().to_path_buf());
    ADaemonWithASession {
        _sessions: sessions,
        _worktree: worktree_dir,
        worktree,
        service,
    }
}

impl ADaemonWithASession {
    async fn tool_call(
        &self,
        conversation_id: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> serde_json::Value {
        let response = self
            .service
            .execute_tool(Request::direct(an_execute_tool(
                conversation_id,
                tool,
                args,
            )))
            .await
            .expect("ExecuteTool answers");
        serde_json::from_str(&response.get_ref().result_json).expect("result JSON")
    }

    async fn conversation_worktree(&self, conversation_id: &str, op: Op) -> serde_json::Value {
        let response = self
            .service
            .conversation_worktree(Request::direct(a_conversation_worktree_request(
                TEST_TOKEN,
                conversation_id,
                op,
            )))
            .await
            .expect("ConversationWorktree answers");
        serde_json::from_str(&response.get_ref().result_json).expect("result JSON")
    }

    fn conversation_root(&self, conversation_id: &str) -> PathBuf {
        self.worktree
            .join("tmp/subagent-worktrees")
            .join(conversation_id)
    }
}

fn an_execute_tool(
    conversation_id: &str,
    tool: &str,
    args: serde_json::Value,
) -> ExecuteToolRequest {
    ExecuteToolRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        tool_name: tool.to_string(),
        args_json: args.to_string(),
        daemon_instance_id: String::new(),
        conversation_id: conversation_id.to_string(),
    }
}

fn a_conversation_worktree_request(
    token: &str,
    conversation_id: &str,
    op: Op,
) -> ConversationWorktreeRequest {
    ConversationWorktreeRequest {
        session_token: token.to_string(),
        session_id: SESSION_ID.to_string(),
        daemon_instance_id: String::new(),
        conversation_id: conversation_id.to_string(),
        op: Some(op),
    }
}

#[tokio::test]
async fn an_execute_tool_carrying_a_conversation_id_writes_to_that_conversations_worktree() {
    // Given
    let daemon = a_daemon_with_a_git_session();

    // When
    let result = daemon
        .tool_call(
            "explore",
            "Write",
            json!({ "path": "src/new.rs", "contents": "pub fn new() {}\n" }),
        )
        .await;

    // Then
    assert_eq!(
        (
            std::fs::read_to_string(daemon.conversation_root("explore").join("src/new.rs"))
                .expect("written in the conversation worktree"),
            daemon.worktree.join("src/new.rs").exists(),
            result["worktreeChange"].clone(),
        ),
        (
            "pub fn new() {}\n".to_string(),
            false,
            json!({
                "commit": short_head(&daemon.conversation_root("explore")),
                "files": { "created": 1, "updated": 0, "removed": 0 },
                "lines": { "added": 1, "removed": 0 }
            }),
        )
    );
}

/// Guards the unchanged path: no `conversation_id`, no conversation worktree, no `worktreeChange`.
#[tokio::test]
async fn an_execute_tool_without_a_conversation_id_writes_to_the_session_worktree() {
    // Given
    let daemon = a_daemon_with_a_git_session();

    // When
    let result = daemon
        .tool_call("", "Write", json!({ "path": "a.txt", "contents": "a\n" }))
        .await;

    // Then
    assert_eq!(
        (
            std::fs::read_to_string(daemon.worktree.join("a.txt")).expect("written in place"),
            daemon.worktree.join("tmp/subagent-worktrees").exists(),
            result.get("worktreeChange").is_none(),
        ),
        ("a\n".to_string(), false, true)
    );
}

#[tokio::test]
async fn an_unsafe_conversation_id_is_refused_before_any_tool_runs() {
    // Given
    let daemon = a_daemon_with_a_git_session();

    // When
    let refused = daemon
        .service
        .execute_tool(Request::direct(an_execute_tool(
            "../escape",
            "Write",
            json!({ "path": "a.txt", "contents": "a\n" }),
        )))
        .await
        .expect_err("an unsafe conversation id is a routing failure");

    // Then
    assert_eq!(
        (refused.code(), daemon.worktree.join("a.txt").exists()),
        (Code::InvalidArgument, false)
    );
}

#[tokio::test]
async fn pull_hands_the_conversations_changes_to_the_session_worktree() {
    // Given
    let daemon = a_daemon_with_a_git_session();
    daemon
        .tool_call(
            "explore",
            "Write",
            json!({ "path": "src/new.rs", "contents": "pub fn new() {}\n" }),
        )
        .await;

    // When
    let answer = daemon
        .conversation_worktree("explore", Op::Pull(PullOp {}))
        .await;

    // Then
    assert_eq!(
        (
            std::fs::read_to_string(daemon.worktree.join("src/new.rs"))
                .expect("pulled into the session worktree"),
            answer["pulled"]["files"].clone(),
            answer["pulled"]["conflicts"].clone(),
        ),
        (
            "pub fn new() {}\n".to_string(),
            json!({ "created": 1, "updated": 0, "removed": 0 }),
            json!([]),
        )
    );
}

#[tokio::test]
async fn pulling_a_conversation_that_never_wrote_pulls_nothing() {
    // Given
    let daemon = a_daemon_with_a_git_session();

    // When
    let answer = daemon
        .conversation_worktree("explore", Op::Pull(PullOp {}))
        .await;

    // Then
    assert_eq!(answer, json!({ "pulled": null }));
}

#[tokio::test]
async fn remove_deletes_the_conversation_worktree() {
    // Given
    let daemon = a_daemon_with_a_git_session();
    daemon
        .tool_call(
            "explore",
            "Write",
            json!({ "path": "a.txt", "contents": "a\n" }),
        )
        .await;

    // When
    let answer = daemon
        .conversation_worktree("explore", Op::Remove(RemoveOp {}))
        .await;

    // Then
    assert_eq!(
        (answer, daemon.conversation_root("explore").exists()),
        (json!({ "removed": true }), false)
    );
}

#[tokio::test]
async fn a_conversation_worktree_call_with_an_unverifiable_token_is_refused() {
    // Given
    let daemon = a_daemon_with_a_git_session();

    // When
    let refused = daemon
        .service
        .conversation_worktree(Request::direct(a_conversation_worktree_request(
            "not-a-session-token",
            "explore",
            Op::Remove(RemoveOp {}),
        )))
        .await
        .expect_err("an unverifiable token is refused");

    // Then
    assert_eq!(refused.code(), Code::Unauthenticated);
}

fn short_head(root: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .expect("spawn git");
    String::from_utf8(out.stdout)
        .expect("utf-8")
        .trim()
        .to_string()
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
