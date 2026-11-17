//! `WaitSessionWatchEvents`: an orchestrator streams its session watches to a process
//! (`#activity-wake` 4/5).
//!
//! `tddy-tools monitor` calls it in a loop and prints each event as one JSON line, for a native
//! monitor tool (Claude Code's `Monitor`) to turn into notifications. A call holds until a named
//! watch matches something or `timeout_ms` passes, like `WaitProjectNotifications`, and is capped at
//! the same [`MAX_WAIT_MS`]. The first call starts a stream and is answered its `stream_id`; each
//! later call presents it with `ack_seq`, the `next_ack_seq` of the batch it printed — a watch's
//! position advances only on that acknowledgement. While a stream keeps calling, its watches'
//! events ride on no tool answer and wake nobody.
//!
//! The stream itself is `tddy_session_activity::session_watches`'. That crate sits above this one,
//! so the port speaks in JSON values and plain arguments.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md

use async_trait::async_trait;
use serde_json::{json, Value};
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::project_home_inbox_tools::MAX_WAIT_MS;
use crate::project_home_tools::{tool_def, ProjectHomePort};
use crate::project_home_watch_tools::NO_SESSION_WATCHES;
use crate::ToolOutcome;

pub const WAIT_SESSION_WATCH_EVENTS: &str = "WaitSessionWatchEvents";

/// Whether `name` is [`WAIT_SESSION_WATCH_EVENTS`].
pub fn is_project_home_watch_stream_tool(name: &str) -> bool {
    name == WAIT_SESSION_WATCH_EVENTS
}

/// What one `WaitSessionWatchEvents` call asks for, once its arguments are read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchStreamRequest {
    /// The caller's watches to stream, as `CreateSessionWatch` returned them.
    pub watch_ids: Vec<String>,
    /// The `next_ack_seq` of the batch the caller printed; `None` on a stream's first call.
    pub ack_seq: Option<u64>,
    /// The stream the call continues; `None` starts one.
    pub stream_id: Option<String>,
    /// How long to hold the call, at most [`MAX_WAIT_MS`].
    pub timeout_ms: u64,
}

/// What the tool needs from the daemon: this host's watches, streamed for the caller.
#[async_trait]
pub trait ProjectHomeWatchStreamPort: ProjectHomePort {
    /// The next batch of `home`'s stream over `request.watch_ids`, held until one is matched or
    /// `request.timeout_ms` passes, answered as `{ stream_id, events, lost, ended, next_ack_seq }`.
    /// An error says what to do.
    async fn wait_watch_events(
        &self,
        home: &HomeCaller,
        request: WatchStreamRequest,
    ) -> Result<Value, String> {
        let _ = (home, request);
        Err(NO_SESSION_WATCHES.to_string())
    }
}

/// The tool's definition.
pub fn project_home_watch_stream_tool_defs() -> Vec<ToolDef> {
    vec![tool_def(
        WAIT_SESSION_WATCH_EVENTS,
        "Stream your session watches to a process: the call `tddy-tools monitor` makes in a loop. Run `tddy-tools monitor --started-by-me --kinds permission,question --ttl 3600` (or `--watch <watch_id>` to re-attach) under Claude Code's Monitor tool with timeout_ms 1800000, and each watched event arrives as a notification, mid-turn or idle; re-arm it with --watch and the watch_id of its first line when it expires — the new monitor takes the watch over at once, from what you last acknowledged. Holds until a named watch matches something or timeout_ms passes (at most 20000). Answers stream_id, events, lost, ended and next_ack_seq: pass stream_id and next_ack_seq back on the next call — a watch advances only past what you acknowledge. While a stream keeps calling, its watches' events are on no other tool answer and do not wake you; 15 s after its last call they are again.",
        json!({
            "type": "object",
            "properties": {
                "watch_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "description": "The watches to stream, as CreateSessionWatch or ListSessionWatches returned them."
                },
                "ack_seq": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "The next_ack_seq of the batch you printed; omit on a stream's first call."
                },
                "stream_id": {
                    "type": "string",
                    "description": "The stream_id the stream's first call returned; omit to start a stream."
                },
                "timeout_ms": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": MAX_WAIT_MS,
                    "description": "How long to wait, in milliseconds: at most 20000."
                }
            },
            "required": ["watch_ids"],
            "additionalProperties": false
        }),
    )]
}

/// Run [`WAIT_SESSION_WATCH_EVENTS`] for `home`. Bad arguments and the port's refusals come back as
/// an `is_error` outcome saying what to do; the role refusal is the daemon's and happens first.
pub async fn execute_project_home_watch_stream_tool(
    port: &dyn ProjectHomeWatchStreamPort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    match answer(port, home, args_json).await {
        Ok(value) => ToolOutcome::ok(value.to_string()),
        Err(message) => ToolOutcome::err(format!("{tool_name}: {message}")),
    }
}

/// The batch `args_json` asks the port for.
async fn answer(
    port: &dyn ProjectHomeWatchStreamPort,
    home: &HomeCaller,
    args_json: &str,
) -> Result<Value, String> {
    let args: Value =
        serde_json::from_str(args_json).map_err(|e| format!("invalid args_json: {e}"))?;
    port.wait_watch_events(home, request_from(&args)?).await
}

/// What a call asks for: its watches, its acknowledgement and stream, and how long to hold it.
fn request_from(args: &Value) -> Result<WatchStreamRequest, String> {
    Ok(WatchStreamRequest {
        watch_ids: watch_ids_argument(args)?,
        ack_seq: ack_seq_argument(args)?,
        stream_id: stream_id_argument(args)?,
        timeout_ms: timeout_argument(args)?,
    })
}

/// `watch_ids`: the watches to stream, as `CreateSessionWatch` returned them; at least one.
fn watch_ids_argument(args: &Value) -> Result<Vec<String>, String> {
    match args.get("watch_ids") {
        Some(Value::Array(ids)) if !ids.is_empty() => ids
            .iter()
            .map(|id| {
                id.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("watch_ids must name watches, not {id}"))
            })
            .collect(),
        Some(Value::Array(_)) => Err(
            "name at least one watch_id: the watch_id CreateSessionWatch or ListSessionWatches returned"
                .to_string(),
        ),
        None | Some(Value::Null) => Err(
            "watch_ids is required: the watch_id CreateSessionWatch or ListSessionWatches returned"
                .to_string(),
        ),
        Some(other) => Err(format!("watch_ids must name watches, not {other}")),
    }
}

/// `ack_seq`: the `next_ack_seq` of the batch the caller printed; absent on a first call.
fn ack_seq_argument(args: &Value) -> Result<Option<u64>, String> {
    match args.get("ack_seq") {
        None | Some(Value::Null) => Ok(None),
        Some(ack) => ack
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("ack_seq must be a whole log position, not {ack}")),
    }
}

/// `stream_id`: the stream the call continues; absent starts one.
fn stream_id_argument(args: &Value) -> Result<Option<String>, String> {
    match args.get("stream_id") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(stream_id)) if !stream_id.is_empty() => Ok(Some(stream_id.clone())),
        Some(other) => Err(format!(
            "stream_id must be the stream_id a stream's first call returned, not {other}"
        )),
    }
}

/// `timeout_ms`: how long to hold the call, at most [`MAX_WAIT_MS`]; absent, the cap.
fn timeout_argument(args: &Value) -> Result<u64, String> {
    match args.get("timeout_ms") {
        None | Some(Value::Null) => Ok(MAX_WAIT_MS),
        Some(timeout) => {
            let Some(timeout_ms) = timeout.as_u64() else {
                return Err(format!(
                    "timeout_ms must be a whole number of milliseconds, not {timeout}"
                ));
            };
            if timeout_ms > MAX_WAIT_MS {
                return Err(format!(
                    "timeout_ms must be at most {MAX_WAIT_MS}, not {timeout_ms}"
                ));
            }
            Ok(timeout_ms)
        }
    }
}
