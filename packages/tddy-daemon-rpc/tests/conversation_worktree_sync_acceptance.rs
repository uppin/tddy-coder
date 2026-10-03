//! Acceptance: `ConversationWorktree { sync }` merges the session worktree's current files into a
//! conversation's worktree before a turn.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-03-agent-worktree-caller-sync.md

use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use serde_json::json;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_rpc::test_util::test_service;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{
    conversation_worktree_request::Op, ConversationWorktreeRequest, DiffOp, ExecToolService,
    ExecuteToolRequest, SyncOp,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_testing_commons::a_session_metadata;

const SESSION_ID: &str = "conversation-worktree-sync-session";

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

    /// The conversation's first tool call writes `path`, which creates its worktree and commits
    /// there; answers the commit the call reported.
    async fn a_conversation_that_wrote(
        &self,
        conversation_id: &str,
        path: &str,
        contents: &str,
    ) -> String {
        let answer = self
            .tool_call(
                conversation_id,
                "Write",
                json!({ "path": path, "contents": contents }),
            )
            .await;
        let commit = answer["worktreeChange"]["commit"]
            .as_str()
            .unwrap_or_else(|| panic!("the Write committed in the conversation: {answer}"))
            .to_string();
        assert_eq!(commit, short_head(&self.conversation_root(conversation_id)));
        commit
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
async fn sync_merges_the_session_worktrees_edit_into_the_conversation() {
    // Given
    let daemon = a_daemon_with_a_git_session();
    daemon
        .a_conversation_that_wrote("explore", "a.txt", "a\n")
        .await;
    std::fs::write(daemon.worktree.join("caller.txt"), "theirs\n").unwrap();

    // When
    let answer = daemon
        .conversation_worktree("explore", Op::Sync(SyncOp {}))
        .await;

    // Then
    assert_eq!(
        (
            answer["sync"]["paths"].clone(),
            answer["sync"]["commit"].clone(),
            std::fs::read_to_string(daemon.conversation_root("explore").join("caller.txt"))
                .expect("merged into the conversation worktree"),
        ),
        (
            json!(["caller.txt"]),
            json!(short_head(&daemon.conversation_root("explore"))),
            "theirs\n".to_string(),
        )
    );
}

#[tokio::test]
async fn sync_with_an_unchanged_session_worktree_merges_nothing() {
    // Given — the conversation has a worktree, and the session worktree has not changed since
    let daemon = a_daemon_with_a_git_session();
    let tip = daemon
        .a_conversation_that_wrote("explore", "a.txt", "a\n")
        .await;

    // When
    let answer = daemon
        .conversation_worktree("explore", Op::Sync(SyncOp {}))
        .await;

    // Then
    assert_eq!(
        (
            answer,
            daemon.conversation_root("explore").exists(),
            short_head(&daemon.conversation_root("explore"))
        ),
        (json!({ "sync": null }), true, tip)
    );
}

#[tokio::test]
async fn sync_on_a_conversation_without_a_worktree_merges_nothing() {
    // Given
    let daemon = a_daemon_with_a_git_session();
    std::fs::write(daemon.worktree.join("caller.txt"), "theirs\n").unwrap();

    // When
    let answer = daemon
        .conversation_worktree("explore", Op::Sync(SyncOp {}))
        .await;

    // Then
    assert_eq!(
        (answer, daemon.conversation_root("explore").exists()),
        (json!({ "sync": null }), false)
    );
}

#[tokio::test]
async fn sync_names_the_conflicting_paths_and_merges_nothing() {
    // Given — the subagent and the caller both rewrote README.md
    let daemon = a_daemon_with_a_git_session();
    let tip = daemon
        .a_conversation_that_wrote("explore", "README.md", "subagent\n")
        .await;
    std::fs::write(daemon.worktree.join("README.md"), "caller\n").unwrap();

    // When
    let answer = daemon
        .conversation_worktree("explore", Op::Sync(SyncOp {}))
        .await;

    // Then
    assert_eq!(
        (answer, short_head(&daemon.conversation_root("explore"))),
        (
            json!({ "conflicts": ["README.md"], "moreConflicts": 0 }),
            tip
        )
    );
}

#[tokio::test]
async fn a_diff_to_the_tip_right_after_a_sync_includes_the_callers_changes() {
    // Given — the tip is the sync's merge: no subagent commit follows it
    let daemon = a_daemon_with_a_git_session();
    daemon
        .a_conversation_that_wrote("explore", "a.txt", "a\n")
        .await;
    std::fs::write(daemon.worktree.join("caller.txt"), "theirs\n").unwrap();
    daemon
        .conversation_worktree("explore", Op::Sync(SyncOp {}))
        .await;

    // When
    let answer = daemon
        .conversation_worktree("explore", Op::Diff(DiffOp::default()))
        .await;

    // Then
    assert_eq!(
        (
            answer["diff"]["to"].clone(),
            answer["diff"]["includesCallerChanges"].clone()
        ),
        (
            json!(short_head(&daemon.conversation_root("explore"))),
            json!(true)
        )
    );
}

fn short_head(root: &Path) -> String {
    git(root, &["rev-parse", "--short", "HEAD"])
        .trim()
        .to_string()
}

fn git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8(out.stdout).expect("utf-8 git output")
}
