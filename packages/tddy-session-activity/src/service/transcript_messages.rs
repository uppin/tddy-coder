//! What a hook-driven agent said, written to the session's ACP transcript.
//!
//! Only the coder presenter writes `acp-transcript.jsonl`, so a claude-cli session's transcript
//! would hold its tool calls and nothing it said. Claude Code's `UserPromptSubmit` carries the
//! `prompt` and its `Stop` carries `last_assistant_message`; this records both as message frames.

use std::path::Path;

use tddy_rpc::Status;
use tddy_service::acp_replay::{append_acp_frame, session_update_frame};
use tddy_service::convert_acp::{agent_message_chunk, user_message_chunk};
use tddy_service::proto::acp::SessionUpdate;
use tddy_service::proto::activity::ReportSessionStatusRequest;

use crate::service::ActivityServiceImpl;

impl ActivityServiceImpl {
    /// Append the prompt or the reply a status report carries, when it is the session's own
    /// conversation that spoke.
    ///
    /// The conversation is the last of the session's recorded ones, else the session's own id —
    /// read after [`Self::follow_conversation_rotation`] may have moved it. Another conversation
    /// firing this session's hooks (an agent tab) is not the session's transcript, and is ignored.
    pub(super) fn record_spoken_message(
        &self,
        session_dir: &Path,
        req: &ReportSessionStatusRequest,
    ) -> Result<(), Status> {
        let (field, chunk_of): (&str, fn(String) -> SessionUpdate) =
            match req.hook_event_name.as_str() {
                "UserPromptSubmit" => ("prompt", user_message_chunk),
                "Stop" => ("last_assistant_message", agent_message_chunk),
                _ => return Ok(()),
            };
        let Some(text) = req.hook_fields.get(field).filter(|t| !t.is_empty()) else {
            return Ok(());
        };

        let meta = tddy_core::read_session_metadata(session_dir)
            .map_err(|e| Status::internal(format!("failed to read the session record: {e}")))?;
        let own_conversation = meta
            .agent_conversations
            .last()
            .map_or(meta.session_id.as_str(), |c| c.id.as_str());
        if req.agent_conversation_id != own_conversation {
            return Ok(());
        }

        let at = i64::try_from(tddy_daemon_kernel::now_unix_ms()).unwrap_or(i64::MAX);
        let frame = session_update_frame(chunk_of(text.clone()), at);
        append_acp_frame(session_dir, &frame)
            .map_err(|e| Status::internal(format!("failed to append to the transcript: {e}")))
    }
}
