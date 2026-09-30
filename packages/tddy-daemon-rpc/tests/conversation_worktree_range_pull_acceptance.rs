//! Acceptance: `ConversationWorktree { pull_range }` hands exactly a range of a conversation's
//! commits to the session worktree, skipping what the caller already took.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-range-pull.md

use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use serde_json::json;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_rpc::test_util::test_service;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{
    conversation_worktree_request::Op, ConversationWorktreeRequest, ExecToolService,
    ExecuteToolRequest, PullRangeOp,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::a_session_metadata;

const SESSION_ID: &str = "conversation-worktree-range-pull-session";

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

impl ADaemonWithASession {
    /// Write `path` through the conversation and return the commit that recorded it.
    async fn commit_through_the_conversation(&self, path: &str) -> String {
        self.tool_call(
            "explore",
            "Write",
            json!({ "path": path, "contents": format!("{path}\n") }),
        )
        .await["worktreeChange"]["commit"]
            .as_str()
            .expect("the write committed")
            .to_string()
    }
}

#[tokio::test]
async fn pull_range_hands_exactly_the_range_to_the_session_worktree() {
    // Given
    let daemon = a_daemon_with_a_git_session();
    daemon.commit_through_the_conversation("a.txt").await;
    let second = daemon.commit_through_the_conversation("b.txt").await;

    // When
    let answer = daemon
        .conversation_worktree(
            "explore",
            Op::PullRange(PullRangeOp {
                from: second.clone(),
                to: second.clone(),
                already_pulled: vec![],
            }),
        )
        .await;

    // Then
    assert_eq!(
        (
            answer["pulled"]["commits"].clone(),
            daemon.worktree.join("a.txt").exists(),
            daemon.worktree.join("b.txt").exists(),
        ),
        (json!([second]), false, true)
    );
}

#[tokio::test]
async fn pull_range_skips_what_the_caller_already_pulled() {
    // Given
    let daemon = a_daemon_with_a_git_session();
    let first = daemon.commit_through_the_conversation("a.txt").await;
    let second = daemon.commit_through_the_conversation("b.txt").await;

    // When
    let answer = daemon
        .conversation_worktree(
            "explore",
            Op::PullRange(PullRangeOp {
                from: String::new(),
                to: String::new(),
                already_pulled: vec![first.clone()],
            }),
        )
        .await;

    // Then
    assert_eq!(
        (
            answer["pulled"]["commits"].clone(),
            daemon.worktree.join("a.txt").exists(),
        ),
        (json!([second]), false)
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
