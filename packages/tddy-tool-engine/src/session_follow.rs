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
use serde_json::{json, Value};
use tddy_core::agent_activity::AgentActivityRecord;
use tddy_core::pending_question::PendingQuestion;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget};

use crate::catalog::ToolDef;
use crate::project_home_tools::{
    resolve_target, target_host_property, tool_def, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID,
};
use crate::ToolOutcome;

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

/// What the daemon reads for [`READ_SESSION_ACTIVITY`]: every call of the session, and the question
/// it waits on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionActivity {
    /// Every call, coalesced by `call_id`, in first-seen order — not the capped tail.
    pub records: Vec<AgentActivityRecord>,
    pub pending_question: Option<PendingQuestion>,
}

/// One tool call's whole input and output (`GetAcpToolCallDetail`'s answer).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolCallBodies {
    pub raw_input: Option<String>,
    pub raw_output: Option<String>,
}

/// The page size `limit` asks for, and the maximum it was clamped to when it asked for more.
fn page_size(limit: Option<usize>) -> (usize, Option<usize>) {
    match limit {
        None | Some(0) => (FOLLOW_PAGE_DEFAULT, None),
        Some(n) if n > FOLLOW_PAGE_MAX => (FOLLOW_PAGE_MAX, Some(FOLLOW_PAGE_MAX)),
        Some(n) => (n, None),
    }
}

/// `cursor` as a position, or `None` for the start (empty). A cursor that is not a decimal position
/// is refused: it is not something a page ever returned.
fn parse_cursor(cursor: &str) -> Result<Option<u64>, String> {
    let cursor = cursor.trim();
    if cursor.is_empty() {
        return Ok(None);
    }
    cursor.parse::<u64>().map(Some).map_err(|_| {
        format!("invalid cursor {cursor:?}: pass back the next_cursor a page returned, or leave it empty")
    })
}

/// The page of `records` after `cursor` (an `activity_seq`; empty for the first), keeping only the
/// `statuses` asked for (all when empty).
///
/// `next_cursor` is the last returned call's `activity_seq`, so a poll from it returns exactly what
/// happened after; on an empty page it is `cursor` unchanged.
pub fn page_activity(
    records: &[AgentActivityRecord],
    cursor: &str,
    limit: Option<usize>,
    statuses: &[String],
) -> Result<FollowPage<AgentActivityRecord>, String> {
    let after = parse_cursor(cursor)?;
    let (size, clamped_to) = page_size(limit);
    let mut matching: Vec<&AgentActivityRecord> = records
        .iter()
        .filter(|r| after.is_none_or(|seq| r.activity_seq > seq))
        .filter(|r| statuses.is_empty() || statuses.contains(&r.status))
        .collect();
    matching.sort_by_key(|r| r.activity_seq);
    let has_more = matching.len() > size;
    let items: Vec<AgentActivityRecord> = matching.into_iter().take(size).cloned().collect();
    let next_cursor = items
        .last()
        .map(|r| r.activity_seq.to_string())
        .unwrap_or_else(|| cursor.trim().to_string());
    Ok(FollowPage {
        items,
        next_cursor,
        has_more,
        clamped_to,
    })
}

/// The page of `items` (a transcript, oldest first, whose index is each item's `seq`) at `cursor`.
///
/// `newest_first` (the default) reads the page of items older than `cursor` — the newest page when
/// it is empty — oldest first within the page, as `GetAcpReplayPage` returns it; its `next_cursor`
/// is the page's first `seq`. `from_cursor` reads forward from `cursor` (the start when empty); its
/// `next_cursor` is the position after the page.
pub fn page_transcript(
    items: &[TranscriptItem],
    cursor: &str,
    limit: Option<usize>,
    direction: &str,
) -> Result<FollowPage<TranscriptItem>, String> {
    let at = parse_cursor(cursor)?;
    let (size, clamped_to) = page_size(limit);
    let len = items.len();
    let (start, end) = match direction {
        "" | "newest_first" => {
            let end = at.map_or(len, |c| usize::try_from(c).unwrap_or(usize::MAX).min(len));
            (end.saturating_sub(size), end)
        }
        "from_cursor" => {
            let start = at.map_or(0, |c| usize::try_from(c).unwrap_or(usize::MAX).min(len));
            (start, (start + size).min(len))
        }
        other => {
            return Err(format!(
                "invalid direction {other:?}: use newest_first or from_cursor"
            ))
        }
    };
    let newest_first = matches!(direction, "" | "newest_first");
    let page = items[start..end]
        .iter()
        .map(|item| {
            let (text, truncated) = truncate_transcript_text(&item.text);
            TranscriptItem {
                text,
                truncated,
                ..item.clone()
            }
        })
        .collect();
    Ok(FollowPage {
        items: page,
        next_cursor: if newest_first { start } else { end }.to_string(),
        has_more: if newest_first { start > 0 } else { end < len },
        clamped_to,
    })
}

/// `text` cut to [`TRANSCRIPT_ITEM_MAX_BYTES`] on a character boundary, and whether it was cut.
pub fn truncate_transcript_text(text: &str) -> (String, bool) {
    if text.len() <= TRANSCRIPT_ITEM_MAX_BYTES {
        return (text.to_string(), false);
    }
    let mut end = TRANSCRIPT_ITEM_MAX_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

fn paging_properties() -> serde_json::Map<String, Value> {
    let mut properties = serde_json::Map::new();
    properties.insert(
        "session_id".into(),
        json!({ "type": "string", "description": "The session of this project to follow, as ListProjectSessions reports it." }),
    );
    properties.insert(TARGET_DAEMON_INSTANCE_ID.into(), target_host_property());
    properties.insert(
        "limit".into(),
        json!({
            "type": "integer",
            "minimum": 1,
            "maximum": FOLLOW_PAGE_MAX,
            "description": format!("Items per page. Default {FOLLOW_PAGE_DEFAULT}, at most {FOLLOW_PAGE_MAX}: a larger value is clamped and the answer carries clamped_to."),
        }),
    );
    properties
}

fn follow_schema(
    cursor_description: &str,
    extra: impl IntoIterator<Item = (&'static str, Value)>,
) -> Value {
    let mut properties = paging_properties();
    properties.insert(
        "cursor".into(),
        json!({ "type": "string", "description": cursor_description }),
    );
    for (name, schema) in extra {
        properties.insert(name.into(), schema);
    }
    json!({ "type": "object", "properties": properties, "required": ["session_id"] })
}

/// The two tools' definitions.
pub fn session_follow_tool_defs() -> Vec<ToolDef> {
    vec![
        tool_def(
            READ_SESSION_ACTIVITY,
            "Read what a session of this project has done: its tool calls, oldest first, a page at a time, with the question it is waiting on (pending_question) when it waits. Answers items, next_cursor and has_more; poll again with cursor set to the last next_cursor to read only what is new. Never returns more than one page.",
            follow_schema(
                "Where to continue: the next_cursor of the last page. Empty reads from the start.",
                [(
                    "statuses",
                    json!({
                        "type": "array",
                        "items": { "type": "string", "enum": ["running", "completed", "error"] },
                        "description": "Keep only calls in these states; all when empty."
                    }),
                )],
            ),
        ),
        tool_def(
            READ_SESSION_TRANSCRIPT,
            format!("Read a session's transcript, as the Activities view shows it: the newest page first, each page oldest first, a page at a time. Answers items, next_cursor and has_more; each item's text is cut at {TRANSCRIPT_ITEM_MAX_BYTES} bytes and marked truncated. Pass tool_call_id to read one tool call's whole raw_input and raw_output instead.").as_str(),
            follow_schema(
                "Where to continue: the next_cursor of the last page. Empty starts at the newest (newest_first) or the oldest (from_cursor).",
                [
                    (
                        "direction",
                        json!({
                            "type": "string",
                            "enum": ["newest_first", "from_cursor"],
                            "description": "newest_first (default) pages back from the newest item; from_cursor pages forward from cursor."
                        }),
                    ),
                    (
                        "tool_call_id",
                        json!({ "type": "string", "description": "Return this tool call whole (raw_input, raw_output) instead of a page; a truncated item names it." }),
                    ),
                ],
            ),
        ),
    ]
}

#[derive(Deserialize)]
struct FollowArgs {
    session_id: String,
    #[serde(default)]
    cursor: String,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    statuses: Vec<String>,
    #[serde(default)]
    direction: String,
    #[serde(default)]
    tool_call_id: String,
}

/// Run [`READ_SESSION_ACTIVITY`] or [`READ_SESSION_TRANSCRIPT`] for `home`.
pub async fn run_session_follow_tool(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    tool_name: &str,
    args: Value,
) -> ToolOutcome {
    let args: FollowArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolOutcome::err(format!("{tool_name}: invalid arguments: {e}")),
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    let answer = if tool_name == READ_SESSION_ACTIVITY {
        read_activity(port, &target, &args).await
    } else if !args.tool_call_id.is_empty() {
        read_tool_call(port, &target, &args.tool_call_id).await
    } else {
        read_transcript(port, &target, &args).await
    };
    match answer {
        Ok(value) => ToolOutcome::ok(value.to_string()),
        Err(message) => ToolOutcome::err(format!("{tool_name}: {message}")),
    }
}

async fn read_activity(
    port: &dyn ProjectHomePort,
    target: &SessionTarget,
    args: &FollowArgs,
) -> Result<Value, String> {
    let read = port
        .session_activity(target)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let page = page_activity(&read.records, &args.cursor, args.limit, &args.statuses)?;
    serde_json::to_value(ActivityPage {
        page,
        pending_question: read.pending_question,
    })
    .map_err(|e| e.to_string())
}

async fn read_transcript(
    port: &dyn ProjectHomePort,
    target: &SessionTarget,
    args: &FollowArgs,
) -> Result<Value, String> {
    let items = port
        .session_transcript(target)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let page = page_transcript(&items, &args.cursor, args.limit, &args.direction)?;
    serde_json::to_value(page).map_err(|e| e.to_string())
}

async fn read_tool_call(
    port: &dyn ProjectHomePort,
    target: &SessionTarget,
    tool_call_id: &str,
) -> Result<Value, String> {
    let bodies = port
        .session_tool_call(target, tool_call_id)
        .await
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| {
            format!(
                "no tool call with id {tool_call_id} in session {}",
                target.session_id
            )
        })?;
    Ok(json!({
        "tool_call_id": tool_call_id,
        "raw_input": bodies.raw_input,
        "raw_output": bodies.raw_output,
    }))
}
