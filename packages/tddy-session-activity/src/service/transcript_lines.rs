//! What an agent's own transcript file says, recorded as the session's activity: the user's prompts
//! and the agent's replies as transcript message frames, and its tool calls paired with the rows
//! its hooks reported — or recorded as rows of their own when its spec reads tool calls from the
//! transcript (`coding_agent.activity`).
//!
//! A line is delivered by the session's transcript follower (`tddy-agent-launch`), already mapped by
//! the spec's declared rules into [`TranscriptEntry`]s — on this host or a target alike.
//!
//! **Order.** `ReadSessionTranscript` merges the message frames with the tool-call rows by their
//! timestamps, and a follower may read a line long after Cursor wrote it (it was restarted, or a
//! Coder target is slow), so "now" would put a reply after calls it preceded. What the transcript
//! says next happened before every call the hooks reported that the transcript has not reached
//! yet, so a message is stamped no later than the earliest such call's start. Text between two
//! calls that ran in parallel is placed before the later call's start: the replay places a call at
//! its completion, which the transcript cannot be compared with.

use std::path::Path;

use tddy_coding_agent_specs::{ActivitySlot, SpokenRole, TranscriptEntry};
use tddy_daemon_kernel::tool_turns::{SeenToolCall, ToolCallSource};
use tddy_rpc::Status;
use tddy_service::acp_replay::{append_acp_frame, session_update_frame};
use tddy_service::convert_acp::{agent_message_chunk, user_message_chunk};

use crate::service::ActivityServiceImpl;

/// One line of a session's agent transcript, as its follower delivers it.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptLine {
    pub session_id: String,
    /// The OS user the session belongs to: whose sessions directory holds it.
    pub os_user: String,
    /// What the line says, mapped by the session's spec, in the line's order.
    pub entries: Vec<TranscriptEntry>,
    /// The byte just past the line in the transcript file: names a tool call the transcript alone
    /// reports, so a re-read line names it the same.
    pub end_offset: u64,
}

/// The transcript frame's update for what `role` said.
pub(super) fn spoken_chunk(
    role: SpokenRole,
    text: &str,
) -> tddy_service::proto::acp::SessionUpdate {
    match role {
        SpokenRole::User => user_message_chunk(text.to_string()),
        SpokenRole::Agent => agent_message_chunk(text.to_string()),
    }
}

/// The key two reports of one message share, when both sources report messages.
pub(super) fn spoken_key(role: SpokenRole, text: &str) -> String {
    let role = match role {
        SpokenRole::User => "user",
        SpokenRole::Agent => "agent",
    };
    format!("{role}\u{0}{}", text.trim())
}

impl ActivityServiceImpl {
    /// Record one transcript line of `line.session_id`.
    ///
    /// Refused for a session whose agent declares no followed transcript: a line for it would be
    /// activity its spec never reads.
    pub async fn record_transcript_line(&self, line: TranscriptLine) -> Result<(), Status> {
        let (_sessions_base, session_dir) =
            self.hook_session_dir(&line.os_user, &line.session_id)?;
        let meta = self.hook_session(&session_dir)?;
        let session_type = meta.session_type.clone().unwrap_or_default();
        let Some(spec) = self
            .agents
            .for_session_type(&session_type)
            .filter(|spec| spec.activity().followed_transcript().is_some())
        else {
            return Err(Status::failed_precondition(format!(
                "session_type {session_type:?} is not an agent session that reads its transcript"
            )));
        };
        let activity = spec.activity().clone();
        let now = tddy_daemon_kernel::now_unix_ms();
        let turns = self.ports.activity.tool_turns();
        for (n, entry) in line.entries.iter().enumerate() {
            match entry {
                TranscriptEntry::UserMessage { text } | TranscriptEntry::AgentMessage { text } => {
                    let role = match entry {
                        TranscriptEntry::UserMessage { .. } => SpokenRole::User,
                        _ => SpokenRole::Agent,
                    };
                    self.record_transcript_message(
                        &session_dir,
                        &line.session_id,
                        &activity,
                        (role, text),
                        now,
                    )?;
                }
                TranscriptEntry::ToolUse { name, input, id } => {
                    let call_id = id
                        .as_deref()
                        .map(sanitized_call_id)
                        .unwrap_or_else(|| format!("transcript:{}:{n}", line.end_offset));
                    let paired = turns.saw(
                        &line.session_id,
                        ToolCallSource::Transcript,
                        SeenToolCall {
                            tool_name: name.clone(),
                            call_id: call_id.clone(),
                            started_unix_ms: now,
                        },
                    );
                    // A call the hooks reported is theirs to record: they carry its status and
                    // its result, which the transcript does not.
                    if paired.is_none() && activity.tool_calls.reads_transcript() {
                        self.record_transcript_tool_call(
                            &session_dir,
                            &line.session_id,
                            &session_type,
                            (call_id, name, input),
                            now,
                        );
                    }
                }
                TranscriptEntry::TurnEnd => turns.transcript_turn_ended(&line.session_id),
            }
        }
        Ok(())
    }

    /// Append one message the transcript says, when the spec reads messages from it — and when
    /// the spec reads them from the hooks too, only if the hooks did not already record it.
    fn record_transcript_message(
        &self,
        session_dir: &Path,
        session_id: &str,
        activity: &ActivitySlot,
        (role, text): (SpokenRole, &str),
        now: u64,
    ) -> Result<(), Status> {
        if !activity.messages.reads_transcript() {
            return Ok(());
        }
        if activity.messages.reads_hooks()
            && !self
                .ports
                .activity
                .first_time_spoken(session_id, &spoken_key(role, text))
        {
            return Ok(());
        }
        let at = self
            .ports
            .activity
            .tool_turns()
            .earliest_unreached_hook_call(session_id)
            .map_or(now, |started| now.min(started.saturating_sub(1)));
        let frame = session_update_frame(
            spoken_chunk(role, text),
            i64::try_from(at).unwrap_or(i64::MAX),
        );
        append_acp_frame(session_dir, &frame)
            .map_err(|e| Status::internal(format!("failed to append to the transcript: {e}")))?;
        // A live `StreamAcpReplay` tail follows the hub, not the file.
        self.ports.activity.publish_frame(session_id, frame);
        Ok(())
    }

    /// Record a tool call only the transcript reported, as one finished row: the transcript
    /// carries no status and no result, so the row says it ran and nothing about how it ended. A
    /// hook row that pairs with it later adopts its id and supersedes it.
    fn record_transcript_tool_call(
        &self,
        session_dir: &Path,
        session_id: &str,
        session_type: &str,
        (call_id, name, input): (String, &str, &serde_json::Value),
        now: u64,
    ) {
        let record = tddy_core::agent_activity::AgentActivityRecord {
            call_id,
            tool_name: name.to_string(),
            input: input.clone(),
            status: tddy_core::agent_activity::STATUS_COMPLETED.to_string(),
            result: serde_json::Value::Null,
            error_message: String::new(),
            started_unix_ms: now,
            completed_unix_ms: now,
            source: session_type.to_string(),
            head_commit: String::new(),
            activity_seq: crate::NO_TICK,
            changed_paths: Vec::new(),
        };
        if let Err(e) = tddy_core::agent_activity::append_agent_activity(session_dir, &record) {
            log::warn!(
                target: "tddy_daemon::connection_service",
                "agent_activity: failed to persist a transcript tool call for session {session_id}: {e}"
            );
        }
        self.ports.activity.publish(session_id, record);
    }
}

/// `id` with every control or whitespace character made `_`: an agent's tool-call id is a key of
/// the activity log and of a delta lookup, and Cursor's embed a newline.
pub(crate) fn sanitized_call_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_control() || c.is_whitespace() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_call_id_with_a_newline_and_a_tab_keeps_its_other_characters() {
        assert_eq!(
            sanitized_call_id("call-1-composer_call_3Ah\nfc_19\t0"),
            "call-1-composer_call_3Ah_fc_19_0"
        );
    }
}
