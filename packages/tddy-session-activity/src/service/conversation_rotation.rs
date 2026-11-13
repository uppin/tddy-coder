//! Following a session's agent through `/clear` (`#orchestrators` 6/11): a hook report is run
//! through the session's agent spec's `conversation.rotation` slot, and a rotation it completes is
//! appended to `.session.yaml` `agent_conversations`.
//!
//! The slot is two reports — the current conversation's `ends`, then a `begins`. The state between
//! them is a marker file in the session directory, not `ActivityServiceImpl` state: the service is
//! built per request by its callers (`activity_service()`), so memory would not survive from the
//! first report to the second, and `.session.yaml` (`deny_unknown_fields`) would need a new field.

use std::path::Path;

use tddy_coding_agent_specs::slots::conversation::{RotationState, RotationStep};
use tddy_core::agent_conversation::{
    record_agent_conversation, AgentConversation, AgentConversationReason,
};
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_rpc::Status;
use tddy_service::proto::activity::ReportSessionStatusRequest;

use crate::service::ActivityServiceImpl;

/// The marker an armed rotation leaves in its session directory.
const ARMED_MARKER: &str = ".conversation-rotation-armed";

fn rotation_state(session_dir: &Path) -> RotationState {
    if session_dir.join(ARMED_MARKER).exists() {
        RotationState::Armed
    } else {
        RotationState::Idle
    }
}

fn set_rotation_state(session_dir: &Path, state: RotationState) -> Result<(), Status> {
    let marker = session_dir.join(ARMED_MARKER);
    let result = match state {
        RotationState::Armed => std::fs::write(&marker, b""),
        RotationState::Idle => match std::fs::remove_file(&marker) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        },
    };
    result.map_err(|e| Status::internal(format!("failed to keep the rotation state: {e}")))
}

impl ActivityServiceImpl {
    /// Run `req` through the session's rotation slot and record a rotation it completes.
    pub(super) fn follow_conversation_rotation(
        &self,
        session_dir: &Path,
        req: &ReportSessionStatusRequest,
    ) -> Result<(), Status> {
        // Read after the status was written: the record this rewrites must carry it.
        let meta = tddy_core::read_session_metadata(session_dir)
            .map_err(|e| Status::internal(format!("failed to read the session record: {e}")))?;
        let session_type = meta.session_type.as_deref().unwrap_or("");
        let Some(rotation) = self
            .agents
            .for_session_type(session_type)
            .and_then(|spec| spec.conversation().rotation().cloned())
        else {
            return Ok(());
        };
        let current = meta
            .agent_conversations
            .last()
            .map_or(meta.session_id.as_str(), |c| c.id.as_str());
        let step = rotation.step(
            rotation_state(session_dir),
            current,
            &req.hook_event_name,
            &req.hook_fields,
        );
        match step {
            RotationStep::Unchanged => Ok(()),
            RotationStep::Arm => set_rotation_state(session_dir, RotationState::Armed),
            RotationStep::RotateTo(next) => {
                set_rotation_state(session_dir, RotationState::Idle)?;
                record_rotation(session_dir, &meta, next)
            }
        }
    }
}

/// Append `next` to the session's list, seeding a list-less session with its own id first.
fn record_rotation(session_dir: &Path, meta: &SessionMetadata, next: String) -> Result<(), Status> {
    let now = tddy_daemon_kernel::now_unix_ms();
    let mut updated = meta.clone();
    if updated.agent_conversations.is_empty() {
        let began = meta
            .created_at
            .parse::<chrono::DateTime<chrono::Utc>>()
            .map_or(now, |at| at.timestamp_millis().max(0) as u64);
        updated.agent_conversations.push(AgentConversation {
            id: meta.session_id.clone(),
            started_at_unix_ms: began,
            reason: AgentConversationReason::Start,
        });
    }
    record_agent_conversation(
        &mut updated.agent_conversations,
        AgentConversation {
            id: next,
            started_at_unix_ms: now,
            reason: AgentConversationReason::Rotate,
        },
    );
    write_session_metadata(session_dir, &updated)
        .map_err(|e| Status::internal(format!("failed to record the conversation rotation: {e}")))
}
