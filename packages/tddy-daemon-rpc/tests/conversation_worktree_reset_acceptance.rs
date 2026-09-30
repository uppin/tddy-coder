//! Acceptance: `ConversationWorktree { reset }` takes a conversation's worktree back to a commit.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-rewind-reset.md

use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use serde_json::json;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_rpc::test_util::test_service;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{
    conversation_worktree_request::Op, ConversationWorktreeRequest, ExecToolService,
    ExecuteToolRequest, ResetOp,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::a_session_metadata;

const SESSION_ID: &str = "conversation-worktree-reset-session";

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
async fn reset_moves_the_conversation_worktree_back_to_the_commit() {
    // Given
    let daemon = a_daemon_with_a_git_session();
    let first = daemon
        .tool_call(
            "explore",
            "Write",
            json!({ "path": "a.txt", "contents": "a\n" }),
        )
        .await["worktreeChange"]["commit"]
        .as_str()
        .expect("the first write committed")
        .to_string();
    let second = daemon
        .tool_call(
            "explore",
            "Write",
            json!({ "path": "b.txt", "contents": "b\n" }),
        )
        .await["worktreeChange"]["commit"]
        .as_str()
        .expect("the second write committed")
        .to_string();

    // When
    let answer = daemon
        .conversation_worktree(
            "explore",
            Op::Reset(ResetOp {
                commit: first.clone(),
            }),
        )
        .await;

    // Then
    assert_eq!(
        (
            answer,
            daemon.conversation_root("explore").join("b.txt").exists(),
            short_head(&daemon.conversation_root("explore")),
        ),
        (
            json!({ "reset": { "to": first.clone(), "droppedCommits": [second] } }),
            false,
            first,
        )
    );
}

#[tokio::test]
async fn reset_on_a_conversation_without_a_worktree_reports_no_worktree() {
    // Given
    let daemon = a_daemon_with_a_git_session();

    // When
    let answer = daemon
        .conversation_worktree(
            "explore",
            Op::Reset(ResetOp {
                commit: String::new(),
            }),
        )
        .await;

    // Then
    assert_eq!(
        (answer, daemon.conversation_root("explore").exists()),
        (json!({ "reset": null }), false)
    );
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
