//! A subagent conversation's own replayable transcript, and the one entry the session's transcript
//! shows for it.
//!
//! Every conversation the daemon runs records its turns as ACP frames under the session it belongs
//! to, one transcript per conversation, read back through the same replay RPCs with a
//! `conversation_id`. In the session's own transcript, the main agent's `mcp__tddy-tools__subagent_*`
//! calls for one conversation fold into **one** tool-call entry whose id is
//! [`SUBAGENT_TOOL_CALL_PREFIX`] + the conversation id — the activity pane's subagent bubble.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use std::io;
use std::path::{Path, PathBuf};

use crate::proto::acp::AcpAgentMessage;

/// The `tool_call_id` prefix of a session transcript's subagent-conversation entry.
pub const SUBAGENT_TOOL_CALL_PREFIX: &str = "subagent:";

/// The main agent's MCP tool names whose calls fold into a conversation's entry.
pub const SUBAGENT_TOOL_NAME_PREFIX: &str = "mcp__tddy-tools__subagent_";

/// Where `conversation_id`'s transcript lives under `session_dir`.
pub fn conversation_transcript_dir(session_dir: &Path, conversation_id: &str) -> PathBuf {
    session_dir.join("conversations").join(conversation_id)
}

/// Append one frame to `conversation_id`'s transcript, creating it on the first frame.
pub fn append_conversation_frame(
    _session_dir: &Path,
    _conversation_id: &str,
    _frame: &AcpAgentMessage,
) -> io::Result<()> {
    // TODO(conversation-bubble): implement
    Err(io::Error::other(
        "TODO(conversation-bubble): append_conversation_frame is not implemented",
    ))
}

/// `conversation_id`'s recorded transcript; `None` when that conversation recorded nothing (an
/// in-jail loop, or an id this session never ran).
pub fn read_conversation_transcript(
    _session_dir: &Path,
    _conversation_id: &str,
) -> io::Result<Option<Vec<AcpAgentMessage>>> {
    // TODO(conversation-bubble): implement
    Ok(None)
}

/// Fold the main agent's `subagent_*` tool calls into one entry per conversation, in place of the
/// first call that named it, with the status of the last.
pub fn fold_subagent_calls(frames: Vec<AcpAgentMessage>) -> Vec<AcpAgentMessage> {
    // TODO(conversation-bubble): implement
    frames
}
