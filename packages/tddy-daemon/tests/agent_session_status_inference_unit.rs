//! What a claude-cli / cursor session is doing, inferred from its own conversation.
//!
//! PRD: docs/ft/daemon/agent-session-status.md. These cover the three pieces the inference is made
//! of — the frame/record mapper, the rules that turn a signal plus the hook word into a status, and
//! the per-session store that keeps the newest signal current.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tddy_core::agent_activity::{
    append_agent_activity, AgentActivityRecord, STATUS_COMPLETED, STATUS_RUNNING,
};
use tddy_core::session_activity::SessionActivityStatus;
use tddy_daemon::connection_service::AgentActivityHub;
use tddy_daemon::session_agent_inference::{
    activity_from_frame, activity_from_record, inferred_activity, session_agent_status,
    SessionAgentInferenceStore,
};
use tddy_daemon::session_agent_status::{AgentActivity, ManagedAgentState};
use tddy_service::acp_replay::{agent_text_frame, append_acp_frame, tool_use_frame};
use tddy_service::proto::acp::{AcpAgentMessage, ToolCallStatus};
use tddy_service::proto::connection::SessionAgentStatus;

// ─── builders ───────────────────────────────────────────────────────────────────────────────────

/// A stamp far enough in the past that it can never be confused with "now".
const RECORDED_AT: i64 = 1_780_828_020_298;

/// A `Read` of `src/main.rs`, as the transcript records one — the ACP frame an agent CLI writes.
fn a_read_frame(status: ToolCallStatus) -> AcpAgentMessage {
    tool_use_frame(
        7,
        "Read",
        &serde_json::json!({ "file_path": "/repo/src/main.rs" }),
        status,
        RECORDED_AT,
    )
}

/// The same `Read`, as the durable agent-activity log records one.
fn a_read_record(call_id: &str, status: &str) -> AgentActivityRecord {
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

/// One observed signal, as the store holds it.
fn an_observed(state: ManagedAgentState, summary: &str) -> AgentActivity {
    AgentActivity {
        state,
        summary: summary.to_string(),
        at_unix_ms: RECORDED_AT as u64,
    }
}

/// A session directory with nothing in it yet.
fn a_session_dir(root: &Path, session_id: &str) -> std::path::PathBuf {
    let dir = root.join(session_id);
    std::fs::create_dir_all(&dir).expect("create session dir");
    dir
}

// ─── the mapper: transcript frames ──────────────────────────────────────────────────────────────

#[test]
fn a_tool_call_still_in_progress_is_executing_tool_titled_by_the_call() {
    // Given a transcript frame for a Read that has not returned
    let frame = a_read_frame(ToolCallStatus::InProgress);

    // When
    let observed = activity_from_frame(&frame).expect("a tool_call frame is an observable signal");

    // Then
    assert_eq!(observed.state, ManagedAgentState::ExecutingTool);
    assert_eq!(observed.summary, "Read main.rs");
    assert_eq!(observed.at_unix_ms, RECORDED_AT as u64);
}

#[test]
fn a_pending_tool_call_is_executing_tool_because_the_turn_is_already_inside_it() {
    // Given
    let frame = a_read_frame(ToolCallStatus::Pending);

    // When
    let observed = activity_from_frame(&frame).expect("a tool_call frame is an observable signal");

    // Then
    assert_eq!(observed.state, ManagedAgentState::ExecutingTool);
}

#[test]
fn a_completed_tool_call_is_running_because_the_tool_returned_and_the_turn_did_not() {
    // Given
    let frame = a_read_frame(ToolCallStatus::Completed);

    // When
    let observed = activity_from_frame(&frame).expect("a tool_call frame is an observable signal");

    // Then — the call is over, the turn that made it is not
    assert_eq!(observed.state, ManagedAgentState::Prompting);
    assert_eq!(observed.summary, "Read main.rs");
}

#[test]
fn a_failed_tool_call_is_running_too_because_a_failed_tool_does_not_end_a_turn() {
    // Given
    let frame = a_read_frame(ToolCallStatus::Failed);

    // When
    let observed = activity_from_frame(&frame).expect("a tool_call frame is an observable signal");

    // Then
    assert_eq!(observed.state, ManagedAgentState::Prompting);
}

#[test]
fn agent_text_is_running_carrying_the_text_the_agent_wrote() {
    // Given
    let frame = agent_text_frame("reading the parser to find the entry point", RECORDED_AT);

    // When
    let observed = activity_from_frame(&frame).expect("agent text is an observable signal");

    // Then
    assert_eq!(observed.state, ManagedAgentState::Prompting);
    assert_eq!(
        observed.summary,
        "reading the parser to find the entry point"
    );
}

#[test]
fn a_frame_carrying_no_session_update_says_nothing_about_what_the_agent_is_doing() {
    // Given a frame shape the transcript never persists
    let frame = AcpAgentMessage { id: 0, msg: None };

    // When
    let observed = activity_from_frame(&frame);

    // Then
    assert_eq!(observed, None);
}

// ─── the mapper: activity records ───────────────────────────────────────────────────────────────

#[test]
fn a_running_activity_record_is_executing_tool_titled_exactly_as_its_replayed_frame() {
    // Given the two records of one Read — the live row and the frame a replay would build
    let record = a_read_record("call-1", STATUS_RUNNING);

    // When
    let observed = activity_from_record(&record);

    // Then — one mapper, so a live row and its replay cannot word the same call differently
    assert_eq!(observed.state, ManagedAgentState::ExecutingTool);
    assert_eq!(observed.summary, "Read main.rs");
    assert_eq!(observed.at_unix_ms, RECORDED_AT as u64);
}

#[test]
fn a_completed_activity_record_is_running_and_stamped_with_the_time_the_call_finished() {
    // Given a Read that finished a second after it started
    let mut record = a_read_record("call-1", STATUS_COMPLETED);
    record.completed_unix_ms = RECORDED_AT as u64 + 1_000;
    record.result = serde_json::json!("fn main() {}");

    // When
    let observed = activity_from_record(&record);

    // Then
    assert_eq!(observed.state, ManagedAgentState::Prompting);
    assert_eq!(observed.at_unix_ms, RECORDED_AT as u64 + 1_000);
}

// ─── the rules: a signal plus the hook word ─────────────────────────────────────────────────────

#[test]
fn a_session_with_nothing_observed_and_no_hook_word_reports_no_activity_at_all() {
    // Given a session this daemon has seen nothing from
    // When
    let inferred = inferred_activity(None, None);

    // Then — "attached and ready" is a claim, and there are no grounds for it
    assert_eq!(inferred, None);
    assert_eq!(
        session_agent_status(inferred.as_ref()),
        SessionAgentStatus::Unspecified
    );
}

#[test]
fn a_tool_call_in_flight_outranks_a_hook_word_of_running() {
    // Given hooks that say only "running" and a transcript that says which call
    let observed = an_observed(ManagedAgentState::ExecutingTool, "Bash cargo test");

    // When
    let inferred = inferred_activity(Some(SessionActivityStatus::Running), Some(&observed))
        .expect("an observed signal is an activity");

    // Then — the transcript is strictly more precise, and the only source of the call's name
    assert_eq!(inferred.state, ManagedAgentState::ExecutingTool);
    assert_eq!(inferred.summary, "Bash cargo test");
    assert_eq!(
        session_agent_status(Some(&inferred)),
        SessionAgentStatus::ExecutingTool
    );
}

#[test]
fn a_stopped_session_is_idle_even_with_a_tool_call_left_in_flight() {
    // Given a `running` row whose terminal record never arrived, and hooks that saw the turn stop
    let observed = an_observed(ManagedAgentState::ExecutingTool, "Bash cargo test");

    // When
    let inferred = inferred_activity(Some(SessionActivityStatus::Done), Some(&observed))
        .expect("an observed signal is an activity");

    // Then — otherwise the badge stays at EXECUTING_TOOL for ever
    assert_eq!(
        session_agent_status(Some(&inferred)),
        SessionAgentStatus::Idle
    );
}

#[test]
fn an_idle_session_still_shows_the_call_it_was_last_seen_making() {
    // Given
    let observed = an_observed(ManagedAgentState::ExecutingTool, "Bash cargo test");

    // When
    let inferred = inferred_activity(Some(SessionActivityStatus::Done), Some(&observed))
        .expect("an observed signal is an activity");

    // Then — what an idle agent was last doing is the useful thing on its row
    assert_eq!(inferred.summary, "Bash cargo test");
    assert_eq!(inferred.at_unix_ms, RECORDED_AT as u64);
}

#[test]
fn an_ended_session_is_idle_because_its_agent_is_no_longer_in_a_turn() {
    // Given
    let observed = an_observed(ManagedAgentState::Prompting, "Read main.rs");

    // When
    let inferred = inferred_activity(Some(SessionActivityStatus::Ended), Some(&observed))
        .expect("an observed signal is an activity");

    // Then
    assert_eq!(
        session_agent_status(Some(&inferred)),
        SessionAgentStatus::Idle
    );
}

#[test]
fn a_hook_word_of_waiting_for_input_decides_when_no_call_is_in_flight() {
    // Given a permission prompt on screen after the last call returned
    let observed = an_observed(ManagedAgentState::Prompting, "Read main.rs");

    // When
    let inferred = inferred_activity(
        Some(SessionActivityStatus::WaitingForInput),
        Some(&observed),
    )
    .expect("an observed signal is an activity");

    // Then
    assert_eq!(
        session_agent_status(Some(&inferred)),
        SessionAgentStatus::WaitingForInput
    );
    assert_eq!(inferred.summary, "Read main.rs");
}

#[test]
fn a_hook_word_with_no_frames_reports_its_state_and_nothing_to_display() {
    // Given hooks wired on a session that has written no transcript yet
    // When
    let inferred = inferred_activity(Some(SessionActivityStatus::Running), None)
        .expect("a hook word is a state even with nothing to show");

    // Then — a bare timestamp reads as an agent that did something unnameable just now
    assert_eq!(
        session_agent_status(Some(&inferred)),
        SessionAgentStatus::Running
    );
    assert_eq!(inferred.summary, "");
    assert_eq!(inferred.to_proto(), None);
}

#[test]
fn a_cursor_session_with_no_hook_word_takes_its_state_from_the_transcript_alone() {
    // Given a session whose worktree has no hooks wired
    let observed = an_observed(ManagedAgentState::ExecutingTool, "Read main.rs");

    // When
    let inferred = inferred_activity(None, Some(&observed)).expect("the signal is the activity");

    // Then
    assert_eq!(
        session_agent_status(Some(&inferred)),
        SessionAgentStatus::ExecutingTool
    );
    assert_eq!(inferred.summary, "Read main.rs");
}

// ─── the store ──────────────────────────────────────────────────────────────────────────────────

#[test]
fn the_store_seeds_the_newest_transcript_frame_when_nothing_has_been_observed() {
    // Given a session whose durable transcript ends on a Read that has not returned
    let temp = tempfile::tempdir().unwrap();
    let session_dir = a_session_dir(temp.path(), "seeded-1");
    append_acp_frame(&session_dir, &agent_text_frame("looking", RECORDED_AT - 10)).unwrap();
    append_acp_frame(&session_dir, &a_read_frame(ToolCallStatus::InProgress)).unwrap();
    let store = SessionAgentInferenceStore::new();

    // When
    store
        .seed_from_transcript("seeded-1", &session_dir)
        .expect("reading a transcript that exists must not fail");

    // Then — the newest frame, not the first
    let latest = store
        .latest("seeded-1")
        .expect("the seed is an observation");
    assert_eq!(latest.state, ManagedAgentState::ExecutingTool);
    assert_eq!(latest.summary, "Read main.rs");
}

#[test]
fn the_store_seeds_from_the_durable_activity_log_when_there_is_no_acp_transcript() {
    // Given a daemon-hosted session, which writes only agent-activity.jsonl
    let temp = tempfile::tempdir().unwrap();
    let session_dir = a_session_dir(temp.path(), "activity-only-1");
    append_agent_activity(&session_dir, &a_read_record("call-1", STATUS_RUNNING)).unwrap();
    let store = SessionAgentInferenceStore::new();

    // When
    store
        .seed_from_transcript("activity-only-1", &session_dir)
        .expect("reading a transcript that exists must not fail");

    // Then
    let latest = store
        .latest("activity-only-1")
        .expect("the seed is an observation");
    assert_eq!(latest.state, ManagedAgentState::ExecutingTool);
    assert_eq!(latest.summary, "Read main.rs");
}

#[test]
fn a_live_observation_outranks_what_the_transcript_had_on_disk() {
    // Given a record that landed while the file was being read
    let temp = tempfile::tempdir().unwrap();
    let session_dir = a_session_dir(temp.path(), "raced-1");
    append_acp_frame(&session_dir, &a_read_frame(ToolCallStatus::InProgress)).unwrap();
    let store = SessionAgentInferenceStore::new();
    store.observe("raced-1", an_observed(ManagedAgentState::Prompting, "done"));

    // When the seed runs afterwards
    store
        .seed_from_transcript("raced-1", &session_dir)
        .expect("reading a transcript that exists must not fail");

    // Then the seed does not overwrite a newer live observation with older disk state
    let latest = store.latest("raced-1").expect("the observation is held");
    assert_eq!(latest.summary, "done");
}

#[test]
fn seeding_a_session_that_has_written_nothing_leaves_it_unobserved() {
    // Given a session directory with neither store on disk
    let temp = tempfile::tempdir().unwrap();
    let session_dir = a_session_dir(temp.path(), "empty-1");
    let store = SessionAgentInferenceStore::new();

    // When
    store
        .seed_from_transcript("empty-1", &session_dir)
        .expect("an absent transcript is not an error");

    // Then
    assert_eq!(store.latest("empty-1"), None);
}

#[test]
fn the_store_answers_per_session_so_one_sessions_call_is_never_anothers() {
    // Given two sessions observed doing different things
    let store = SessionAgentInferenceStore::new();
    store.observe(
        "sess-a",
        an_observed(ManagedAgentState::ExecutingTool, "Bash cargo test"),
    );
    store.observe(
        "sess-b",
        an_observed(ManagedAgentState::Prompting, "Read main.rs"),
    );

    // When / Then
    assert_eq!(
        store.latest("sess-a").expect("sess-a is observed").summary,
        "Bash cargo test"
    );
    assert_eq!(
        store.latest("sess-b").expect("sess-b is observed").summary,
        "Read main.rs"
    );
}

#[test]
fn forgetting_a_session_drops_what_it_was_last_seen_doing() {
    // Given
    let store = SessionAgentInferenceStore::new();
    store.observe(
        "deleted-1",
        an_observed(ManagedAgentState::ExecutingTool, "Bash cargo test"),
    );

    // When
    store.forget("deleted-1");

    // Then
    assert_eq!(store.latest("deleted-1"), None);
}

#[test]
fn an_oversized_multi_line_summary_is_cut_to_one_line_before_it_is_stored() {
    // Given an agent that wrote a paragraph — a whole-roster broadcast cannot carry one
    let store = SessionAgentInferenceStore::new();
    let paragraph = format!("first line\n{}", "x".repeat(200));
    store.observe(
        "verbose-1",
        an_observed(ManagedAgentState::Prompting, &paragraph),
    );

    // When
    let latest = store.latest("verbose-1").expect("the observation is held");

    // Then — 120 characters, collapsed to one line
    assert_eq!(latest.summary.chars().count(), 120);
    assert!(
        latest.summary.starts_with("first line x"),
        "the newline must collapse to a space, was {:?}",
        latest.summary
    );
}

// ─── the live tail ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_record_published_after_tailing_starts_becomes_the_sessions_latest_activity() {
    // Given a session being tailed, with nothing on disk
    let temp = tempfile::tempdir().unwrap();
    let session_dir = a_session_dir(temp.path(), "tailed-1");
    let hub = Arc::new(AgentActivityHub::default());
    let store = Arc::new(SessionAgentInferenceStore::new());
    store.ensure_tailing(&hub, "tailed-1", &session_dir);

    // When the daemon records a call for it
    hub.publish("tailed-1", a_read_record("call-1", STATUS_RUNNING));

    // Then — bounded wait on the consumer task, never a fixed sleep
    let latest = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(latest) = store.latest("tailed-1") {
                return latest;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the published record never reached the inference store");
    assert_eq!(latest.state, ManagedAgentState::ExecutingTool);
    assert_eq!(latest.summary, "Read main.rs");
}

#[tokio::test]
async fn tailing_a_session_twice_does_not_replace_what_the_first_tail_observed() {
    // Given a session already tailed and already observed
    let temp = tempfile::tempdir().unwrap();
    let session_dir = a_session_dir(temp.path(), "twice-1");
    append_acp_frame(&session_dir, &a_read_frame(ToolCallStatus::InProgress)).unwrap();
    let hub = Arc::new(AgentActivityHub::default());
    let store = Arc::new(SessionAgentInferenceStore::new());
    store.ensure_tailing(&hub, "twice-1", &session_dir);
    store.observe(
        "twice-1",
        an_observed(ManagedAgentState::Prompting, "newer"),
    );

    // When a second listing tails it again
    store.ensure_tailing(&hub, "twice-1", &session_dir);

    // Then the second call re-reads nothing — every listing would otherwise cost a file read
    assert_eq!(
        store
            .latest("twice-1")
            .expect("the observation is held")
            .summary,
        "newer"
    );
}
