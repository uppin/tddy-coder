//! `ListSessions` reports what each agent session is doing, inferred from that session's own
//! conversation.
//!
//! PRD: docs/ft/daemon/agent-session-status.md § Acceptance Criteria. These run the real handler
//! over on-disk session fixtures, because what is under test is a correlation between two durable
//! stores and the listing that has to speak for them.

use tddy_core::agent_activity::{
    append_agent_activity, AgentActivityRecord, STATUS_COMPLETED, STATUS_RUNNING,
};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_daemon::test_util::{test_service, TEST_TOKEN};
use tddy_rpc::Request;
use tddy_service::acp_replay::{append_acp_frame, tool_use_frame};
use tddy_service::proto::acp::ToolCallStatus;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSessionsRequest, SessionAgentStatus,
    SessionEntry as ProtoSessionEntry,
};
use tddy_testing_commons::{a_session_metadata, fs::write_session_yaml};

/// A stamp far enough in the past that it can never be confused with "now".
const RECORDED_AT: i64 = 1_780_828_020_298;

// ─── fixtures ───────────────────────────────────────────────────────────────────────────────────

/// A session on disk, of a kind and with a hook word chosen by the test.
struct SessionFixture {
    session_dir: std::path::PathBuf,
    session_id: String,
    session_type: String,
    hook_word: Option<String>,
}

/// A claude-cli session — the kind whose worktree the daemon wires hooks into.
fn a_claude_cli_session(sessions_base: &std::path::Path, session_id: &str) -> SessionFixture {
    a_session_of_kind(sessions_base, session_id, "claude-cli")
}

/// A cursor session — an agent session whose worktree has no hooks wired.
fn a_cursor_session(sessions_base: &std::path::Path, session_id: &str) -> SessionFixture {
    a_session_of_kind(sessions_base, session_id, "cursor-cli")
}

/// A workspace session — a checkout holder that runs no agent at all.
fn a_workspace_session(sessions_base: &std::path::Path, session_id: &str) -> SessionFixture {
    a_session_of_kind(sessions_base, session_id, "workspace")
}

fn a_session_of_kind(
    sessions_base: &std::path::Path,
    session_id: &str,
    session_type: &str,
) -> SessionFixture {
    SessionFixture {
        session_dir: sessions_base.join(SESSIONS_SUBDIR).join(session_id),
        session_id: session_id.to_string(),
        session_type: session_type.to_string(),
        hook_word: None,
    }
}

impl SessionFixture {
    /// The last status this session's worktree reported through `ReportSessionStatus`.
    fn whose_hooks_said(mut self, hook_word: &str) -> Self {
        self.hook_word = Some(hook_word.to_string());
        self
    }

    /// Write the session to disk and return its directory, for the tests that record a
    /// conversation into it.
    fn build(self) -> std::path::PathBuf {
        std::fs::create_dir_all(&self.session_dir).expect("create session dir");
        let mut metadata = a_session_metadata()
            .with_session_id(&self.session_id)
            .with_session_type(&self.session_type)
            .with_repo_path("/tmp/repo")
            .with_pid(0);
        if let Some(hook_word) = &self.hook_word {
            metadata = metadata.with_activity_status(hook_word);
        }
        write_session_yaml(&self.session_dir, &metadata.build());
        self.session_dir
    }
}

/// One `Read` of `src/main.rs`, as the durable agent-activity log records it.
fn a_read_of_main(call_id: &str, status: &str) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: call_id.to_string(),
        tool_name: "Read".to_string(),
        input: serde_json::json!({ "file_path": "/repo/src/main.rs" }),
        status: status.to_string(),
        result: serde_json::Value::Null,
        error_message: String::new(),
        started_unix_ms: RECORDED_AT as u64,
        completed_unix_ms: 0,
        source: "claude-cli".to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    }
}

/// Every session `ListSessions` returns, by id.
async fn listed_sessions(sessions_base: std::path::PathBuf) -> Vec<ProtoSessionEntry> {
    let service = test_service(sessions_base);
    service
        .list_sessions(Request::new(ListSessionsRequest {
            session_token: TEST_TOKEN.to_string(),
        }))
        .await
        .expect("ListSessions RPC should succeed")
        .into_inner()
        .sessions
}

/// The one listed session with this id (panics when the listing does not hold it).
fn the_session<'a>(sessions: &'a [ProtoSessionEntry], session_id: &str) -> &'a ProtoSessionEntry {
    sessions
        .iter()
        .find(|s| s.session_id == session_id)
        .unwrap_or_else(|| panic!("'{session_id}' was not listed"))
}

// ─── assertions ─────────────────────────────────────────────────────────────────────────────────

trait AgentStatusAssertions {
    fn assert_agent_status(&self, expected: SessionAgentStatus) -> &Self;
    fn assert_last_activity(&self, summary: &str, at_unix_ms: u64) -> &Self;
    fn assert_no_last_activity(&self) -> &Self;
}

impl AgentStatusAssertions for ProtoSessionEntry {
    fn assert_agent_status(&self, expected: SessionAgentStatus) -> &Self {
        assert_eq!(
            SessionAgentStatus::try_from(self.agent_status).expect("a known status"),
            expected,
            "agent_status of '{}'",
            self.session_id
        );
        self
    }

    fn assert_last_activity(&self, summary: &str, at_unix_ms: u64) -> &Self {
        let activity = self
            .last_activity
            .as_ref()
            .unwrap_or_else(|| panic!("'{}' carried no last_activity", self.session_id));
        assert_eq!(
            activity.summary, summary,
            "summary of '{}'",
            self.session_id
        );
        assert_eq!(
            activity.at_unix_ms, at_unix_ms,
            "last_activity stamp of '{}' must be the signal's own, not the listing's",
            self.session_id
        );
        self
    }

    fn assert_no_last_activity(&self) -> &Self {
        assert_eq!(
            self.last_activity, None,
            "'{}' must carry no last_activity",
            self.session_id
        );
        self
    }
}

// ─── the criteria ───────────────────────────────────────────────────────────────────────────────

/// Criteria 1 and 2.
#[tokio::test]
async fn a_claude_cli_session_mid_read_is_executing_tool_naming_the_file_it_is_reading() {
    // Given a claude-cli session whose newest activity row is a Read that has not returned
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = a_claude_cli_session(&sessions_base, "peer-reading-1").build();
    append_agent_activity(&session_dir, &a_read_of_main("call-1", STATUS_RUNNING)).unwrap();

    // When
    let sessions = listed_sessions(sessions_base).await;

    // Then
    the_session(&sessions, "peer-reading-1")
        .assert_agent_status(SessionAgentStatus::ExecutingTool)
        .assert_last_activity("Read main.rs", RECORDED_AT as u64);
}

/// Criterion 3.
#[tokio::test]
async fn a_session_whose_turn_has_finished_is_idle_and_still_shows_the_call_it_finished() {
    // Given a completed Read and hooks that saw the turn stop
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = a_claude_cli_session(&sessions_base, "peer-finished-1")
        .whose_hooks_said("Done")
        .build();
    let mut finished = a_read_of_main("call-1", STATUS_COMPLETED);
    finished.completed_unix_ms = RECORDED_AT as u64 + 1_000;
    finished.result = serde_json::json!("fn main() {}");
    append_agent_activity(&session_dir, &finished).unwrap();

    // When
    let sessions = listed_sessions(sessions_base).await;

    // Then — what an idle agent was last seen doing is the useful thing on its row
    the_session(&sessions, "peer-finished-1")
        .assert_agent_status(SessionAgentStatus::Idle)
        .assert_last_activity("Read main.rs", RECORDED_AT as u64 + 1_000);
}

/// Criterion 4.
#[tokio::test]
async fn a_stopped_session_is_idle_even_with_a_tool_call_left_in_flight() {
    // Given a `running` row whose terminal record never arrived, and hooks that saw the turn stop
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = a_claude_cli_session(&sessions_base, "peer-abandoned-1")
        .whose_hooks_said("Done")
        .build();
    append_agent_activity(&session_dir, &a_read_of_main("call-1", STATUS_RUNNING)).unwrap();

    // When
    let sessions = listed_sessions(sessions_base).await;

    // Then — otherwise the badge stays at EXECUTING_TOOL for the rest of the session's life
    the_session(&sessions, "peer-abandoned-1").assert_agent_status(SessionAgentStatus::Idle);
}

/// Criterion 5.
#[tokio::test]
async fn a_cursor_session_with_no_hooks_wired_infers_its_status_from_its_acp_transcript_alone() {
    // Given a cursor session that writes only its own ACP transcript
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = a_cursor_session(&sessions_base, "peer-cursor-1").build();
    append_acp_frame(
        &session_dir,
        &tool_use_frame(
            3,
            "Bash",
            &serde_json::json!({ "command": "cargo test", "description": "run the test suite" }),
            ToolCallStatus::InProgress,
            RECORDED_AT,
        ),
    )
    .unwrap();

    // When
    let sessions = listed_sessions(sessions_base).await;

    // Then
    the_session(&sessions, "peer-cursor-1")
        .assert_agent_status(SessionAgentStatus::ExecutingTool)
        .assert_last_activity("Bash run the test suite", RECORDED_AT as u64);
}

/// Criterion 6.
#[tokio::test]
async fn a_session_that_has_written_no_conversation_and_reported_no_hook_word_is_unspecified() {
    // Given a claude-cli session that has done nothing yet
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    a_claude_cli_session(&sessions_base, "peer-silent-1").build();

    // When
    let sessions = listed_sessions(sessions_base).await;

    // Then — "attached and ready" is a claim, and this daemon has no grounds for it
    the_session(&sessions, "peer-silent-1")
        .assert_agent_status(SessionAgentStatus::Unspecified)
        .assert_no_last_activity();
}

/// Criterion 7.
#[tokio::test]
async fn a_workspace_session_is_unspecified_even_with_a_transcript_on_disk() {
    // Given a workspace session — a checkout holder that runs no agent
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = a_workspace_session(&sessions_base, "clone-holder-1").build();
    append_agent_activity(&session_dir, &a_read_of_main("call-1", STATUS_RUNNING)).unwrap();

    // When
    let sessions = listed_sessions(sessions_base).await;

    // Then
    the_session(&sessions, "clone-holder-1")
        .assert_agent_status(SessionAgentStatus::Unspecified)
        .assert_no_last_activity();
}

/// Criterion 8.
#[tokio::test]
async fn one_sessions_tool_call_never_appears_on_another_sessions_row() {
    // Given two peer sessions, only one of which is inside a call
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let busy_dir = a_claude_cli_session(&sessions_base, "peer-busy-1").build();
    append_agent_activity(&busy_dir, &a_read_of_main("call-1", STATUS_RUNNING)).unwrap();
    a_claude_cli_session(&sessions_base, "peer-idle-1")
        .whose_hooks_said("Done")
        .build();

    // When
    let sessions = listed_sessions(sessions_base).await;

    // Then
    the_session(&sessions, "peer-busy-1")
        .assert_agent_status(SessionAgentStatus::ExecutingTool)
        .assert_last_activity("Read main.rs", RECORDED_AT as u64);
    the_session(&sessions, "peer-idle-1")
        .assert_agent_status(SessionAgentStatus::Idle)
        .assert_no_last_activity();
}

/// Criterion 10.
#[tokio::test]
async fn an_oversized_multi_line_summary_reaches_the_wire_cut_to_one_line_of_120_characters() {
    // Given an agent that ran a shell command with a paragraph for a description
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = a_claude_cli_session(&sessions_base, "peer-verbose-1").build();
    let paragraph = format!("first line\n{}", "x".repeat(200));
    let mut verbose = a_read_of_main("call-1", STATUS_RUNNING);
    verbose.tool_name = "Bash".to_string();
    verbose.input = serde_json::json!({ "command": "true", "description": paragraph });
    append_agent_activity(&session_dir, &verbose).unwrap();

    // When
    let sessions = listed_sessions(sessions_base).await;

    // Then — an oversized snapshot is chunk-framed, where one lost frame wedges the call silently
    let listed = the_session(&sessions, "peer-verbose-1");
    let activity = listed
        .last_activity
        .as_ref()
        .expect("peer-verbose-1 carried no last_activity");
    assert_eq!(activity.summary.chars().count(), 120);
    assert!(
        activity.summary.starts_with("Bash first line x"),
        "the newline must collapse to a space, was {:?}",
        activity.summary
    );
}

/// The hook word alone is a state, and nothing to display.
#[tokio::test]
async fn a_session_whose_hooks_are_running_but_that_has_written_nothing_reports_running_with_no_activity(
) {
    // Given hooks wired on a session that has recorded no call yet
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    a_claude_cli_session(&sessions_base, "peer-starting-1")
        .whose_hooks_said("Running")
        .build();

    // When
    let sessions = listed_sessions(sessions_base).await;

    // Then — a bare timestamp reads as an agent that did something unnameable just now
    the_session(&sessions, "peer-starting-1")
        .assert_agent_status(SessionAgentStatus::Running)
        .assert_no_last_activity();
}

/// The raw hook word keeps its own field — the inference is built on it, not a replacement for it.
#[tokio::test]
async fn the_inferred_status_does_not_disturb_the_hook_word_the_entry_already_carried() {
    // Given a session mid-call whose hooks last reported ExecutingTool
    let temp = tempfile::tempdir().unwrap();
    let sessions_base = temp.path().to_path_buf();
    let session_dir = a_claude_cli_session(&sessions_base, "peer-both-1")
        .whose_hooks_said("ExecutingTool")
        .build();
    append_agent_activity(&session_dir, &a_read_of_main("call-1", STATUS_RUNNING)).unwrap();

    // When
    let sessions = listed_sessions(sessions_base).await;

    // Then both fields are populated, from their own sources
    let listed = the_session(&sessions, "peer-both-1");
    assert_eq!(listed.activity_status, "ExecutingTool");
    listed
        .assert_agent_status(SessionAgentStatus::ExecutingTool)
        .assert_last_activity("Read main.rs", RECORDED_AT as u64);
}
