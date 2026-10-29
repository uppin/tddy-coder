//! `NotifyOperator`: a session with a standing role — the project-home main — notifies its operator
//! in its own words (`#home-inbox` 11/11).
//!
//! It is `tddy-tools notify` (`#home-inbox` 1/8) for an agent that has no shell to run the CLI in:
//! the same title, markdown body, level and sound, published as the same `AGENT` attention
//! notification, so Telegram and the desktop show it as they show the CLI's.
//!
//! Served to a session with a standing role only; a session without one notifies with the CLI. The
//! caller is the one the daemon's exec-tool path already authenticated and whose role it read from
//! the session's own record, so this tool takes no credential: the daemon publishes for the calling
//! session ([`ProjectHomeNotifyPort::notify_operator`]).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use tddy_service::proto::activity::NotificationLevel;
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::project_home_tools::tool_def;
use crate::ToolOutcome;

pub const NOTIFY_OPERATOR: &str = "NotifyOperator";

/// Whether `name` is [`NOTIFY_OPERATOR`].
pub fn is_project_home_notify_tool(name: &str) -> bool {
    name == NOTIFY_OPERATOR
}

/// [`NOTIFY_OPERATOR`]'s definition.
pub fn notify_operator_tool_def() -> ToolDef {
    tool_def(
        NOTIFY_OPERATOR,
        "Notify your operator, who is often not watching: when long work finishes, or something \
         blocks you and needs them while they are likely away. Give a one-line title (at most 200 \
         characters) and a markdown body_markdown (at most 2,000; empty for a title-only notice); \
         level is info, success, warning or error (info when omitted), and sound alerts them \
         audibly. It reaches them as an agent notification on every surface they watch (Telegram, \
         the desktop). A title or body over its limit is refused, saying why. Never use it for \
         routine progress: every notification interrupts a person.",
        json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "One line, 1 to 200 characters."
                },
                "body_markdown": {
                    "type": "string",
                    "description": "Markdown, at most 2,000 characters."
                },
                "level": {
                    "type": "string",
                    "enum": ["info", "success", "warning", "error"],
                    "description": "How the notice reads; info when omitted."
                },
                "sound": {
                    "type": "boolean",
                    "description": "Whether to alert the operator audibly; silent when omitted."
                }
            },
            "required": ["title", "body_markdown"]
        }),
    )
}

/// [`NOTIFY_OPERATOR`]'s arguments, as the agent writes them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NotifyOperatorArgs {
    pub title: String,
    pub body_markdown: String,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub sound: Option<bool>,
}

impl NotifyOperatorArgs {
    /// The notice these arguments ask for, or why it is refused: a title empty after trimming or
    /// over 200 characters, or a body over 2,000 — the limits `tddy-tools notify` is held to
    /// ([`tddy_daemon_kernel::agent_notification_limits`]), refused and never truncated — or a level
    /// other than `info`, `success`, `warning` or `error`. An omitted level is `info`, an omitted
    /// sound silent.
    pub fn into_notice(self) -> Result<OperatorNotice, String> {
        // TODO(sweep): implement — `validate_agent_notification`, then the level by name.
        Err(format!(
            "{NOTIFY_OPERATOR}: argument validation is not implemented yet ({:?})",
            self.title
        ))
    }
}

/// One notice for the calling session's operator, as the daemon publishes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorNotice {
    pub title: String,
    pub body_markdown: String,
    pub level: NotificationLevel,
    pub sound: bool,
}

/// What the tool needs from the daemon: publishing a notice for the calling session.
#[async_trait]
pub trait ProjectHomeNotifyPort: Send + Sync {
    /// Publish `notice` for `home`'s session as one `AGENT` attention notification. An error names
    /// why it was not published: an over-long title or body, or a session the daemon cannot
    /// publish for.
    ///
    /// The default refuses, so a port that publishes nothing need not name it.
    async fn notify_operator(
        &self,
        home: &HomeCaller,
        notice: OperatorNotice,
    ) -> Result<(), String> {
        let _ = (home, notice);
        Err("this host publishes no operator notifications".to_string())
    }
}

/// Run [`NOTIFY_OPERATOR`] for `home`.
///
/// Bad arguments and the port's refusal come back as an `is_error` outcome naming what was refused;
/// the role refusal is the daemon's and happens before this is called.
pub async fn execute_project_home_notify_tool(
    port: &dyn ProjectHomeNotifyPort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    // TODO(sweep): implement — parse NotifyOperatorArgs, map the level, call the port and answer
    // what was published.
    let _ = (port, home, args_json);
    ToolOutcome::err(format!("{tool_name}: not implemented yet"))
}
