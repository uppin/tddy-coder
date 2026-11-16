//! `CreateSessionWatch`, `ListSessionWatches`, `RenewSessionWatch`, `CancelSessionWatch`: an
//! orchestrator watches the sessions it can see (`#activity-wake` 2/3).
//!
//! A watch selects sessions — by id, or every session the caller started — and kinds of event, and
//! lives for a required time to live of at most 24 hours. Every tool answer the orchestrator then
//! receives on this host carries, under `watchEvents`, what its watches matched since the last
//! answer; the daemon attaches it, not these tools.
//!
//! The watches themselves are `tddy_session_activity::session_watches`'. That crate sits above
//! this one, so the port speaks in JSON values and plain arguments.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-16-session-watches.md

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
pub const WATCH_KINDS: [&str; 6] = [
    "question",
    "permission",
    "turn_finished",
    "waiting_for_input",
    "agent_notification",
    "ended",
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

    /// `home`'s watch `watch_id` renewed for `ttl_seconds`, as JSON.
    async fn renew_watch(
        &self,
        home: &HomeCaller,
        watch_id: &str,
        ttl_seconds: u64,
    ) -> Result<Value, String> {
        let _ = (home, watch_id, ttl_seconds);
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
            "Watch sessions of this project you can see, on this host: from now on, every tool answer you receive carries `watchEvents` with what they did since your last answer — a question or a permission prompt (with the request_id AnswerSessionQuestion / AnswerSessionPermission take), a finished turn, a wait for input, an agent's notification, or a session ending — each event once. Name sessions by session_ids, or set started_by_me for every session you started, or both. Your own session is never watched. While you are idle with events waiting, you are sent a turn listing them. Answers the watch with its watch_id and expires_at_unix_ms.",
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
                        "description": "Which events; default all six."
                    },
                    "ttl_seconds": ttl.clone()
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
            "Renew one of your session watches: it now expires ttl_seconds from now (at most 86400).",
            json!({
                "type": "object",
                "properties": { "watch_id": watch_id.clone(), "ttl_seconds": ttl },
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
    // TODO(session-watches): implement — read the arguments, call the port, answer its JSON.
    let _ = (port, home, args_json);
    ToolOutcome::err(format!(
        "{tool_name}: session watches are not implemented yet"
    ))
}
