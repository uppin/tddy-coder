//! `CreateSessionWatch`, `ListSessionWatches`, `RenewSessionWatch`, `CancelSessionWatch`: an
//! orchestrator watches the sessions it can see (`#activity-wake` 2/3).
//!
//! A watch selects sessions — by id, or every session the caller started — and kinds of event, and
//! lives for a required time to live of at most 24 hours. The orchestrator waits for what it
//! matched with `WaitSessionWatchEvents`; otherwise every tool answer it receives on this host
//! carries, under `watchEvents`, what its watches matched since the last answer — the daemon
//! attaches it, not these tools. Only a watch created (or renewed) with `wake_when_idle` makes the
//! daemon send its idle orchestrator a turn.
//!
//! The watches themselves are `tddy_session_activity::session_watches`'. That crate sits above
//! this one, so the port speaks in JSON values and plain arguments.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

use async_trait::async_trait;
use serde_json::{json, Value};
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, ProjectHomePort};
use crate::ToolOutcome;

pub const CREATE_SESSION_WATCH: &str = "CreateSessionWatch";
pub const LIST_SESSION_WATCHES: &str = "ListSessionWatches";
pub const RENEW_SESSION_WATCH: &str = "RenewSessionWatch";
pub const CANCEL_SESSION_WATCH: &str = "CancelSessionWatch";

/// The kinds a watch can ask for, as the tools take them — `tddy_session_activity`'s `WatchKind`
/// names, which a test there holds to this list.
pub const WATCH_KINDS: [&str; 8] = [
    "question",
    "permission",
    "turn_finished",
    "waiting_for_input",
    "agent_notification",
    "ended",
    // A message a watched session sent (`#activity-wake` 5/5).
    "message",
    // A commit on the project's remote trunk — the orchestrator's own activity, so the one kind a
    // watch of its own session asks for (`#orchestra` 5/9).
    "trunk_commit",
];

/// The longest `ttl_seconds` a watch is created or renewed with: 24 hours.
pub const MAX_WATCH_TTL_SECONDS: u64 = 86_400;

/// The refusal of a port that keeps no watches.
pub const NO_SESSION_WATCHES: &str = "this host keeps no session watches";

/// Whether `name` is one of the four watch tools.
pub fn is_project_home_watch_tool(name: &str) -> bool {
    matches!(
        name,
        CREATE_SESSION_WATCH | LIST_SESSION_WATCHES | RENEW_SESSION_WATCH | CANCEL_SESSION_WATCH
    )
}

/// What `CreateSessionWatch` asks for, once its arguments are read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchRequest {
    pub session_ids: Vec<String>,
    pub started_by_me: bool,
    /// Every kind when the call named none.
    pub kinds: Vec<String>,
    pub ttl_seconds: u64,
    /// Whether an idle orchestrator is sent a turn carrying what the watch matched; off unless the
    /// call sets it.
    pub wake_when_idle: bool,
}

/// What the tools need from the daemon: this host's watches, for the caller.
#[async_trait]
pub trait ProjectHomeWatchPort: ProjectHomePort {
    /// A new watch for `home`, answered as the watch's JSON. An error says what to do.
    async fn create_watch(
        &self,
        home: &HomeCaller,
        request: WatchRequest,
    ) -> Result<Value, String> {
        let _ = (home, request);
        Err(NO_SESSION_WATCHES.to_string())
    }

    /// `home`'s live watches, as JSON.
    async fn list_watches(&self, home: &HomeCaller) -> Result<Value, String> {
        let _ = home;
        Err(NO_SESSION_WATCHES.to_string())
    }

    /// `home`'s watch `watch_id` renewed for `ttl_seconds`, as JSON — its wake turned on or off when
    /// `wake_when_idle` is `Some`, kept as it was when `None`.
    async fn renew_watch(
        &self,
        home: &HomeCaller,
        watch_id: &str,
        ttl_seconds: u64,
        wake_when_idle: Option<bool>,
    ) -> Result<Value, String> {
        let _ = (home, watch_id, ttl_seconds, wake_when_idle);
        Err(NO_SESSION_WATCHES.to_string())
    }

    /// End `home`'s watch `watch_id`.
    async fn cancel_watch(&self, home: &HomeCaller, watch_id: &str) -> Result<Value, String> {
        let _ = (home, watch_id);
        Err(NO_SESSION_WATCHES.to_string())
    }
}

/// The four tools' definitions.
pub fn project_home_watch_tool_defs() -> Vec<ToolDef> {
    let ttl = json!({
        "type": "integer",
        "minimum": 1,
        "maximum": MAX_WATCH_TTL_SECONDS,
        "description": "How long the watch lives, in seconds: required, at most 86400 (24 hours). Renew it with RenewSessionWatch before it ends."
    });
    let watch_id = json!({
        "type": "string",
        "description": "The watch_id CreateSessionWatch or ListSessionWatches returned."
    });
    vec![
        tool_def(
            CREATE_SESSION_WATCH,
            "Watch sessions of this project you can see, on this host: a question or a permission prompt (with the request_id AnswerSessionQuestion / AnswerSessionPermission take), a finished turn, a wait for input, an agent's notification, or a session ending — each event delivered once. Wait for them with WaitSessionWatchEvents (or `tddy-tools monitor` under Monitor); otherwise they ride on your next tool answer as `watchEvents`. Name sessions by session_ids, or set started_by_me for every session you started, or both. Your own session is never watched, except for kind trunk_commit: a new commit on the project's remote trunk (or a force-push rewriting it), which every live orchestrator of the project receives as its own activity. A watch does not interrupt you: only with wake_when_idle is a turn listing its events typed into your conversation while you are idle — set it only when you must be woken and will not be waiting. Answers the watch with its watch_id, wake_when_idle and expires_at_unix_ms.",
            json!({
                "type": "object",
                "properties": {
                    "session_ids": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Sessions of this project to watch, by id (ListProjectSessions)."
                    },
                    "started_by_me": {
                        "type": "boolean",
                        "description": "Watch every session you started (whose orchestrator is you), including ones you start later."
                    },
                    "kinds": {
                        "type": "array",
                        "items": { "type": "string", "enum": WATCH_KINDS },
                        "description": "Which events; default every kind."
                    },
                    "ttl_seconds": ttl.clone(),
                    "wake_when_idle": {
                        "type": "boolean",
                        "default": false,
                        "description": "Type a turn listing this watch's events into your conversation while you are idle. Default false: events wait for WaitSessionWatchEvents or your next tool answer. Injected turns interleave with what the operator types, so set it only when you must be woken."
                    }
                },
                "required": ["ttl_seconds"],
                "additionalProperties": false
            }),
        ),
        tool_def(
            LIST_SESSION_WATCHES,
            "Your live session watches on this host: each one's watch_id, what it watches, when it expires, and how many matched events wait for your next answer.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        tool_def(
            RENEW_SESSION_WATCH,
            "Renew one of your session watches: it now expires ttl_seconds from now (at most 86400). wake_when_idle turns its idle wake on or off; omit it to keep the watch's choice.",
            json!({
                "type": "object",
                "properties": {
                    "watch_id": watch_id.clone(),
                    "ttl_seconds": ttl,
                    "wake_when_idle": {
                        "type": "boolean",
                        "description": "Turn the idle wake on (true) or off (false); omit to keep it as it is."
                    }
                },
                "required": ["watch_id", "ttl_seconds"],
                "additionalProperties": false
            }),
        ),
        tool_def(
            CANCEL_SESSION_WATCH,
            "End one of your session watches: nothing more is delivered for it.",
            json!({
                "type": "object",
                "properties": { "watch_id": watch_id },
                "required": ["watch_id"],
                "additionalProperties": false
            }),
        ),
    ]
}

/// Run one of the four tools for `home`. Bad arguments and the port's refusals come back as an
/// `is_error` outcome saying what to do; the role refusal is the daemon's and happens first.
pub async fn execute_project_home_watch_tool(
    port: &dyn ProjectHomeWatchPort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    match answer(port, home, tool_name, args_json).await {
        Ok(value) => ToolOutcome::ok(value.to_string()),
        Err(message) => ToolOutcome::err(format!("{tool_name}: {message}")),
    }
}

/// The JSON `tool_name` answers for `args_json`, from the port.
async fn answer(
    port: &dyn ProjectHomeWatchPort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> Result<Value, String> {
    if !is_project_home_watch_tool(tool_name) {
        return Err(format!("unknown session watch tool: {tool_name}"));
    }
    let args: Value =
        serde_json::from_str(args_json).map_err(|e| format!("invalid args_json: {e}"))?;
    match tool_name {
        CREATE_SESSION_WATCH => port.create_watch(home, create_request(&args)?).await,
        LIST_SESSION_WATCHES => port.list_watches(home).await,
        RENEW_SESSION_WATCH => {
            port.renew_watch(
                home,
                &watch_id_argument(&args)?,
                ttl_argument(&args)?,
                wake_argument(&args)?,
            )
            .await
        }
        _ => port.cancel_watch(home, &watch_id_argument(&args)?).await,
    }
}

/// What `CreateSessionWatch` asks for: `ttl_seconds` required and in bounds, a session selected,
/// every kind known (all of them when none is named).
fn create_request(args: &Value) -> Result<WatchRequest, String> {
    let ttl_seconds = ttl_argument(args)?;
    let session_ids = match args.get("session_ids") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(ids)) => ids
            .iter()
            .map(|id| {
                id.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("session_ids must name sessions, not {id}"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(other) => return Err(format!("session_ids must name sessions, not {other}")),
    };
    let started_by_me = match args.get("started_by_me") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(other) => return Err(format!("started_by_me must be true or false, not {other}")),
    };
    if session_ids.is_empty() && !started_by_me {
        return Err(
            "name session_ids or started_by_me: a watch must select at least one session"
                .to_string(),
        );
    }
    let kinds = match args.get("kinds") {
        None | Some(Value::Null) => WATCH_KINDS.iter().map(|k| k.to_string()).collect(),
        Some(Value::Array(kinds)) => {
            let mut named = Vec::new();
            for kind in kinds {
                let Some(name) = kind.as_str().filter(|n| WATCH_KINDS.contains(n)) else {
                    return Err(format!(
                        "unknown kind {kind}: kinds are {}",
                        WATCH_KINDS.join(", ")
                    ));
                };
                named.push(name.to_string());
            }
            named
        }
        Some(other) => return Err(format!("kinds must name kinds, not {other}")),
    };
    Ok(WatchRequest {
        session_ids,
        started_by_me,
        kinds,
        ttl_seconds,
        wake_when_idle: wake_argument(args)?.unwrap_or(false),
    })
}

/// `ttl_seconds`: required, a whole number of seconds between 1 and [`MAX_WATCH_TTL_SECONDS`].
fn ttl_argument(args: &Value) -> Result<u64, String> {
    let Some(ttl) = args.get("ttl_seconds").and_then(Value::as_u64) else {
        return Err("ttl_seconds is required: how long the watch lives, in seconds".to_string());
    };
    if ttl == 0 {
        return Err(
            "ttl_seconds must be at least 1: how long the watch lives, in seconds".to_string(),
        );
    }
    if ttl > MAX_WATCH_TTL_SECONDS {
        return Err(format!(
            "ttl_seconds must be at most {MAX_WATCH_TTL_SECONDS} (24 hours), not {ttl}"
        ));
    }
    Ok(ttl)
}

/// `wake_when_idle`: `Some` when the call names it, `None` when it does not.
fn wake_argument(args: &Value) -> Result<Option<bool>, String> {
    match args.get("wake_when_idle") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(wake)) => Ok(Some(*wake)),
        Some(other) => Err(format!("wake_when_idle must be true or false, not {other}")),
    }
}

/// `watch_id`: the watch the call names, as `CreateSessionWatch` returned it.
fn watch_id_argument(args: &Value) -> Result<String, String> {
    match args.get("watch_id").and_then(Value::as_str) {
        Some(watch_id) if !watch_id.is_empty() => Ok(watch_id.to_string()),
        _ => Err(
            "watch_id is required: the watch_id CreateSessionWatch or ListSessionWatches returned"
                .to_string(),
        ),
    }
}
