//! A permission request's records, streamed into a conversation's transcript as they arrive: the request
//! pending while it is held for the user, then the answer replacing it — and the detail the web reads for the
//! request is the answer's, not the first record's.
//!
//! Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_agent_backend::{AcpToolStatus, AcpUpdate};
use tddy_daemon_kernel::conversation_frames::ConversationFrames;
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{PromptOutcome, SubagentError, SubagentSession, TurnRequest};
use tddy_service::acp_replay::tool_call_detail_in;
use tddy_service::proto::acp::{
    acp_agent_message, session_update, AcpAgentMessage, ToolCall, ToolCallStatus,
};
use tddy_service::subagent_conversation_replay::read_conversation_transcript;
use tddy_session_agents::conversation_recorder::ConversationRecorder;
use tokio::sync::broadcast;

/// A session that streams the records its test sends, and takes no turn of its own.
struct StreamsRecords {
    updates: broadcast::Sender<AcpUpdate>,
}

#[async_trait]
impl SubagentSession for StreamsRecords {
    async fn take_turn(&mut self, _request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        Err(SubagentError::from("these tests stream records, not turns"))
    }

    async fn prompt(&mut self, _text: &str) -> Result<PromptOutcome, SubagentError> {
        Err(SubagentError::from("these tests stream records, not turns"))
    }

    fn model(&self) -> &str {
        "stub-model"
    }

    fn cumulative_usage(&self) -> TokenUsage {
        TokenUsage::default()
    }

    fn context_tokens(&self) -> u64 {
        0
    }

    fn tail(&self, _max_messages: usize) -> Vec<String> {
        Vec::new()
    }

    fn updates(&self) -> Option<broadcast::Receiver<AcpUpdate>> {
        Some(self.updates.subscribe())
    }
}

fn pending_record() -> AcpUpdate {
    AcpUpdate::ToolCallRecord {
        id: "permission:perm-1".to_string(),
        title: "Permission: Run cargo test".to_string(),
        kind: "other".to_string(),
        status: AcpToolStatus::Pending,
        raw_input: Some(serde_json::json!({
            "requestId": "perm-1",
            "title": "Run cargo test",
            "deferredToUser": true,
        })),
        raw_output: None,
    }
}

fn answered_record() -> AcpUpdate {
    AcpUpdate::ToolCallRecord {
        id: "permission:perm-1".to_string(),
        title: "Permission: Run cargo test".to_string(),
        kind: "other".to_string(),
        status: AcpToolStatus::Completed,
        raw_input: Some(serde_json::json!({
            "requestId": "perm-1",
            "title": "Run cargo test",
            "deferredToUser": true,
        })),
        raw_output: Some(serde_json::json!({ "answeredBy": "alice", "optionId": "allow-once" })),
    }
}

/// The transcript's latest frame for `permission:perm-1`, once it reads `status`, within a bounded wait.
async fn transcript_of_permission_once_it_reads(
    session_dir: &Path,
    status: ToolCallStatus,
) -> Vec<AcpAgentMessage> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(frames) =
                read_conversation_transcript(session_dir, "conv-1").expect("the transcript reads")
            {
                if latest_status(&frames) == Some(status) {
                    return frames;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the transcript never read {status:?}"))
}

fn latest_status(frames: &[AcpAgentMessage]) -> Option<ToolCallStatus> {
    frames.iter().find_map(|frame| match &frame.msg {
        Some(acp_agent_message::Msg::SessionUpdate(note)) => {
            match note
                .update
                .as_ref()
                .and_then(|update| update.update.as_ref())
            {
                Some(session_update::Update::ToolCall(call)) if is_permission(call) => {
                    Some(call.status())
                }
                _ => None,
            }
        }
        _ => None,
    })
}

fn is_permission(call: &ToolCall) -> bool {
    call.tool_call_id
        .as_ref()
        .is_some_and(|id| id.value == "permission:perm-1")
}

#[tokio::test]
async fn a_streamed_answer_replaces_the_pending_request_and_its_detail_is_the_answer() {
    // Given
    let session_dir = tempfile::tempdir().expect("session dir");
    let (updates, _) = broadcast::channel(16);
    let session = StreamsRecords {
        updates: updates.clone(),
    };
    let _recorder = ConversationRecorder::open(
        session_dir.path(),
        "sess-1",
        "conv-1",
        Arc::new(ConversationFrames::default()),
        &session,
    )
    .expect("the recorder opens");

    // When the request is held for the user, its pending record is written while the turn is still running.
    updates.send(pending_record()).expect("a subscriber");
    let pending =
        transcript_of_permission_once_it_reads(session_dir.path(), ToolCallStatus::Pending).await;
    assert_eq!(pending.len(), 1, "{pending:?}");

    // When the answer is given, its completed record supersedes the pending one.
    updates.send(answered_record()).expect("a subscriber");
    let answered =
        transcript_of_permission_once_it_reads(session_dir.path(), ToolCallStatus::Completed).await;

    // Then the web's detail for the request is the answer, with the request's input kept.
    let detail =
        tool_call_detail_in(&answered, "permission:perm-1").expect("the request has a detail");
    assert_eq!(
        detail
            .raw_output
            .as_deref()
            .map(|output| serde_json::from_str::<serde_json::Value>(output).expect("JSON")),
        Some(serde_json::json!({ "answeredBy": "alice", "optionId": "allow-once" }))
    );
    let input: serde_json::Value =
        serde_json::from_str(detail.raw_input.as_deref().expect("the request's input"))
            .expect("JSON");
    assert_eq!(input["deferredToUser"], true);
}
