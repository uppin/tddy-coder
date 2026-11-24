//! The MCP tools that drive the session: start it, read its screen, type into it, wait for text,
//! read and wait for its hooks, ask its status, stop it.

use std::sync::Arc;
use std::time::Duration;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::json;
use tddy_task::GridRegion;
use tddy_tool_engine::project_home_terminal_tools::terminal_grid_answer;
use tddy_tool_engine::terminal_keys::{encode_typing, type_into, EchoWait, TypingTerminal};

use crate::session::DriverSession;

/// How long `wait_for_text` waits when it is not told.
const DEFAULT_WAIT: Duration = Duration::from_secs(30);

/// A box of screen cells: its top-left cell, zero-based, and its size.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RegionArgs {
    pub row: u16,
    pub col: u16,
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct ReadTerminalArgs {
    #[schemars(description = "Also answer colours and attributes (styles, highlighted_rows)")]
    pub styles: Option<bool>,
    #[schemars(description = "Read only this box of the screen")]
    pub region: Option<RegionArgs>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct SendKeysArgs {
    #[schemars(
        description = "Text typed first; a line break is typed as Enter unless the terminal takes a paste"
    )]
    pub text: Option<String>,
    #[schemars(
        description = "Named keys typed after the text: Enter, Escape, Tab, BTab, Backspace, Delete, Up, Down, Left, Right, Home, End, PageUp, PageDown, Space, C-a … C-z, F1 … F12"
    )]
    pub keys: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitForTextArgs {
    #[schemars(description = "The text to wait for; whitespace is ignored on both sides")]
    pub text: String,
    #[schemars(description = "How long to wait, in seconds (default 30)")]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct ReadHooksArgs {
    #[schemars(description = "Answer the hooks from this index on (default 0, every hook)")]
    pub since: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WaitForHookArgs {
    #[schemars(
        description = "The hook event to wait for, as the agent names it (SessionStart, Stop, stop, …)"
    )]
    pub event: String,
    #[schemars(description = "Only a hook from this index on counts (default 0)")]
    pub since: Option<usize>,
    #[schemars(description = "How long to wait, in seconds (default 30)")]
    pub timeout_secs: Option<u64>,
}

/// The driver's MCP server over its one session.
#[derive(Clone)]
pub struct DriverServer {
    tool_router: ToolRouter<Self>,
    session: Arc<DriverSession>,
}

#[tool_router]
impl DriverServer {
    pub fn new(session: Arc<DriverSession>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            session,
        }
    }

    #[tool(
        description = "Launch the session the driver's arguments describe, through the daemon's own launch code. Answers {state: running, after_ms}, or fails with the launch's own failure — for an agent that exited while starting, its last output. A driver runs one session: a second start is refused."
    )]
    async fn start_session(&self) -> Result<String, String> {
        let after = self.session.start().await?;
        Ok(json!({ "state": "running", "after_ms": after.as_millis() as u64 }).to_string())
    }

    #[tool(
        description = "Read what the session's terminal shows now, as ReadSessionTerminal answers it: lines (the visible rows, trailing whitespace trimmed), cursor {row, col, visible}, cols, rows and offset; with styles: true also styles and highlighted_rows; with region {row, col, rows, cols} only that box."
    )]
    async fn read_terminal(
        &self,
        Parameters(args): Parameters<ReadTerminalArgs>,
    ) -> Result<String, String> {
        let grid = self.session.grid().await?;
        let region = args.region.map(|region| GridRegion {
            row: region.row,
            col: region.col,
            rows: region.rows,
            cols: region.cols,
        });
        terminal_grid_answer(&grid, args.styles.unwrap_or(false), region)
            .map(|answer| answer.to_string())
    }

    #[tool(
        description = "Type into the session's terminal: text first, then named keys, encoded and typed as SendSessionKeys does (the text as one paste when the terminal enabled bracketed paste; the keys once the application has echoed the text, or after 5 seconds with no echo). Answers {sent_bytes}, plus echo_seen when keys followed text. Nothing is sent when a key name is unknown or the session has exited."
    )]
    async fn send_keys(
        &self,
        Parameters(args): Parameters<SendKeysArgs>,
    ) -> Result<String, String> {
        let text = args.text.as_deref().filter(|text| !text.is_empty());
        let bracketed_paste = match text {
            Some(_) => self.session.grid().await?.bracketed_paste,
            None => false,
        };
        let typing = encode_typing(text, &args.keys.unwrap_or_default(), bracketed_paste)
            .map_err(|refusal| format!("{refusal}; nothing was sent"))?;
        let typed = type_into(&*self.session, typing, &EchoWait::default())
            .await
            .map_err(|e| e.to_string())?;
        Ok(typed.answer().to_string())
    }

    #[tool(
        description = "Wait until the session's screen shows text (whitespace ignored on both sides, since full-screen TUIs place words by moving the cursor), the session exits, or timeout_secs passes. Answers {matched, exited, lines}, the screen it last read."
    )]
    async fn wait_for_text(
        &self,
        Parameters(args): Parameters<WaitForTextArgs>,
    ) -> Result<String, String> {
        let timeout = args
            .timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_WAIT);
        let waited = self.session.wait_for(&args.text, timeout).await?;
        Ok(json!({
            "matched": waited.matched,
            "exited": waited.exited,
            "lines": waited.grid.map(|grid| grid.lines).unwrap_or_default(),
        })
        .to_string())
    }

    #[tool(
        description = "The session's lifecycle hooks, followed as a daemon follows a target session's: {followed, hooks, next}. Each hook is {index, event, payload, reports} — reports are the ReportSessionStatus / ReportAgentActivity requests the daemon's activity handling would be served for it, as {kind: session_status | agent_activity, …}, without the hook token. followed is false for an agent whose spec declares no hooks, or before the session started; next is how many hooks have arrived."
    )]
    async fn read_hooks(
        &self,
        Parameters(args): Parameters<ReadHooksArgs>,
    ) -> Result<String, String> {
        let read = self.session.hooks_since(args.since.unwrap_or(0));
        Ok(json!({
            "followed": read.followed,
            "hooks": read.hooks.iter().map(|hook| hook.to_json()).collect::<Vec<_>>(),
            "next": read.next,
        })
        .to_string())
    }

    #[tool(
        description = "Wait until a hook of event arrives (from index since on), or timeout_secs passes. Answers {matched, hook}, hook as read_hooks answers one, or null. Refused for a session whose hooks are not followed."
    )]
    async fn wait_for_hook(
        &self,
        Parameters(args): Parameters<WaitForHookArgs>,
    ) -> Result<String, String> {
        let timeout = args
            .timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_WAIT);
        let hook = self
            .session
            .wait_for_hook(&args.event, args.since.unwrap_or(0), timeout)
            .await?;
        Ok(json!({
            "matched": hook.is_some(),
            "hook": hook.map(|hook| hook.to_json()),
        })
        .to_string())
    }

    #[tool(
        description = "The session's state: not_started, running, or exited — with how it ended and its last output, or the failure its start answered."
    )]
    async fn session_status(&self) -> Result<String, String> {
        Ok(self.session.status().await.to_string())
    }

    #[tool(
        description = "Stop the session: its agent on a remote target, then its terminal. Answers {state: exited} once the terminal has ended."
    )]
    async fn stop_session(&self) -> Result<String, String> {
        self.session.stop().await?;
        Ok(json!({ "state": "exited" }).to_string())
    }
}

#[tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for DriverServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "tddy-session-driver: one agent session on a local, ssh or Coder target. Start it with \
             start_session, watch it with read_terminal and wait_for_text, type with send_keys, and \
             end it with stop_session.",
        )
    }
}

/// The driven session as [`type_into`] types into it: its screen's output offset, and its input.
#[async_trait::async_trait]
impl TypingTerminal for DriverSession {
    async fn output_offset(&self) -> Result<u64, String> {
        Ok(self.grid().await?.offset)
    }

    async fn write(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.send(bytes).await
    }
}
