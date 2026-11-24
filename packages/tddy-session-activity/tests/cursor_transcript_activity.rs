//! Acceptance: a Cursor session's transcript (`ReadSessionTranscript`) holds the user's prompt, the
//! agent's every reply — the text between its tool calls included — and its tool calls, in the
//! order Cursor's own `agent-transcripts` JSONL holds them.
//!
//! Cursor's hooks report its tool calls (with ids, status and results) and nothing it says; its
//! transcript holds everything it says and no tool ids. The spec (`coding_agent.activity` in
//! `cursor.yaml`) takes messages from the transcript and tool calls from the hooks, so the two
//! reports of one call — the hook's row and the transcript's `tool_use` — are paired, and the
//! transcript's text is placed around the calls the hooks recorded. Either source may come first:
//! a live follower reads a line as Cursor writes it, a follower that is behind (restarted, or
//! slow over a Coder target) reads it after every hook of the turn has fired.
//!
//! The path under test is the real hook mapping (`hook_request`, which the local
//! `tddy-tools session-hook` and the remote hook-log follower both use), the real activity handlers,
//! and the real spec's mapper over the probe's transcript (`tddy-coding-agent-specs`'
//! `tests/fixtures/cursor-agent-transcript.jsonl`), read back as `ReadSessionTranscript` reads it.
//! Local and target sessions share it: a target's transcript follower delivers to the same handler.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::{Request, Status};
use tddy_service::proto::acp::{acp_agent_message, content_block, session_update, AcpAgentMessage};
use tddy_service::proto::activity::ActivityService;
use tddy_session_activity::hook_request::{agent_activity_request, session_status_request};
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, NoSessionDeltaStores, SessionLabels, TranscriptLine,
};

const OS_USER: &str = "ada";
const SESSION_ID: &str = "01a20f1b-2e10-7c50-b055-b905d7b5debe";
const CHAT_ID: &str = "79fec325-2cbd-4223-b2a2-380b95d278c4";
const HOOK_TOKEN: &str = "tok-cursor-transcript";
const WORKTREE: &str = "/Users/dev/Code/tddy-coder/.worktrees/cursor-cli-01a20f1b";

const PROMPT: &str = "This is a hooks/activity probe; do not change any files. Step by step: 1) \
                      read README.md (first 20 lines), 2) run the shell command `git log \
                      --oneline -3`, 3) grep for \"CodingBackend\" in packages/, 4) then reply \
                      with a one-line summary and stop.";
const FIRST_REPLY: &str = "Running the probe steps without modifying any files.";
const LAST_REPLY_START: &str = "**Probe complete:**";

/// The ids Cursor's hooks gave the probe's three calls — with the newline Cursor embeds in them.
const READ_ID: &str = "call-64785683-composer_call_3AhseUhG\nfc_199e15db_0";
const SHELL_ID: &str = "93cc1692-cba9-4d5c-91b5-b66c774e7480";
const GREP_ID: &str = "call-64785683-composer_call_60DHMwTE\nfc_199e15db_2";

struct EverySessionIsLabelled;

impl SessionLabels for EverySessionIsLabelled {
    fn label_for(&self, _sessions_base: &Path, _session_id: &str) -> String {
        "probe".to_string()
    }
}

/// One daemon host holding one cursor-cli session. Its hub is the daemon's one: every handler
/// built on the host shares it, as `activity_service()` instances do.
struct ACursorHost {
    data_dir: tempfile::TempDir,
    hub: Arc<AgentActivityHub>,
}

impl ACursorHost {
    fn holding_a_cursor_session() -> Self {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        write_session(data_dir.path());
        Self {
            data_dir,
            hub: Arc::new(AgentActivityHub::default()),
        }
    }

    /// The host's activity handlers, built afresh as each of the daemon's calls builds them.
    fn service(&self) -> ActivityServiceImpl {
        ActivityServiceImpl::new(ActivityPorts {
            os_users: Arc::new(|_token: &str| {
                Err(Status::unauthenticated(
                    "no session token is presented on the hook path",
                ))
            }),
            tddy_data_dir: self.data_dir.path().to_path_buf(),
            activity: Arc::clone(&self.hub),
            conversation_frames: Arc::new(
                tddy_daemon_kernel::conversation_frames::ConversationFrames::default(),
            ),
            notifications: None,
            session_labels: Arc::new(EverySessionIsLabelled),
            deltas: Arc::new(NoSessionDeltaStores),
            daemon_instance_id: "host-a".to_string(),
            usage_bindings: Arc::new(|_spec: &str| None),
        })
        .with_agents(Arc::new(
            tddy_coding_agent_specs::CodingAgentRegistry::builtin()
                .expect("the built-in specs form a registry"),
        ))
    }

    fn session_dir(&self) -> PathBuf {
        self.data_dir.path().join("sessions").join(SESSION_ID)
    }

    /// Cursor fires the hook `event` with `payload`, and `tddy-tools session-hook` reports it.
    async fn hook_fires(&self, event: &str, payload: serde_json::Value) {
        let stdin = payload.to_string();
        if let Some(request) =
            session_status_request(SESSION_ID, HOOK_TOKEN, OS_USER, event, &stdin)
        {
            self.service()
                .report_session_status(Request::direct(request))
                .await
                .unwrap_or_else(|e| panic!("{event} is accepted: {e:?}"));
        }
        if let Some(request) =
            agent_activity_request(event, &stdin, SESSION_ID, HOOK_TOKEN, OS_USER)
        {
            self.service()
                .report_agent_activity(Request::direct(request))
                .await
                .unwrap_or_else(|e| panic!("{event} is accepted: {e:?}"));
        }
        // Hooks a millisecond apart, so their order on the transcript's clock is theirs.
        tokio::time::sleep(Duration::from_millis(3)).await;
    }

    async fn prompt_is_submitted(&self) {
        self.hook_fires(
            "beforeSubmitPrompt",
            serde_json::json!({"hook_event_name": "beforeSubmitPrompt", "prompt": PROMPT,
                               "conversation_id": CHAT_ID, "model": "composer-2.5"}),
        )
        .await;
    }

    async fn tool_starts(&self, tool: &str, id: &str, input: serde_json::Value) {
        self.hook_fires(
            "preToolUse",
            serde_json::json!({"hook_event_name": "preToolUse", "tool_name": tool,
                               "tool_use_id": id, "tool_input": input, "conversation_id": CHAT_ID}),
        )
        .await;
    }

    async fn tool_ends(&self, tool: &str, id: &str) {
        self.hook_fires(
            "postToolUse",
            serde_json::json!({"hook_event_name": "postToolUse", "tool_name": tool,
                               "tool_use_id": id, "tool_input": {},
                               "tool_output": "{\"success\":true}", "conversation_id": CHAT_ID}),
        )
        .await;
    }

    /// The probe's three parallel tool calls, as Cursor's hooks reported them.
    async fn the_three_tool_calls_run(&self) {
        self.tool_starts(
            "Read",
            READ_ID,
            serde_json::json!({"file_path": format!("{WORKTREE}/README.md")}),
        )
        .await;
        self.tool_starts(
            "Shell",
            SHELL_ID,
            serde_json::json!({"command": "git log --oneline -3"}),
        )
        .await;
        self.tool_starts(
            "Grep",
            GREP_ID,
            serde_json::json!({"pattern": "CodingBackend"}),
        )
        .await;
        self.tool_ends("Read", READ_ID).await;
        self.tool_ends("Shell", SHELL_ID).await;
        self.tool_ends("Grep", GREP_ID).await;
    }

    async fn turn_stops(&self) {
        self.hook_fires(
            "stop",
            serde_json::json!({"hook_event_name": "stop", "status": "completed",
                               "conversation_id": CHAT_ID}),
        )
        .await;
    }

    /// The transcript follower read line `n` (from 0) of the probe's transcript.
    async fn transcript_line_is_read(&self, n: usize) {
        let (line, end_offset) = the_probe_transcript_lines()
            .into_iter()
            .nth(n)
            .expect("the probe's transcript has the line");
        let entries = the_cursor_transcript_mapping().map_line(&line);
        self.service()
            .record_transcript_line(TranscriptLine {
                session_id: SESSION_ID.to_string(),
                os_user: OS_USER.to_string(),
                entries,
                end_offset,
            })
            .await
            .unwrap_or_else(|e| panic!("transcript line {n} is accepted: {e:?}"));
        tokio::time::sleep(Duration::from_millis(3)).await;
    }

    async fn whole_transcript_is_read(&self) {
        for n in 0..the_probe_transcript_lines().len() {
            self.transcript_line_is_read(n).await;
        }
    }

    /// What `ReadSessionTranscript` lists: each item's kind and text, in order.
    fn transcript(&self) -> Vec<(String, String)> {
        tddy_service::acp_replay::read_session_transcript(&self.session_dir())
            .expect("the transcript reads")
            .iter()
            .map(kind_and_text)
            .collect()
    }

    /// The call ids of the session's activity log, each call once, in order.
    fn logged_call_ids(&self) -> Vec<String> {
        tddy_core::agent_activity::read_agent_activity(&self.session_dir())
            .expect("the activity log reads")
            .into_iter()
            .map(|record| record.call_id)
            .collect()
    }
}

fn the_cursor_transcript_mapping() -> tddy_coding_agent_specs::TranscriptSource {
    tddy_coding_agent_specs::embedded_spec("cursor")
        .expect("cursor is built in")
        .activity()
        .transcript
        .clone()
        .expect("cursor's spec declares its transcript")
}

/// The probe's transcript lines with the byte each ends at.
fn the_probe_transcript_lines() -> Vec<(serde_json::Value, u64)> {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tddy-coding-agent-specs/tests/fixtures/cursor-agent-transcript.jsonl"),
    )
    .expect("the fixture");
    let mut end = 0u64;
    text.lines()
        .map(|line| {
            end += line.len() as u64 + 1;
            (serde_json::from_str(line).expect("a JSON line"), end)
        })
        .collect()
}

fn kind_and_text(frame: &AcpAgentMessage) -> (String, String) {
    let update = match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(n)) => {
            n.update.as_ref().and_then(|u| u.update.as_ref())
        }
        _ => None,
    };
    let text_of = |chunk: &tddy_service::proto::acp::ContentChunk| match chunk
        .content
        .as_ref()
        .and_then(|c| c.block.as_ref())
    {
        Some(content_block::Block::Text(t)) => t.text.clone(),
        _ => String::new(),
    };
    match update {
        Some(session_update::Update::UserMessageChunk(c)) => ("user_message".into(), text_of(c)),
        Some(session_update::Update::AgentMessageChunk(c)) => ("agent_message".into(), text_of(c)),
        Some(session_update::Update::ToolCall(tc)) => ("tool_call".into(), tc.title.clone()),
        _ => ("other".into(), String::new()),
    }
}

/// The transcript's kinds in order, each message's text shortened to what identifies it.
fn shape(transcript: &[(String, String)]) -> Vec<(String, String)> {
    transcript
        .iter()
        .map(|(kind, text)| {
            let text = if kind == "tool_call" {
                text.split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string()
            } else if text.starts_with(LAST_REPLY_START) {
                LAST_REPLY_START.to_string()
            } else {
                text.clone()
            };
            (kind.clone(), text)
        })
        .collect()
}

fn the_probes_order() -> Vec<(String, String)> {
    [
        ("user_message", PROMPT),
        ("agent_message", FIRST_REPLY),
        ("tool_call", "Read"),
        ("tool_call", "Shell"),
        ("tool_call", "Grep"),
        ("agent_message", LAST_REPLY_START),
    ]
    .into_iter()
    .map(|(kind, text)| (kind.to_string(), text.to_string()))
    .collect()
}

fn write_session(data_dir: &Path) {
    let session_dir = data_dir.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("the session dir");
    let meta = SessionMetadata {
        display_name: None,
        effort: None,
        session_id: SESSION_ID.to_string(),
        project_id: "a-project".to_string(),
        created_at: "2026-11-24T19:49:42Z".to_string(),
        updated_at: "2026-11-24T19:49:42Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(WORKTREE.to_string()),
        pid: Some(4242),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("cursor-cli".to_string()),
        model: Some("composer-2.5".to_string()),
        cursor_chat_id: Some(CHAT_ID.to_string()),
        harness_resume_token: None,
        activity_status: None,
        hook_token: Some(HOOK_TOKEN.to_string()),
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
        ssh_config_host: None,
        target: None,
        managed_codebase: false,
        session_role: None,
        tmux_session: None,
        orchestration_mark: None,
        agent_conversations: Vec::new(),
    };
    write_session_metadata(&session_dir, &meta).expect("the session's .session.yaml");
}

#[tokio::test]
async fn a_live_followers_transcript_is_the_prompt_the_replies_and_the_tool_calls_in_cursors_order()
{
    // Given a cursor-cli session whose transcript follower reads each line as Cursor writes it
    let host = ACursorHost::holding_a_cursor_session();

    // When the probe runs: the prompt, the reply that announces the calls, the calls, the summary
    host.prompt_is_submitted().await;
    host.transcript_line_is_read(0).await;
    host.transcript_line_is_read(1).await;
    host.the_three_tool_calls_run().await;
    host.transcript_line_is_read(2).await;
    host.turn_stops().await;
    host.transcript_line_is_read(3).await;

    // Then
    assert_eq!(shape(&host.transcript()), the_probes_order());
}

#[tokio::test]
async fn a_follower_that_is_behind_still_places_every_message_where_cursor_wrote_it() {
    // Given a cursor-cli session whose follower reads the transcript only after the whole turn —
    // a restarted follower, or a slow one on a Coder target
    let host = ACursorHost::holding_a_cursor_session();

    // When every hook of the turn fires before any transcript line is read
    host.prompt_is_submitted().await;
    host.the_three_tool_calls_run().await;
    host.turn_stops().await;
    host.whole_transcript_is_read().await;

    // Then the prompt and the first reply still come before the calls they preceded
    assert_eq!(shape(&host.transcript()), the_probes_order());
}

#[tokio::test]
async fn the_prompt_hook_adds_no_message_when_the_spec_reads_messages_from_the_transcript() {
    // Given a cursor-cli session, whose spec reads its messages from its transcript alone
    let host = ACursorHost::holding_a_cursor_session();

    // When the prompt hook fires and the transcript's prompt line is read
    host.prompt_is_submitted().await;
    host.transcript_line_is_read(0).await;

    // Then
    assert_eq!(
        host.transcript(),
        vec![("user_message".to_string(), PROMPT.to_string())]
    );
}

#[tokio::test]
async fn a_cursor_call_id_carries_no_newline() {
    // Given a cursor-cli session
    let host = ACursorHost::holding_a_cursor_session();

    // When Cursor reports calls whose ids embed a newline
    host.the_three_tool_calls_run().await;

    // Then no id holds a newline, and each call's running and finished rows still share one —
    // the log reads back one call per id
    assert_eq!(
        host.logged_call_ids(),
        vec![
            "call-64785683-composer_call_3AhseUhG_fc_199e15db_0",
            SHELL_ID,
            "call-64785683-composer_call_60DHMwTE_fc_199e15db_2",
        ]
    );
}
