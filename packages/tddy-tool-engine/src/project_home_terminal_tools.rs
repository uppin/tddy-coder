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
//! `ClaimSessionTerminal` and `SendSessionKeys` are the other two tools of this module
//! (`#home-inbox` 5/8): the agent claims the target's terminal control lease, then types into it for as
//! long as it holds that lease. Both are target tools too, and both run on the host that owns the
//! terminal, which is where the lease lives.
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
use crate::terminal_keys::encode_keys;
use crate::ToolOutcome;

pub const READ_SESSION_TERMINAL: &str = "ReadSessionTerminal";
pub const CLAIM_SESSION_TERMINAL: &str = "ClaimSessionTerminal";
pub const SEND_SESSION_KEYS: &str = "SendSessionKeys";

/// The screen id a project-home session holds a terminal's control lease under.
///
/// Distinct per home session, so a browser's "who has control" names the agent that took it and two
/// homes never share a lease.
fn project_home_screen_id(home: &HomeCaller) -> String {
    format!("project-home:{}", home.session_id)
}

/// Whether `name` is one of the terminal tools: [`READ_SESSION_TERMINAL`],
/// [`CLAIM_SESSION_TERMINAL`] or [`SEND_SESSION_KEYS`].
pub fn is_project_home_terminal_tool(name: &str) -> bool {
    matches!(
        name,
        READ_SESSION_TERMINAL | CLAIM_SESSION_TERMINAL | SEND_SESSION_KEYS
    )
}

/// The tools' definitions.
pub fn project_home_terminal_tool_defs() -> Vec<ToolDef> {
    let mut defs = vec![tool_def(
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
    )];
    defs.extend(terminal_keys_tool_defs());
    defs
}

fn terminal_keys_tool_defs() -> Vec<ToolDef> {
    vec![
        tool_def(
            CLAIM_SESSION_TERMINAL,
            "Take control of a session's main terminal so SendSessionKeys can type into it. Taking control takes it from whoever holds it: a person watching the session in the browser drops to watch-only and sees that you took control, and they take it back by claiming it themselves. Claim only when you mean to act, and never to fight a person for control. Control stays yours until a person takes it; there is no release. Answers holder, the screen id you now hold it under.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "The session of this project whose terminal to take control of, as ListProjectSessions reports it."
                    },
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["session_id"]
            }),
        ),
        tool_def(
            SEND_SESSION_KEYS,
            "Type into a session's main terminal while you hold its control (ClaimSessionTerminal). Refused, naming who holds control, when you do not, and nothing is sent: claim it again. `text` is typed first, as a bracketed paste when the terminal has enabled that (text holding the paste-end sequence ESC[201~ is then refused); `keys` follow in order. Key names: Enter, Escape, Tab, BTab, Backspace, Delete, Up, Down, Left, Right, Home, End, PageUp, PageDown, Space, C-a to C-z (control keys, C-c interrupts), F1 to F12. An unknown key name is refused before anything is sent. Read the result with ReadSessionTerminal.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "The session of this project to type into, as ListProjectSessions reports it."
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type first. Does not press Enter: name Enter in keys."
                    },
                    "keys": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Named keys to press, in order, after the text."
                    },
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["session_id"]
            }),
        ),
    ]
}

/// What the tool needs from the daemon beyond the target resolver: the screen of a session's main
/// terminal on this host.
#[async_trait]
pub trait ProjectHomeTerminalPort: ProjectHomePort {
    /// The visible text of `target`'s main terminal. An error names why there is none: the session
    /// has no live terminal, or the terminal's size is not yet known.
    async fn read_terminal(&self, target: &SessionTarget) -> anyhow::Result<TextGrid>;

    /// Take the control lease of `target`'s terminals for `screen_id`, stealing it from whoever
    /// holds it, and keep the issued token on this host on behalf of the calling home session. The
    /// token is never returned. An error names why it could not be taken.
    ///
    /// `#home-inbox` 5/8.
    async fn claim_terminal(&self, target: &SessionTarget, screen_id: &str) -> Result<(), String>;

    /// Write `bytes` to `target`'s main terminal, provided `screen_id`'s token is the session's
    /// current lease. The lease is verified explicitly, also for a jailed terminal whose own input
    /// path does not check it. An error names why nothing was written, and the current holder when
    /// that is why.
    ///
    /// `#home-inbox` 5/8.
    async fn send_keys(
        &self,
        target: &SessionTarget,
        screen_id: &str,
        bytes: Vec<u8>,
    ) -> Result<(), String>;
}

#[derive(Deserialize)]
struct SessionArgs {
    session_id: String,
}

/// The arguments of `SendSessionKeys`: the text typed first, then the named keys. Either may be
/// absent, not both.
#[derive(Deserialize)]
struct SendKeysArgs {
    session_id: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    keys: Vec<String>,
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
    if tool_name != READ_SESSION_TERMINAL {
        return execute_terminal_keys_tool(port, home, tool_name, args).await;
    }
    let args: SessionArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutcome::err(format!("{READ_SESSION_TERMINAL}: invalid arguments: {e}"))
        }
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    match port.read_terminal(&target).await {
        Ok(grid) => ToolOutcome::ok(grid_answer(&grid).to_string()),
        Err(e) => ToolOutcome::err(format!("{READ_SESSION_TERMINAL}: {e:#}")),
    }
}

/// The answer [`READ_SESSION_TERMINAL`] gives for `grid`. `bracketed_paste` is left out: it is the
/// input-mode state the keys tool reads, not part of what the screen shows.
fn grid_answer(grid: &TextGrid) -> Value {
    json!({
        "lines": grid.lines,
        "cursor": {
            "row": grid.cursor.row,
            "col": grid.cursor.col,
            "visible": grid.cursor.visible,
        },
        "cols": grid.cols,
        "rows": grid.rows,
        "offset": grid.offset,
    })
}

/// Run [`CLAIM_SESSION_TERMINAL`] or [`SEND_SESSION_KEYS`] for `home`: resolve the target, then take
/// or use the lease under [`project_home_screen_id`]. `args` is the call's parsed argument object.
async fn execute_terminal_keys_tool(
    port: &dyn ProjectHomeTerminalPort,
    home: &HomeCaller,
    tool_name: &str,
    args: Value,
) -> ToolOutcome {
    if tool_name == CLAIM_SESSION_TERMINAL {
        return claim_session_terminal(port, home, args).await;
    }
    send_session_keys(port, home, args).await
}

/// `ClaimSessionTerminal`: take the target's control lease for this home session. Answers
/// `{holder}`, the screen id the lease is now held under. The token stays on the host.
async fn claim_session_terminal(
    port: &dyn ProjectHomeTerminalPort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: SessionArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutcome::err(format!("{CLAIM_SESSION_TERMINAL}: invalid arguments: {e}"))
        }
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    let screen_id = project_home_screen_id(home);
    match port.claim_terminal(&target, &screen_id).await {
        Ok(()) => ToolOutcome::ok(json!({ "holder": screen_id }).to_string()),
        Err(e) => ToolOutcome::err(format!("{CLAIM_SESSION_TERMINAL}: {e}")),
    }
}

/// `SendSessionKeys`: encode the text and keys, then write them under this home session's lease.
/// The text is encoded as a paste only when the target's terminal has enabled bracketed paste, so
/// that is read first, and only when there is text to type. Every refusal leaves nothing typed.
/// Answers `{sent_bytes}`, the number of bytes written.
async fn send_session_keys(
    port: &dyn ProjectHomeTerminalPort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: SendKeysArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolOutcome::err(format!("{SEND_SESSION_KEYS}: invalid arguments: {e}")),
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    let typing_text = args.text.as_deref().filter(|text| !text.is_empty());
    let bracketed_paste = match typing_text {
        None => false,
        Some(_) => match port.read_terminal(&target).await {
            Ok(grid) => grid.bracketed_paste,
            Err(e) => {
                return ToolOutcome::err(format!(
                    "{SEND_SESSION_KEYS}: cannot tell whether the terminal takes a paste, so \
                     nothing was sent: {e:#}"
                ))
            }
        },
    };
    let bytes = match encode_keys(typing_text, &args.keys, bracketed_paste) {
        Ok(bytes) => bytes,
        Err(e) => return ToolOutcome::err(format!("{SEND_SESSION_KEYS}: {e}; nothing was sent")),
    };
    let sent_bytes = bytes.len();
    match port
        .send_keys(&target, &project_home_screen_id(home), bytes)
        .await
    {
        Ok(()) => ToolOutcome::ok(json!({ "sent_bytes": sent_bytes }).to_string()),
        Err(e) => ToolOutcome::err(format!("{SEND_SESSION_KEYS}: {e}; nothing was sent")),
    }
}
