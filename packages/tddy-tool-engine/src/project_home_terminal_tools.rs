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
use tddy_task::{crop, highlighted_rows, GridRegion, RowStyles, StyleRun, TextGrid};
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget};

use crate::catalog::ToolDef;
use crate::project_home_tools::resolve_target;
use crate::project_home_tools::{
    target_host_property, tool_def, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID,
};
use crate::terminal_keys::{encode_typing, type_into, EchoWait, TypingTerminal};
use crate::ToolOutcome;

pub const READ_SESSION_TERMINAL: &str = "ReadSessionTerminal";
pub const CLAIM_SESSION_TERMINAL: &str = "ClaimSessionTerminal";
pub const SEND_SESSION_KEYS: &str = "SendSessionKeys";
/// Give back a terminal this session claimed (`#orchestra` 7/9).
pub const RELEASE_SESSION_TERMINAL: &str = "ReleaseSessionTerminal";

/// [`RELEASE_SESSION_TERMINAL`]'s definition.
pub fn release_session_terminal_tool_def() -> ToolDef {
    tool_def(
        RELEASE_SESSION_TERMINAL,
        "Give back control of a session's main terminal that you took with ClaimSessionTerminal, so a person watching the session can take control without stealing it. Releasing is for your own claim only: a terminal you never claimed, or whose control a person has taken since, is refused naming why, and nothing changes. Answers released: true. After a release SendSessionKeys is refused until you claim again.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session of this project whose terminal to give back, as ListProjectSessions reports it."
                },
                TARGET_DAEMON_INSTANCE_ID: target_host_property()
            },
            "required": ["session_id"]
        }),
    )
}

/// The screen id a project-home session holds a terminal's control lease under.
///
/// Distinct per home session, so a browser's "who has control" names the agent that took it and two
/// homes never share a lease.
fn project_home_screen_id(home: &HomeCaller) -> String {
    format!("project-home:{}", home.session_id)
}

/// Whether `name` is one of the terminal tools: [`READ_SESSION_TERMINAL`],
/// [`CLAIM_SESSION_TERMINAL`], [`SEND_SESSION_KEYS`] or [`RELEASE_SESSION_TERMINAL`].
pub fn is_project_home_terminal_tool(name: &str) -> bool {
    matches!(
        name,
        READ_SESSION_TERMINAL
            | CLAIM_SESSION_TERMINAL
            | SEND_SESSION_KEYS
            | RELEASE_SESSION_TERMINAL
    )
}

/// The tools' definitions.
pub fn project_home_terminal_tool_defs() -> Vec<ToolDef> {
    let mut defs = vec![tool_def(
        READ_SESSION_TERMINAL,
        "Read what a session of this project shows on its main terminal right now: the visible rows of the screen it is on (the alternate screen while a full-screen TUI runs), each with trailing whitespace trimmed, and the cursor's row, column and visibility, zero-based. Answers lines, cursor {row, col, visible}, cols, rows and offset (the position in the terminal's output stream the text corresponds to). Text only, with no scrollback, unless you pass styles: true, which adds colours and attributes: `styles`, per row that has any, the runs {from, to, fg, bg, bold, dim, italic, underline, inverse, strikethrough, blink} whose style differs from the terminal default (only the set attributes are present; from is inclusive and to exclusive, as character positions in that row's line, and a run may reach past the trimmed text into a bar's blank cells), and `highlighted_rows`, the rows painted as a bar, where the runs covering all the row's text are contiguous and all inverse or all one background colour. Pass styles: true when you must know what is selected before you type: many TUIs mark the current item only by inverse video or a background colour. Highlighted rows are usually the selection but may be a status bar, so check the text. Pass region {row, col, rows, cols} (the top-left cell, zero-based, and a size, in screen cells) to read only one pane, menu or dialog: lines, styles and the cursor then describe that box, the cursor relative to it with inside telling whether it is in the box, and the answer echoes the region read, clipped to the screen (a box with no size, or off the screen, is refused); cols and rows stay the full screen's size. Reading sends no input and disturbs no attached screen. A session with no live terminal, or whose terminal has not reported its size, is refused with the reason.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session of this project to read, as ListProjectSessions reports it."
                },
                "region": {
                    "type": "object",
                    "description": "Read only this box of the screen, in screen cells (the units of cursor, cols and rows).",
                    "properties": {
                        "row": { "type": "integer", "minimum": 0, "description": "Top row of the box, zero-based." },
                        "col": { "type": "integer", "minimum": 0, "description": "Left column of the box, zero-based." },
                        "rows": { "type": "integer", "minimum": 1, "description": "Height of the box." },
                        "cols": { "type": "integer", "minimum": 1, "description": "Width of the box." }
                    },
                    "required": ["row", "col", "rows", "cols"]
                },
                "styles": {
                    "type": "boolean",
                    "description": "Also answer the screen's colours and attributes as `styles` and `highlighted_rows`. Default false: text only."
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
            "Take control of a session's main terminal so SendSessionKeys can type into it. Taking control takes it from whoever holds it: a person watching the session in the browser drops to watch-only and sees that you took control, and they take it back by claiming it themselves. Claim only when you mean to act, and never to fight a person for control. Control stays yours until a person takes it or you give it back with ReleaseSessionTerminal. Answers holder, the screen id you now hold it under.",
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
            "Type into a session's main terminal while you hold its control (ClaimSessionTerminal). Refused, naming who holds control, when you do not, and nothing is sent: claim it again. `text` is typed first, as a bracketed paste when the terminal has enabled that (text holding the paste-end sequence ESC[201~ is then refused); `keys` follow in order, pressed once the application has echoed the text (it may otherwise read them as part of the paste), or after 5 seconds with no echo. Answers sent_bytes and, when keys followed text, echo_seen: false means no echo came and the keys were pressed anyway. Key names: Enter, Escape, Tab, BTab, Backspace, Delete, Up, Down, Left, Right, Home, End, PageUp, PageDown, Space, C-a to C-z (control keys, C-c interrupts), F1 to F12. An unknown key name is refused before anything is sent. Read the result with ReadSessionTerminal.",
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
        release_session_terminal_tool_def(),
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

    /// Give back `target`'s terminal lease that `screen_id` holds, forgetting its token. An error
    /// names why nothing was released: this session never claimed it, or another screen holds it
    /// now (`#orchestra` 7/9).
    ///
    /// The default refuses, so a port that holds no claims need not name it.
    async fn release_terminal(
        &self,
        target: &SessionTarget,
        screen_id: &str,
    ) -> Result<(), String> {
        let _ = (target, screen_id);
        Err("this host holds no terminal claims".to_string())
    }
}

#[derive(Deserialize)]
struct SessionArgs {
    session_id: String,
}

/// The arguments of `ReadSessionTerminal`: the session, and whether to answer its styles too.
#[derive(Deserialize)]
struct ReadArgs {
    session_id: String,
    #[serde(default)]
    styles: bool,
    #[serde(default)]
    region: Option<RegionArg>,
}

/// The `region` argument: a box of screen cells.
#[derive(Deserialize)]
struct RegionArg {
    row: u16,
    col: u16,
    rows: u16,
    cols: u16,
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
    let args: ReadArgs = match serde_json::from_value(args) {
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
        Ok(grid) => match terminal_grid_answer(
            &grid,
            args.styles,
            args.region.as_ref().map(|region| GridRegion {
                row: region.row,
                col: region.col,
                rows: region.rows,
                cols: region.cols,
            }),
        ) {
            Ok(answer) => ToolOutcome::ok(answer.to_string()),
            Err(refusal) => ToolOutcome::err(format!("{READ_SESSION_TERMINAL}: {refusal}")),
        },
        Err(e) => ToolOutcome::err(format!("{READ_SESSION_TERMINAL}: {e:#}")),
    }
}

/// The answer [`READ_SESSION_TERMINAL`] gives for `grid` — public so another driver of a terminal
/// (`tddy-session-driver`) answers exactly what this tool does. `bracketed_paste` is left out: it is the
/// input-mode state the keys tool reads, not part of what the screen shows.
///
/// `styles` and `highlighted_rows` are added only when `with_styles` is set, so an answer without
/// the argument is exactly the text-only answer of `#home-inbox` 2/8. With a `region`, `lines`,
/// `cursor` and the style fields are those of the cropped box, and the answer echoes the region
/// read; `cols`, `rows` and `offset` stay the whole screen's. A box the screen cannot give is the
/// `Err`, naming why.
pub fn terminal_grid_answer(
    grid: &TextGrid,
    with_styles: bool,
    region: Option<GridRegion>,
) -> Result<Value, String> {
    let Some(region) = region else {
        let mut answer = json!({
            "lines": grid.lines,
            "cursor": {
                "row": grid.cursor.row,
                "col": grid.cursor.col,
                "visible": grid.cursor.visible,
            },
            "cols": grid.cols,
            "rows": grid.rows,
            "offset": grid.offset,
        });
        if with_styles {
            answer["styles"] = grid.styles.iter().map(row_styles_answer).collect();
            answer["highlighted_rows"] = json!(highlighted_rows(&grid.styles, &grid.lines));
        }
        return Ok(answer);
    };
    let cropped = crop(grid, region)?;
    let mut answer = json!({
        "lines": cropped.lines,
        "cursor": {
            "row": cropped.cursor.row,
            "col": cropped.cursor.col,
            "visible": cropped.cursor.visible,
            "inside": cropped.cursor.inside,
        },
        "cols": grid.cols,
        "rows": grid.rows,
        "offset": grid.offset,
        "region": {
            "row": cropped.region.row,
            "col": cropped.region.col,
            "rows": cropped.region.rows,
            "cols": cropped.region.cols,
        },
    });
    if with_styles {
        answer["styles"] = cropped.styles.iter().map(row_styles_answer).collect();
        answer["highlighted_rows"] = json!(cropped.highlighted_rows);
    }
    Ok(answer)
}

/// One row's runs as the agent reads them: `{row, runs: [...]}`.
fn row_styles_answer(row: &RowStyles) -> Value {
    json!({ "row": row.row, "runs": row.runs.iter().map(run_answer).collect::<Vec<_>>() })
}

/// One run as `{from, to}` plus only the attributes that are set: `fg` and `bg` as the colour's
/// name, each flag as `true`.
fn run_answer(run: &StyleRun) -> Value {
    let mut answer = json!({ "from": run.from, "to": run.to });
    if let Some(fg) = run.fg {
        answer["fg"] = json!(fg.to_string());
    }
    if let Some(bg) = run.bg {
        answer["bg"] = json!(bg.to_string());
    }
    let flags = [
        ("bold", run.bold),
        ("dim", run.dim),
        ("italic", run.italic),
        ("underline", run.underline),
        ("inverse", run.inverse),
        ("strikethrough", run.strikethrough),
        ("blink", run.blink),
    ];
    for (name, set) in flags {
        if set {
            answer[name] = json!(true);
        }
    }
    answer
}

/// Run [`CLAIM_SESSION_TERMINAL`], [`SEND_SESSION_KEYS`] or [`RELEASE_SESSION_TERMINAL`] for `home`:
/// resolve the target, then take, use or give back the lease under [`project_home_screen_id`].
/// `args` is the call's parsed argument object.
async fn execute_terminal_keys_tool(
    port: &dyn ProjectHomeTerminalPort,
    home: &HomeCaller,
    tool_name: &str,
    args: Value,
) -> ToolOutcome {
    if tool_name == CLAIM_SESSION_TERMINAL {
        return claim_session_terminal(port, home, args).await;
    }
    if tool_name == RELEASE_SESSION_TERMINAL {
        return release_session_terminal(port, home, args).await;
    }
    send_session_keys(port, home, args).await
}

/// `ReleaseSessionTerminal`: give back the lease this home session claimed on the target. Answers
/// `{released: true}`; a claim it does not hold is refused with the port's reason.
async fn release_session_terminal(
    port: &dyn ProjectHomeTerminalPort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: SessionArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutcome::err(format!(
                "{RELEASE_SESSION_TERMINAL}: invalid arguments: {e}"
            ))
        }
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    match port
        .release_terminal(&target, &project_home_screen_id(home))
        .await
    {
        Ok(()) => ToolOutcome::ok(json!({ "released": true }).to_string()),
        Err(e) => ToolOutcome::err(format!("{RELEASE_SESSION_TERMINAL}: {e}")),
    }
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

/// `SendSessionKeys`: encode the text and keys, then type them under this home session's lease —
/// the text, then, once the application has echoed it, the keys ([`type_into`]). The text is
/// encoded as a paste only when the target's terminal has enabled bracketed paste, so that is read
/// first, and only when there is text to type. Every refusal before the first write leaves nothing
/// typed. Answers `{sent_bytes}`, the number of bytes written, and — when keys followed text —
/// `echo_seen`, whether the application echoed the text before the keys were pressed.
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
    let typing = match encode_typing(typing_text, &args.keys, bracketed_paste) {
        Ok(typing) => typing,
        Err(e) => return ToolOutcome::err(format!("{SEND_SESSION_KEYS}: {e}; nothing was sent")),
    };
    let screen_id = project_home_screen_id(home);
    let terminal = LeasedTerminal {
        port,
        target: &target,
        screen_id: &screen_id,
    };
    match type_into(&terminal, typing, &EchoWait::default()).await {
        Ok(typed) => {
            let mut answer = json!({ "sent_bytes": typed.sent_bytes });
            if let Some(echo_seen) = typed.echo_seen {
                answer["echo_seen"] = json!(echo_seen);
            }
            ToolOutcome::ok(answer.to_string())
        }
        Err(e) => ToolOutcome::err(format!("{SEND_SESSION_KEYS}: {e}")),
    }
}

/// A target's terminal typed into under a home session's lease: its output read through
/// [`ProjectHomeTerminalPort::read_terminal`], each write made through
/// [`ProjectHomeTerminalPort::send_keys`], which checks the lease.
struct LeasedTerminal<'a> {
    port: &'a dyn ProjectHomeTerminalPort,
    target: &'a SessionTarget,
    screen_id: &'a str,
}

#[async_trait]
impl TypingTerminal for LeasedTerminal<'_> {
    async fn output_offset(&self) -> Result<u64, String> {
        self.port
            .read_terminal(self.target)
            .await
            .map(|grid| grid.offset)
            .map_err(|e| format!("cannot read the terminal: {e:#}"))
    }

    async fn write(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.port.send_keys(self.target, self.screen_id, bytes).await
    }
}
