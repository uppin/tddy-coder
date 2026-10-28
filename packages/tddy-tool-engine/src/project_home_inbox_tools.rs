//! `WaitProjectNotifications` and `ReadProjectNotifications`: the project-home agent learns what
//! happened in its project without polling (`#home-inbox` 7/8).
//!
//! Both tools read the host's **project notification log** — every attention notification of the
//! project's sessions, appended by a bus subscriber and addressed by a cursor — and answer in
//! `#start-parity` 6/9's paging shape ([`crate::session_follow::FollowPage`]): `items`,
//! `next_cursor`, `has_more` and `clamped_to`, plus `lost` for a read and `timed_out` for a wait.
//!
//! - **Wait** returns at once when records newer than `cursor` exist, otherwise holds until one
//!   arrives or `timeout_ms` passes. The timeout is capped at [`MAX_WAIT_MS`], the remote-call
//!   clamp, so one call stays under every transport's deadline; the agent re-waits with the cursor.
//! - **Read** returns up to `limit` records after `cursor`, oldest first, and says how many records
//!   an evicted cursor lost.
//!
//! Neither returns the caller's own session's notifications. Both are target tools in the sense of
//! [`TARGET_DAEMON_INSTANCE_ID`]: the daemon forwards a call naming another host to that host
//! whole (`#start-parity` 1/9), so the port below is only ever asked about this host's log. There
//! is no merged cross-host view: the agent waits on each host it has children on.
//!
//! The log itself is `tddy_session_activity::project_notification_log`'s. That crate sits above
//! this one, so the port speaks in JSON values: the daemon serialises the log's records into
//! [`NotificationsPage::items`].
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::project_home_tools::{tool_def, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID};
use crate::session_follow::FollowPage;
use crate::ToolOutcome;

pub const WAIT_PROJECT_NOTIFICATIONS: &str = "WaitProjectNotifications";
pub const READ_PROJECT_NOTIFICATIONS: &str = "ReadProjectNotifications";

/// The longest a wait is held, whatever `timeout_ms` asks: the remote-call clamp
/// (`tddy_session_tool_client::MAX_REMOTE_BLOCK_MS`), which this crate does not depend on.
pub const MAX_WAIT_MS: u64 = 20_000;

/// Whether `name` is [`WAIT_PROJECT_NOTIFICATIONS`] or [`READ_PROJECT_NOTIFICATIONS`].
pub fn is_project_home_inbox_tool(name: &str) -> bool {
    matches!(
        name,
        WAIT_PROJECT_NOTIFICATIONS | READ_PROJECT_NOTIFICATIONS
    )
}

/// The tools' definitions.
pub fn project_home_inbox_tool_defs() -> Vec<ToolDef> {
    let host = json!({
        "type": "string",
        "description": "The host whose notification log to use, as ListProjectSessions reports it in its daemon_instance_id; empty for this host. There is no merged view across hosts: wait on each host you have sessions on."
    });
    let cursor = json!({
        "type": "string",
        "description": "The next_cursor an earlier answer returned; only what came after it is returned. Never invent one."
    });
    vec![
        tool_def(
            WAIT_PROJECT_NOTIFICATIONS,
            "Wait for this project's sessions to need you: a session finished, is waiting for input, asks a question or a permission (its request_id and content are in the record, for AnswerSessionPermission and AnswerSessionQuestion), or an agent wrote you a notification. Returns at once when records newer than cursor exist, otherwise holds until one arrives or timeout_ms passes (at most 20000). Without a cursor it waits for the next new record. Answers items, next_cursor, has_more and timed_out; pass next_cursor back as the cursor. A timeout means nothing happened: do not wait in a loop with nothing to decide, and read ListProjectSessions before composing a status. Your own session's notifications are never returned.",
            json!({
                "type": "object",
                "properties": {
                    "cursor": cursor.clone(),
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": MAX_WAIT_MS,
                        "description": "How long to wait, in milliseconds. Capped at 20000."
                    },
                    TARGET_DAEMON_INSTANCE_ID: host.clone()
                },
                "additionalProperties": false
            }),
        ),
        tool_def(
            READ_PROJECT_NOTIFICATIONS,
            "Read this project's notification log without waiting: up to limit records after cursor, oldest first. Without a cursor it returns the newest records. Answers items, next_cursor, has_more, clamped_to when limit was above 200, and lost when cursor is older than the log's window (the log keeps the newest 1000 records per project): lost is how many records were evicted before you read them, never silently trimmed. Your own session's notifications are never returned.",
            json!({
                "type": "object",
                "properties": {
                    "cursor": cursor,
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": crate::session_follow::FOLLOW_PAGE_MAX,
                        "description": "Records per page. Default 50, at most 200: a larger value is clamped and the answer carries clamped_to."
                    },
                    TARGET_DAEMON_INSTANCE_ID: host
                },
                "additionalProperties": false
            }),
        ),
    ]
}

/// One page of either tool's answer: the paging of `#start-parity` 6/9 over the log's records.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotificationsPage {
    /// The log's records, as JSON, oldest first.
    #[serde(flatten)]
    pub page: FollowPage<Value>,
    /// A read's count of records evicted between its cursor and the oldest kept; `None` when the
    /// cursor is within the window (and always for a wait).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lost: Option<u64>,
    /// A wait's flag: it returned because `timeout_ms` passed, not because a record arrived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timed_out: Option<bool>,
}

/// What the tools need from the daemon beyond the target resolver: this host's project
/// notification log, scoped to the caller.
#[async_trait]
pub trait ProjectHomeInboxPort: ProjectHomePort {
    /// The records of `home`'s project on this host after `cursor`, for the caller's OS user and
    /// excluding `home`'s own session: at most `limit` of them (the default and maximum are the
    /// follow tools'), with `lost` set when `cursor` was evicted. An error names why it could not be
    /// read, among them a `cursor` that no page ever returned.
    ///
    /// The default refuses, so a port that keeps no log (and the fakes of the other home-inbox
    /// nodes' tests) need not name it.
    async fn read_notifications(
        &self,
        home: &HomeCaller,
        cursor: &str,
        limit: Option<usize>,
    ) -> Result<NotificationsPage, String> {
        // TODO(home-wait): implement — `ProjectNotificationLog::read` for `home.project_id`, the
        // caller's OS user and `home.session_id` as the excluded session, records as JSON.
        let _ = (home, cursor, limit);
        Err("read_notifications: not implemented".to_string())
    }

    /// As [`Self::read_notifications`], but holding the call until a record newer than `cursor`
    /// arrives or `timeout_ms` passes. `timeout_ms` is already capped at [`MAX_WAIT_MS`]; an empty
    /// `cursor` waits for the next new record.
    async fn wait_notifications(
        &self,
        home: &HomeCaller,
        cursor: &str,
        timeout_ms: u64,
    ) -> Result<NotificationsPage, String> {
        // TODO(home-wait): implement — `ProjectNotificationLog::wait`, as above.
        let _ = (home, cursor, timeout_ms);
        Err("wait_notifications: not implemented".to_string())
    }
}

/// Run [`WAIT_PROJECT_NOTIFICATIONS`] or [`READ_PROJECT_NOTIFICATIONS`] for `home`.
///
/// Bad arguments and a refusal of the port come back as an `is_error` outcome naming what was
/// refused; the role refusal is the daemon's and happens before this is called.
pub async fn execute_project_home_inbox_tool(
    port: &dyn ProjectHomeInboxPort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    // TODO(home-wait): implement — parse `cursor`, `timeout_ms` (capped at `MAX_WAIT_MS`) or
    // `limit`, call the port, and answer the page as JSON.
    let _ = (port, home, args_json);
    ToolOutcome::err(format!("{tool_name}: not implemented"))
}
