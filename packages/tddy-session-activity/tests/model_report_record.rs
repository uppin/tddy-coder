//! A session's record follows the model its agent reports it is on (`coding_agent.model_report`):
//! a model the operator picked inside the agent — Cursor's or Claude Code's `/model` — replaces
//! the start's in `.session.yaml` `model`, which is what every resume relaunches the agent with.
//!
//! Seen 2026-11-24: a Cursor session started with `--model auto` was switched to GLM 5.3 Flash in
//! its TUI, and its resume relaunched it with `--model auto`. Every Cursor hook event carries the
//! model the session is on (`glm-5p3-flash-high`); Claude Code's `SessionStart` names its model, and
//! its transcript names each assistant message's.
//!
//! The path under test is the real `ReportSessionStatus` handler over a real session directory,
//! with the built-in agent specs: the record on disk is the behaviour.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::{Request, Status};
use tddy_service::proto::activity::{ActivityService as _, ReportSessionStatusRequest};
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, NoSessionDeltaStores, SessionLabels,
};
use tddy_usage::{write_usage_context, UsageContextFile};

const OS_USER: &str = "ada";
const SESSION_ID: &str = "01900000-0000-7000-8000-0000000007a1";
const HOOK_TOKEN: &str = "tok-model-report";
/// The chat a Cursor session owns, which its hooks report as their conversation.
const CURSOR_CHAT: &str = "7b8e6f39-53d5-4291-8074-ccf64d5da3b7";

struct NoLabels;

impl SessionLabels for NoLabels {
    fn label_for(&self, _sessions_base: &Path, _session_id: &str) -> String {
        String::new()
    }
}

/// One daemon host holding one session, recorded as started on `model`.
struct AnActivityHost {
    data_dir: tempfile::TempDir,
    /// The agent's HOME, where Claude Code keeps its transcripts.
    agent_home: tempfile::TempDir,
    service: ActivityServiceImpl,
}

impl AnActivityHost {
    fn holding_a_cursor_session_started_on(model: &str) -> Self {
        Self::holding("cursor-cli", "cursor", model)
    }

    fn holding_a_claude_session_started_on(model: &str) -> Self {
        Self::holding("claude-cli", "claude", model)
    }

    fn holding(session_type: &str, agent: &str, model: &str) -> Self {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        let agent_home = tempfile::tempdir().expect("an agent home");
        write_session(data_dir.path(), session_type, agent, model);
        write_usage_context(
            &data_dir.path().join("sessions").join(SESSION_ID),
            &UsageContextFile {
                spec_id: agent.to_string(),
                agent_home: Some(agent_home.path().to_path_buf()),
                conversation_id: Some(SESSION_ID.to_string()),
                worktree: None,
                model: Some(model.to_string()),
            },
        )
        .expect("the usage context");
        let agents = Arc::new(
            tddy_coding_agent_specs::CodingAgentRegistry::builtin()
                .expect("the built-in specs form a registry"),
        );
        let bindings = Arc::clone(&agents);
        let service = ActivityServiceImpl::new(ActivityPorts {
            os_users: Arc::new(|_token: &str| {
                Err(Status::unauthenticated(
                    "no session token is presented on the hook path",
                ))
            }),
            tddy_data_dir: data_dir.path().to_path_buf(),
            activity: Arc::new(AgentActivityHub::default()),
            conversation_frames: Arc::new(
                tddy_daemon_kernel::conversation_frames::ConversationFrames::default(),
            ),
            notifications: None,
            session_labels: Arc::new(NoLabels),
            deltas: Arc::new(NoSessionDeltaStores),
            daemon_instance_id: "host-a".to_string(),
            usage_bindings: Arc::new(move |spec: &str| {
                bindings.get(spec).and_then(|spec| spec.agent.usage.clone())
            }),
        })
        .with_agents(agents);
        Self {
            data_dir,
            agent_home,
            service,
        }
    }

    /// Claude Code has written the main conversation's transcript, one assistant message per
    /// model, in order.
    fn claude_answered_with(&self, models: &[&str]) -> &Self {
        let dir = self
            .agent_home
            .path()
            .join(".claude/projects/-home-ada-work");
        std::fs::create_dir_all(&dir).expect("the projects dir");
        let lines: String = models
            .iter()
            .enumerate()
            .map(|(n, model)| {
                format!(
                    "{}\n",
                    serde_json::json!({
                        "type": "assistant",
                        "message": {
                            "id": format!("msg_{n}"),
                            "model": model,
                            "usage": {"input_tokens": 10, "output_tokens": 2},
                        },
                    })
                )
            })
            .collect();
        std::fs::write(dir.join(format!("{SESSION_ID}.jsonl")), lines).expect("the transcript");
        self
    }

    async fn reported(&self, event: &str, status: &str, fields: &[(&str, &str)]) -> &Self {
        let mut hook_fields: HashMap<String, String> = fields
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        hook_fields.insert("hook_event_name".to_string(), event.to_string());
        let conversation = hook_fields
            .get("session_id")
            .cloned()
            .unwrap_or_else(|| SESSION_ID.to_string());
        self.service
            .report_session_status(Request::direct(ReportSessionStatusRequest {
                status: status.to_string(),
                hook_event_name: event.to_string(),
                hook_fields,
                session_id: SESSION_ID.to_string(),
                os_user: OS_USER.to_string(),
                hook_token: HOOK_TOKEN.to_string(),
                agent_conversation_id: conversation,
                ..Default::default()
            }))
            .await
            .expect("the hook report is accepted");
        self
    }

    fn session_dir(&self) -> PathBuf {
        self.data_dir.path().join("sessions").join(SESSION_ID)
    }

    /// The model `.session.yaml` records, which a resume relaunches the agent with.
    fn recorded_model(&self) -> Option<String> {
        tddy_core::read_session_metadata(&self.session_dir())
            .expect("the session's .session.yaml")
            .model
    }
}

fn write_session(data_dir: &Path, session_type: &str, agent: &str, model: &str) {
    let session_dir = data_dir.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("the session dir");
    let meta = SessionMetadata {
        owner_subject: None,
        display_name: None,
        session_id: SESSION_ID.to_string(),
        project_id: "a-project".to_string(),
        created_at: "2026-11-24T10:00:00Z".to_string(),
        updated_at: "2026-11-24T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(format!("/home/{OS_USER}/work")),
        pid: Some(4242),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some(session_type.to_string()),
        model: Some(model.to_string()),
        effort: None,
        cursor_chat_id: (session_type == "cursor-cli").then(|| CURSOR_CHAT.to_string()),
        harness_resume_token: None,
        activity_status: Some("Running".to_string()),
        hook_token: Some(HOOK_TOKEN.to_string()),
        sandbox: None,
        agent: Some(agent.to_string()),
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
    write_session_metadata(&session_dir, &meta).expect("the session record");
}

// ── Cursor: every hook event names the model ─────────────────────────────────────────────────

#[tokio::test]
async fn a_cursor_session_records_the_model_its_hooks_report() {
    // Given a Cursor session started on auto, switched to GLM 5.3 Flash with /model
    let host = AnActivityHost::holding_a_cursor_session_started_on("auto");

    // When its next prompt's hook reports the model it is on
    host.reported(
        "beforeSubmitPrompt",
        "Running",
        &[("model", "glm-5p3-flash-high"), ("session_id", CURSOR_CHAT)],
    )
    .await;

    // Then the record names that model, so a resume relaunches on it
    assert_eq!(host.recorded_model().as_deref(), Some("glm-5p3-flash-high"));
}

#[tokio::test]
async fn a_cursor_session_records_what_auto_reports_itself_as() {
    // Given a Cursor session started on auto, which Cursor reports as auto-smart
    let host = AnActivityHost::holding_a_cursor_session_started_on("auto");

    // When its turn ends
    host.reported(
        "stop",
        "Done",
        &[("model", "auto-smart"), ("session_id", CURSOR_CHAT)],
    )
    .await;

    // Then the record names the model as reported (operator decision 2026-11-24)
    assert_eq!(host.recorded_model().as_deref(), Some("auto-smart"));
}

#[tokio::test]
async fn a_report_naming_no_model_leaves_the_record_alone() {
    // Given a Cursor session on GLM 5.3 Flash
    let host = AnActivityHost::holding_a_cursor_session_started_on("glm-5p3-flash-high");

    // When a hook reports without a model field
    host.reported("stop", "Done", &[("session_id", CURSOR_CHAT)])
        .await;

    // Then the record keeps its model
    assert_eq!(host.recorded_model().as_deref(), Some("glm-5p3-flash-high"));
}

#[tokio::test]
async fn a_tool_call_on_another_model_does_not_repoint_a_cursor_session() {
    // Given a Cursor session on GLM 5.3 Flash
    let host = AnActivityHost::holding_a_cursor_session_started_on("glm-5p3-flash-high");

    // When a tool call's hook names another model (a subagent's)
    host.reported(
        "preToolUse",
        "ExecutingTool",
        &[("model", "composer-2.5"), ("session_id", CURSOR_CHAT)],
    )
    .await;

    // Then the record keeps the session's own model
    assert_eq!(host.recorded_model().as_deref(), Some("glm-5p3-flash-high"));
}

// ── Claude Code: SessionStart, then the transcript ───────────────────────────────────────────

#[tokio::test]
async fn a_claude_session_records_the_model_its_session_start_names() {
    // Given a Claude session recorded on an alias
    let host = AnActivityHost::holding_a_claude_session_started_on("haiku");

    // When Claude Code's SessionStart names the model it runs
    host.reported("SessionStart", "Started", &[("model", "claude-haiku-5-5")])
        .await;

    // Then the record names it
    assert_eq!(host.recorded_model().as_deref(), Some("claude-haiku-5-5"));
}

#[tokio::test]
async fn a_claude_session_records_a_model_picked_mid_session_from_its_transcript() {
    // Given a Claude session started on haiku, switched to Sonnet with /model before its last turn
    let host = AnActivityHost::holding_a_claude_session_started_on("claude-haiku-5-5");
    host.claude_answered_with(&["claude-haiku-5-5", "claude-sonnet-5-5"]);

    // When that turn ends (Claude Code's Stop names no model)
    host.reported("Stop", "Done", &[("last_assistant_message", "C2")])
        .await;

    // Then the record names the model the transcript answered with last
    assert_eq!(host.recorded_model().as_deref(), Some("claude-sonnet-5-5"));
}

#[tokio::test]
async fn a_message_claude_code_wrote_itself_is_not_a_model() {
    // Given a Claude session on Sonnet whose last message Claude Code wrote itself
    let host = AnActivityHost::holding_a_claude_session_started_on("claude-sonnet-5-5");
    host.claude_answered_with(&["claude-sonnet-5-5", "<synthetic>"]);

    // When the turn ends
    host.reported("Stop", "Done", &[]).await;

    // Then the record keeps the real model
    assert_eq!(host.recorded_model().as_deref(), Some("claude-sonnet-5-5"));
}

#[tokio::test]
async fn a_claude_prompt_names_no_model_and_reads_no_transcript() {
    // Given a Claude session whose transcript already shows another model
    let host = AnActivityHost::holding_a_claude_session_started_on("claude-haiku-5-5");
    host.claude_answered_with(&["claude-sonnet-5-5"]);

    // When only a prompt is reported (UserPromptSubmit is neither of Claude's model reports)
    host.reported("UserPromptSubmit", "Running", &[("prompt", "hi")])
        .await;

    // Then the record is unchanged until a turn ends
    assert_eq!(host.recorded_model().as_deref(), Some("claude-haiku-5-5"));
}
