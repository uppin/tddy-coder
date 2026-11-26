//! Reproduction: a claude-cli session's transcript holds its tool calls and nothing it said.
//!
//! Observed 2026-11-21 on the Coder target `mantasi/remote-dev` (session
//! `01a1fcba-5ee1-7240-9d69-a7512de61640`): `ReadSessionTranscript` listed the two tool calls and
//! neither the prompt nor the reply "probe done". The same holds for a local claude-cli session: no
//! claude-cli session directory has an `acp-transcript.jsonl` (only the coder presenter writes
//! one), so `tddy_service::acp_replay::read_session_transcript` has `agent-activity.jsonl` alone to
//! replay. Yet both texts reach the daemon: Claude Code's `UserPromptSubmit` carries `prompt` and
//! its `Stop` carries `last_assistant_message` — the payloads below are the ones that session's
//! hook log held — and the handler drops them once the status is recorded.
//!
//! The path under test is the real hook mapping (`hook_request::session_status_request`, which the
//! local `tddy-tools session-hook` and the remote hook-log follower both use) into the real
//! `ReportSessionStatus` handler over a real session directory, read back the way
//! `ReadSessionTranscript` reads it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt as _;
use pretty_assertions::assert_eq;
use prost::Message as _;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::{Request, Status};
use tddy_service::proto::acp::{acp_agent_message, content_block, session_update, AcpAgentMessage};
use tddy_service::proto::activity::{
    AcpReplayFrame, ActivityService, StreamAcpReplayRequest, StreamMode,
};
use tddy_session_activity::hook_request::session_status_request;
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, NoSessionDeltaStores, SessionLabels,
};

const OS_USER: &str = "ada";
const SESSION_ID: &str = "01a1fcba-5ee1-7240-9d69-a7512de61640";
const HOOK_TOKEN: &str = "tok-transcript-messages";
/// What a web client presents to `StreamAcpReplay`.
const WEB_TOKEN: &str = "web-token";

const PROMPT: &str =
    "Run `pwd` and `ls` in the shell, read README.md's first 5 lines, then reply \"probe done\".";
const REPLY: &str = "probe done";

struct EverySessionIsLabelled;

impl SessionLabels for EverySessionIsLabelled {
    fn label_for(&self, _sessions_base: &Path, _session_id: &str) -> String {
        "probe".to_string()
    }
}

/// One daemon host holding one claude-cli session.
struct AnActivityHost {
    data_dir: tempfile::TempDir,
    service: ActivityServiceImpl,
}

impl AnActivityHost {
    fn holding_a_claude_cli_session() -> Self {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        write_session(data_dir.path());
        let service = ActivityServiceImpl::new(ActivityPorts {
            os_users: Arc::new(|token: &str| {
                if token == WEB_TOKEN {
                    Ok(OS_USER.to_string())
                } else {
                    Err(Status::unauthenticated(
                        "no session token is presented on the hook path",
                    ))
                }
            }),
            tddy_data_dir: data_dir.path().to_path_buf(),
            activity: Arc::new(AgentActivityHub::default()),
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
        ));
        Self { data_dir, service }
    }

    fn session_dir(&self) -> PathBuf {
        self.data_dir.path().join("sessions").join(SESSION_ID)
    }

    /// The agent fires the hook `event` with `payload` on its stdin, and the hook reports it.
    async fn hook_fires(&self, event: &str, payload: serde_json::Value) {
        let request =
            session_status_request(SESSION_ID, HOOK_TOKEN, OS_USER, event, &payload.to_string())
                .unwrap_or_else(|| panic!("{event} maps to a status report"));
        self.service
            .report_session_status(Request::direct(request))
            .await
            .unwrap_or_else(|e| panic!("{event} is accepted: {e:?}"));
    }

    /// The agent ran a tool: its `PostToolUse` hook reports the call.
    async fn tool_call_reported(&self, tool_name: &str) {
        let payload = serde_json::json!({
            "session_id": SESSION_ID,
            "hook_event_name": "PostToolUse",
            "tool_name": tool_name,
            "tool_use_id": "toolu_01",
            "tool_input": {"command": "pwd && ls"},
            "tool_response": {"stdout": "README.md"},
        });
        let request = tddy_session_activity::hook_request::agent_activity_request(
            "PostToolUse",
            &payload.to_string(),
            SESSION_ID,
            HOOK_TOKEN,
            OS_USER,
        )
        .expect("PostToolUse maps to an activity report");
        self.service
            .report_agent_activity(Request::direct(request))
            .await
            .expect("the tool call is accepted");
    }

    /// A web client opens `StreamAcpReplay` on the session, live-only, before anything is reported.
    async fn live_tail_opened(
        &self,
    ) -> <ActivityServiceImpl as ActivityService>::StreamAcpReplayStream {
        self.service
            .stream_acp_replay(Request::direct(StreamAcpReplayRequest {
                session_token: WEB_TOKEN.to_string(),
                session_id: SESSION_ID.to_string(),
                mode: StreamMode::LiveOnly as i32,
                ..Default::default()
            }))
            .await
            .expect("the stream opens")
            .into_inner()
    }

    /// What `ReadSessionTranscript` lists: each frame's kind and text, in order.
    fn transcript(&self) -> Vec<(String, String)> {
        tddy_service::acp_replay::read_session_transcript(&self.session_dir())
            .expect("the transcript reads")
            .iter()
            .map(kind_and_text)
            .collect()
    }
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

fn write_session(data_dir: &Path) {
    let session_dir = data_dir.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("the session dir");
    let meta = SessionMetadata {
        owner_subject: None,
        display_name: None,
        effort: None,
        session_id: SESSION_ID.to_string(),
        project_id: "a-project".to_string(),
        created_at: "2026-11-21T06:12:45Z".to_string(),
        updated_at: "2026-11-21T06:12:45Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(format!("/home/coder/.tddy/repos/r/.worktrees/{SESSION_ID}")),
        pid: Some(4242),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("sonnet".to_string()),
        cursor_chat_id: None,
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
async fn the_prompt_and_the_agents_reply_reach_the_transcript() {
    // Given a claude-cli session
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When its agent is prompted and replies, as the probe's hook log recorded it
    host.hook_fires(
        "UserPromptSubmit",
        serde_json::json!({
            "session_id": SESSION_ID,
            "hook_event_name": "UserPromptSubmit",
            "permission_mode": "auto",
            "prompt": PROMPT,
        }),
    )
    .await;
    host.hook_fires(
        "Stop",
        serde_json::json!({
            "session_id": SESSION_ID,
            "hook_event_name": "Stop",
            "stop_hook_active": false,
            "last_assistant_message": REPLY,
        }),
    )
    .await;

    // Then the transcript holds what was said, in order
    assert_eq!(
        host.transcript(),
        vec![
            ("user_message".to_string(), PROMPT.to_string()),
            ("agent_message".to_string(), REPLY.to_string()),
        ]
    );
}

/// Hook reports a millisecond apart, so their order on the transcript's clock is theirs.
async fn a_moment() {
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
}

fn a_prompt(conversation: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "session_id": conversation,
        "hook_event_name": "UserPromptSubmit",
        "prompt": prompt,
    })
}

fn a_stop(conversation: &str, reply: &str) -> serde_json::Value {
    serde_json::json!({
        "session_id": conversation,
        "hook_event_name": "Stop",
        "stop_hook_active": false,
        "last_assistant_message": reply,
    })
}

#[tokio::test]
async fn a_tool_call_keeps_its_place_between_the_prompt_and_the_reply() {
    // Given a claude-cli session
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When it is prompted, runs a tool, and replies
    host.hook_fires("UserPromptSubmit", a_prompt(SESSION_ID, PROMPT))
        .await;
    a_moment().await;
    host.tool_call_reported("Bash").await;
    a_moment().await;
    host.hook_fires("Stop", a_stop(SESSION_ID, REPLY)).await;

    // Then the transcript reads in that order
    assert_eq!(
        host.transcript()
            .into_iter()
            .map(|(kind, _)| kind)
            .collect::<Vec<_>>(),
        vec!["user_message", "tool_call", "agent_message"]
    );
}

#[tokio::test]
async fn a_stop_with_no_reply_text_adds_no_agent_message() {
    // Given a claude-cli session that was prompted
    let host = AnActivityHost::holding_a_claude_cli_session();
    host.hook_fires("UserPromptSubmit", a_prompt(SESSION_ID, PROMPT))
        .await;

    // When its turn ends with no reply text
    host.hook_fires("Stop", a_stop(SESSION_ID, "")).await;

    // Then
    assert_eq!(
        host.transcript(),
        vec![("user_message".to_string(), PROMPT.to_string())]
    );
}

#[tokio::test]
async fn a_prompt_from_another_conversation_is_not_added() {
    // Given a claude-cli session, whose conversation is the session's own id
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When a Claude Code in one of its agent tabs — another conversation — is prompted and
    // replies, firing the session's hooks
    host.hook_fires("UserPromptSubmit", a_prompt("tab-chat-7", "tab prompt"))
        .await;
    host.hook_fires("Stop", a_stop("tab-chat-7", "tab reply"))
        .await;

    // Then the session's transcript holds none of it
    assert_eq!(host.transcript(), Vec::<(String, String)>::new());
}

/// The next frame a live tail delivers: its kind, text and `seq`.
async fn next_live_frame(
    tail: &mut <ActivityServiceImpl as ActivityService>::StreamAcpReplayStream,
) -> (String, String, u64) {
    let frame: AcpReplayFrame =
        tokio::time::timeout(std::time::Duration::from_secs(5), tail.next())
            .await
            .expect("a frame within the deadline")
            .expect("the stream is still open")
            .expect("a frame, not an error");
    let message = AcpAgentMessage::decode(frame.acp_agent_message.as_slice())
        .expect("the frame carries an AcpAgentMessage");
    let (kind, text) = kind_and_text(&message);
    (kind, text, frame.seq)
}

#[tokio::test]
async fn a_live_tail_receives_the_prompt_and_the_reply_among_the_tool_calls() {
    // Given a claude-cli session with a live transcript tail opened before anything is reported
    let host = AnActivityHost::holding_a_claude_cli_session();
    let mut tail = host.live_tail_opened().await;

    // When it is prompted, runs a tool, and replies
    host.hook_fires("UserPromptSubmit", a_prompt(SESSION_ID, PROMPT))
        .await;
    a_moment().await;
    host.tool_call_reported("Bash").await;
    a_moment().await;
    host.hook_fires("Stop", a_stop(SESSION_ID, REPLY)).await;

    // Then the tail delivers each in order, at the position a re-read gives it
    assert_eq!(
        vec![
            next_live_frame(&mut tail).await,
            next_live_frame(&mut tail).await,
            next_live_frame(&mut tail).await,
        ],
        vec![
            ("user_message".to_string(), PROMPT.to_string(), 0),
            ("tool_call".to_string(), "Bash pwd && ls".to_string(), 1),
            ("agent_message".to_string(), REPLY.to_string(), 2),
        ]
    );
}
