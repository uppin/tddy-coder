//! `ReadSessionTerminal`: what another session's TUI is showing right now, as text, for the
//! project-home agent (`#home-inbox` 2/8).
//!
//! The answer is the target's **main** terminal's screen model as the daemon holds it
//! ([`tddy_task::TerminalCapture::text_grid`]) — the active buffer's visible rows, the cursor, the
//! grid size and the stream offset — the same screen a reconnecting browser is repainted with.
//! Reading writes nothing and takes no control lease.
//!
//! A target tool: it takes [`TARGET_DAEMON_INSTANCE_ID`](crate::project_home_tools::TARGET_DAEMON_INSTANCE_ID)
//! to name the host its session runs on, and the daemon forwards such a call to that host whole
//! (`#start-parity` 1/9), so the port below is only ever asked about this host's terminals.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tddy_task::TextGrid;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget};

use crate::catalog::ToolDef;
use crate::project_home_tools::{
    resolve_target, target_host_property, tool_def, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID,
};
use crate::ToolOutcome;

pub const READ_SESSION_TERMINAL: &str = "ReadSessionTerminal";

/// Whether `name` is [`READ_SESSION_TERMINAL`].
pub fn is_project_home_terminal_tool(name: &str) -> bool {
    name == READ_SESSION_TERMINAL
}

/// The tool's definition.
pub fn project_home_terminal_tool_defs() -> Vec<ToolDef> {
    vec![tool_def(
        READ_SESSION_TERMINAL,
        "Read what a session of this project shows on its main terminal right now: the visible rows of the screen it is on (the alternate screen while a full-screen TUI runs), each with trailing whitespace trimmed, and the cursor's row, column and visibility, zero-based. Answers lines, cursor {row, col, visible}, cols, rows and offset (the position in the terminal's output stream the text corresponds to). Text only: no scrollback, colours or attributes. Reading sends no input and disturbs no attached screen. A session with no live terminal, or whose terminal has not reported its size, is refused with the reason.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session of this project to read, as ListProjectSessions reports it."
                },
                TARGET_DAEMON_INSTANCE_ID: target_host_property()
            },
            "required": ["session_id"]
        }),
    )]
}

/// What the tool needs from the daemon beyond the target resolver: the screen of a session's main
/// terminal on this host.
#[async_trait]
pub trait ProjectHomeTerminalPort: ProjectHomePort {
    /// The visible text of `target`'s main terminal. An error names why there is none: the session
    /// has no live terminal, or the terminal's size is not yet known.
    async fn read_terminal(&self, target: &SessionTarget) -> anyhow::Result<TextGrid>;
}

#[derive(Deserialize)]
struct ReadTerminalArgs {
    session_id: String,
}

/// Run [`READ_SESSION_TERMINAL`] for `home`.
///
/// A target refusal and a failure of the port come back as an `is_error` outcome naming what was
/// refused; the role refusal is the daemon's and happens before this is called.
pub async fn execute_project_home_terminal_tool(
    port: &dyn ProjectHomeTerminalPort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    if !is_project_home_terminal_tool(tool_name) {
        return ToolOutcome::err(format!("unknown project-home terminal tool: {tool_name}"));
    }
    let args: Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::err(format!("invalid args_json: {e}")),
    };
    let args: ReadTerminalArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutcome::err(format!("{READ_SESSION_TERMINAL}: invalid arguments: {e}"))
        }
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    // TODO(terminal-read): implement — read `port.read_terminal(&target)` and answer
    // `{lines, cursor: {row, col, visible}, cols, rows, offset}`.
    ToolOutcome::err(format!(
        "{READ_SESSION_TERMINAL}: not implemented (session {})",
        target.session_id
    ))
}
