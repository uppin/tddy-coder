//! `WaitSessionWatchEvents`: an orchestrator streams its session watches to a process
//! (`#activity-wake` 4/5).
//!
//! An orchestrator calls it directly to wait for what its watches match, and `tddy-tools monitor`
//! calls it in a loop and prints each event as one JSON line, for a native monitor tool (Claude
//! Code's `Monitor`) to turn into notifications. A call waits in a **window**: it returns once it
//! holds `max_events` events (default 1), `settle_ms` after its first event when that is set, as
//! soon as there is a `lost` count or an ended watch to report, or when `timeout_ms` passes (default
//! [`DEFAULT_WATCH_WAIT_MS`], at most [`MAX_WATCH_WAIT_MS`]). It may **pick** by `kinds` and
//! `session_ids`; what it does not pick stays pending. The first call starts a stream and is
//! answered its `stream_id`; each later call presents it with `ack_seq`, the `next_ack_seq` of the
//! batch it printed — a watch's position advances only on that acknowledgement, and only past what
//! was handed. While a stream keeps calling, its watches' events ride on no tool answer and wake
//! nobody.
//!
//! The stream itself is `tddy_session_activity::session_watches`'. That crate sits above this one,
//! so the port speaks in JSON values and plain arguments.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md

use async_trait::async_trait;
use serde_json::{json, Value};
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, ProjectHomePort};
use crate::project_home_watch_tools::{NO_SESSION_WATCHES, WATCH_KINDS};
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
    /// How long to hold the call, at most [`MAX_WATCH_WAIT_MS`]; [`DEFAULT_WATCH_WAIT_MS`] when the
    /// call names none.
    pub timeout_ms: u64,
    /// The call returns once it holds this many events, and carries at most this many: from 1 to
    /// [`MAX_EVENTS_PER_WAIT`], 1 when the call names none.
    pub max_events: usize,
    /// How long the call keeps collecting after its first event, while it holds fewer than
    /// `max_events`: at most [`MAX_SETTLE_MS`], 0 when the call names none.
    pub settle_ms: u64,
    /// Which kinds this call returns, a subset of what the watches match; every kind when empty.
    pub kinds: Vec<String>,
    /// Which sessions' events this call returns; every session the watches cover when empty.
    pub session_ids: Vec<String>,
}

/// How long a wait is held when the call names no `timeout_ms`: a minute.
pub const DEFAULT_WATCH_WAIT_MS: u64 = 60_000;

/// The longest a wait may be held: 30 minutes, as long as Claude Code's `Monitor` runs one command.
/// A tool call held past about two minutes is moved by Claude Code to the background as a task,
/// whose result arrives later as a notification — so a hold past ~120 s does not block the agent,
/// and the answer still comes back whole. Over LiveKit the call is clamped further, to the remote
/// block ceiling (`tddy_session_tool_client::MAX_REMOTE_BLOCK_MS`), which guards against a peer
/// that died mid-call.
pub const MAX_WATCH_WAIT_MS: u64 = 1_800_000;

/// The most events one wait carries — `tddy_session_activity::session_watches::MAX_EVENTS_PER_ANSWER`,
/// which this crate does not depend on.
pub const MAX_EVENTS_PER_WAIT: usize = 50;

/// The longest `settle_ms` a wait may name: 5 s.
pub const MAX_SETTLE_MS: u64 = 5_000;

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
        "Wait for what your session watches match — the way to follow your sessions; watches do not wake you unless created with wake_when_idle. A call returns once it holds max_events events (default 1, so the first event returns at once; at most 50, which is also the most it carries), settle_ms after its first event when you set it (to take a burst in one answer), at once when events were lost or a watch ended, or when timeout_ms passes (default 60000 — a minute; at most 1800000). A call held past about two minutes runs in the background as a task, and its answer arrives when it returns. Narrow one call with kinds and session_ids: what it does not pick stays pending for a later call — it is neither returned nor acknowledged. Answers stream_id, events, lost, ended and next_ack_seq: pass stream_id and next_ack_seq back on the next call — a watch advances only past what that acknowledges. To be notified mid-turn instead, run `tddy-tools monitor --started-by-me --kinds permission,question --ttl 3600` (or `--watch <watch_id>` to re-attach) under Claude Code's Monitor tool with timeout_ms 1800000; re-arm it with --watch and the watch_id of its first line when it expires. While a stream keeps calling, its watches' events are on no other tool answer and wake nobody; 15 s after its last call they are again.",
        json!({
            "type": "object",
            "properties": {
                "watch_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1,
                    "description": "The watches to wait on, as CreateSessionWatch or ListSessionWatches returned them."
                },
                "ack_seq": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "The next_ack_seq of the answer you acted on; omit on a stream's first call."
                },
                "stream_id": {
                    "type": "string",
                    "description": "The stream_id the stream's first call returned; omit to start a stream."
                },
                "timeout_ms": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": MAX_WATCH_WAIT_MS,
                    "default": DEFAULT_WATCH_WAIT_MS,
                    "description": "The window, in milliseconds: how long to wait for max_events. Default 60000 (a minute), at most 1800000 (30 minutes); past about two minutes the call runs in the background and its answer arrives later. 0 answers at once."
                },
                "max_events": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_EVENTS_PER_WAIT,
                    "default": 1,
                    "description": "Return once this many events are held, and carry at most this many: from 1 to 50, default 1."
                },
                "settle_ms": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": MAX_SETTLE_MS,
                    "default": 0,
                    "description": "After the first event, keep collecting this long before returning, unless max_events comes first: at most 5000, default 0 (no settle)."
                },
                "kinds": {
                    "type": "array",
                    "items": { "type": "string", "enum": WATCH_KINDS },
                    "description": "Return only these kinds; default every kind the watches match. Other events stay pending."
                },
                "session_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Return only these sessions' events; default every session the watches cover. Other events stay pending."
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

/// What a call asks for, read from its parsed arguments — the same read
/// [`execute_project_home_watch_stream_tool`] makes, for a daemon serving a call it has already
/// routed itself (the inbox watch of a session that is not an orchestrator, `#activity-wake` 5/5).
pub fn request_from(args: &Value) -> Result<WatchStreamRequest, String> {
    Ok(WatchStreamRequest {
        watch_ids: watch_ids_argument(args)?,
        ack_seq: ack_seq_argument(args)?,
        stream_id: stream_id_argument(args)?,
        timeout_ms: timeout_argument(args)?,
        max_events: max_events_argument(args)?,
        settle_ms: settle_argument(args)?,
        kinds: kinds_argument(args)?,
        session_ids: session_ids_argument(args)?,
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

/// `timeout_ms`: how long to hold the call, at most [`MAX_WATCH_WAIT_MS`]; absent,
/// [`DEFAULT_WATCH_WAIT_MS`].
fn timeout_argument(args: &Value) -> Result<u64, String> {
    match args.get("timeout_ms") {
        None | Some(Value::Null) => Ok(DEFAULT_WATCH_WAIT_MS),
        Some(timeout) => {
            let Some(timeout_ms) = timeout.as_u64() else {
                return Err(format!(
                    "timeout_ms must be a whole number of milliseconds, not {timeout}"
                ));
            };
            if timeout_ms > MAX_WATCH_WAIT_MS {
                return Err(format!(
                    "timeout_ms must be at most {MAX_WATCH_WAIT_MS} (30 minutes), not {timeout_ms}"
                ));
            }
            Ok(timeout_ms)
        }
    }
}

/// `max_events`: from 1 to [`MAX_EVENTS_PER_WAIT`]; absent, 1.
fn max_events_argument(args: &Value) -> Result<usize, String> {
    match args.get("max_events") {
        None | Some(Value::Null) => Ok(1),
        Some(count) => count
            .as_u64()
            .and_then(|count| usize::try_from(count).ok())
            .filter(|count| (1..=MAX_EVENTS_PER_WAIT).contains(count))
            .ok_or_else(|| {
                format!("max_events must be a whole number from 1 to {MAX_EVENTS_PER_WAIT}, not {count}")
            }),
    }
}

/// `settle_ms`: at most [`MAX_SETTLE_MS`]; absent, 0.
fn settle_argument(args: &Value) -> Result<u64, String> {
    match args.get("settle_ms") {
        None | Some(Value::Null) => Ok(0),
        Some(settle) => settle
            .as_u64()
            .filter(|settle_ms| *settle_ms <= MAX_SETTLE_MS)
            .ok_or_else(|| {
                format!("settle_ms must be a whole number of milliseconds, at most {MAX_SETTLE_MS}, not {settle}")
            }),
    }
}

/// `kinds`: watch kinds, by name; absent, every kind.
fn kinds_argument(args: &Value) -> Result<Vec<String>, String> {
    match args.get("kinds") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(kinds)) => kinds
            .iter()
            .map(|kind| {
                kind.as_str()
                    .filter(|name| WATCH_KINDS.contains(name))
                    .map(str::to_string)
                    .ok_or_else(|| {
                        format!("unknown kind {kind}: kinds are {}", WATCH_KINDS.join(", "))
                    })
            })
            .collect(),
        Some(other) => Err(format!("kinds must name kinds, not {other}")),
    }
}

/// `session_ids`: session ids; absent, every session the watches cover.
fn session_ids_argument(args: &Value) -> Result<Vec<String>, String> {
    let refuse = |what: &Value| format!("session_ids must name sessions, not {what}");
    match args.get("session_ids") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(ids)) => ids
            .iter()
            .map(|id| id.as_str().map(str::to_string).ok_or_else(|| refuse(id)))
            .collect(),
        Some(other) => Err(refuse(other)),
    }
}
