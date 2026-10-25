//! `ReadSessionActivity` and `ReadSessionTranscript`: following a session an orchestrator started,
//! a page at a time (`#start-parity` 6/9).
//!
//! Both tools page the same way, and neither can return everything at once: a call takes a
//! `cursor` (empty for the start) and a `limit` (default [`FOLLOW_PAGE_DEFAULT`], at most
//! [`FOLLOW_PAGE_MAX`] — a larger one is clamped and the answer says so), and answers `items`,
//! `next_cursor` and `has_more`.
//!
//! - **Activity** pages forward through `agent-activity.jsonl` by `activity_seq`, so a poll from
//!   the last `next_cursor` returns exactly what happened since. Its answer also carries the
//!   question the session is waiting on, if any.
//! - **Transcript** pages the frames the web's Activities view pages (`GetAcpReplayPage`), newest
//!   first; an item longer than [`TRANSCRIPT_ITEM_MAX_BYTES`] is cut and marked, and asking again
//!   with its `tool_call_id` returns it whole (`GetAcpToolCallDetail`).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde::{Deserialize, Serialize};
use tddy_core::agent_activity::AgentActivityRecord;
use tddy_core::pending_question::PendingQuestion;

use crate::catalog::ToolDef;

pub const READ_SESSION_ACTIVITY: &str = "ReadSessionActivity";
pub const READ_SESSION_TRANSCRIPT: &str = "ReadSessionTranscript";

/// The page size a call without a `limit` gets.
pub const FOLLOW_PAGE_DEFAULT: usize = 50;
/// The most items one page holds.
pub const FOLLOW_PAGE_MAX: usize = 200;
/// The longest a transcript item's text is returned before it is cut.
pub const TRANSCRIPT_ITEM_MAX_BYTES: usize = 4096;

/// One page of either tool's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FollowPage<T> {
    pub items: Vec<T>,
    /// Pass back as `cursor` to continue; for activity, also what to poll from later.
    pub next_cursor: String,
    pub has_more: bool,
    /// Set when the `limit` asked for was above [`FOLLOW_PAGE_MAX`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clamped_to: Option<usize>,
}

/// `ReadSessionActivity`'s answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityPage {
    #[serde(flatten)]
    pub page: FollowPage<AgentActivityRecord>,
    /// The question the session is waiting on, if it is waiting.
    pub pending_question: Option<PendingQuestion>,
}

/// One transcript entry as `ReadSessionTranscript` returns it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptItem {
    /// The frame's position from the oldest; what the cursor counts in.
    pub seq: u64,
    /// `agent_message`, `user_message`, `agent_thought` or `tool_call`.
    pub kind: String,
    pub text: String,
    /// Set on a tool call; ask with it for the call's whole input and output.
    pub tool_call_id: String,
    pub truncated: bool,
}

/// The page of `records` after `cursor` (an `activity_seq`; empty for the first), keeping only the
/// `statuses` asked for (all when empty).
pub fn page_activity(
    records: &[AgentActivityRecord],
    cursor: &str,
    limit: Option<usize>,
    statuses: &[String],
) -> Result<FollowPage<AgentActivityRecord>, String> {
    // TODO(session-follow): implement
    let _ = (records, cursor, limit, statuses);
    Err("TODO(session-follow): page_activity".to_string())
}

/// `text` cut to [`TRANSCRIPT_ITEM_MAX_BYTES`] on a character boundary, and whether it was cut.
pub fn truncate_transcript_text(text: &str) -> (String, bool) {
    // TODO(session-follow): implement
    (text.to_string(), false)
}

/// The two tools' definitions.
pub fn session_follow_tool_defs() -> Vec<ToolDef> {
    // TODO(session-follow): implement
    Vec::new()
}
