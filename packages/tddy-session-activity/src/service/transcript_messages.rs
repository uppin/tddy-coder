//! What a hook-driven agent said, written to the session's ACP transcript.
//!
//! Only the coder presenter writes `acp-transcript.jsonl`, so a claude-cli session's transcript
//! would hold its tool calls and nothing it said. The agent's spec names the hook events that carry
//! what was said (Claude Code's `UserPromptSubmit` carries the `prompt` and its `Stop` carries
//! `last_assistant_message`); this records each as a message frame.

use std::path::Path;

use tddy_coding_agent_specs::ActivitySlot;
use tddy_rpc::Status;
use tddy_service::acp_replay::{append_acp_frame, session_update_frame};
use tddy_service::proto::activity::ReportSessionStatusRequest;

use crate::service::ActivityServiceImpl;

impl ActivityServiceImpl {
    /// Append the prompt or the reply a status report carries, when the agent's spec names its
    /// event as one that carries what was said (`coding_agent.activity.hook_messages`) and it is
    /// the session's own conversation that spoke.
    ///
    /// The conversation is the last of the session's recorded ones, else the session's own id —
    /// read after [`Self::follow_conversation_rotation`] may have moved it. Another conversation
    /// firing this session's hooks (an agent tab) is not the session's transcript, and is ignored.
    /// A spec that reads messages from its transcript as well records a message said in both once.
    pub(super) fn record_spoken_message(
        &self,
        session_dir: &Path,
        req: &ReportSessionStatusRequest,
        activity: &ActivitySlot,
    ) -> Result<(), Status> {
        let Some((role, text)) = activity.spoken_in_hook(&req.hook_event_name, &req.hook_fields)
        else {
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
        if activity.messages.reads_transcript()
            && !self.ports.activity.first_time_spoken(
                &req.session_id,
                &super::transcript_lines::spoken_key(role, text),
            )
        {
            return Ok(());
        }

        let at = i64::try_from(tddy_daemon_kernel::now_unix_ms()).unwrap_or(i64::MAX);
        let frame = session_update_frame(super::transcript_lines::spoken_chunk(role, text), at);
        append_acp_frame(session_dir, &frame)
            .map_err(|e| Status::internal(format!("failed to append to the transcript: {e}")))?;
        // A live `StreamAcpReplay` tail follows the hub, not the file.
        self.ports.activity.publish_frame(&req.session_id, frame);
        Ok(())
    }
}
