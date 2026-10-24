//! Recording a daemon-run conversation's turns, so the web can replay the conversation itself.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use std::io;
use std::path::Path;

use tddy_discovery::subagent::PromptOutcome;

/// Record one finished turn of `conversation_id` under `session_dir`: the prompt that started it
/// (`None` for a resume), the agent's text, every tool call its messages name, a pending permission
/// request, and the stop reason — as ACP frames stamped `at_unix_ms`.
///
/// A coding agent's turn records its tool calls live, as they stream
/// (`CodingAgentSession::record_updates_with`); for it this records the prompt and the end.
pub fn record_turn(
    _session_dir: &Path,
    _conversation_id: &str,
    _prompt: Option<&str>,
    _outcome: &PromptOutcome,
    _at_unix_ms: i64,
) -> io::Result<()> {
    // TODO(conversation-bubble): implement
    Err(io::Error::other(
        "TODO(conversation-bubble): record_turn is not implemented",
    ))
}
