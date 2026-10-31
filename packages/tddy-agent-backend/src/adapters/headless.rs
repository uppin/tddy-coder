//! The port a spec-driven headless launch is rendered through.
//!
//! `ClaudeCodeBackend` and `HeadlessDriver` live in this crate; the agent specs live in
//! `tddy-coding-agent-specs`, which depends on this crate. So the consumers here name a
//! [`HeadlessRenderer`] — what one headless turn of an agent is spawned with — and the specs crate
//! implements it from `coding_agent.headless` (`SpecHeadlessRenderer`). Nothing here names an agent.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::backend::SessionMode;
use crate::error::BackendError;

use super::StreamAdapter;

/// The MCP server one turn registers with its agent (`tddy-tools --mcp`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessMcpServer {
    /// The server's name in the registration (`tddy-tools`).
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    /// The server's `env` block; omitted from the registration when empty.
    pub env: BTreeMap<String, String>,
}

/// What one headless turn is launched with: the template's parameters and the slots' inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessTurn {
    pub prompt: String,
    pub model: Option<String>,
    /// The agent's own session: `Fresh` names a new one, `Resume` continues one; `None` names none
    /// (a conversation's first turn).
    pub session: Option<SessionMode>,
    /// The template's `permission_mode` (`plan`, `acceptEdits`); `None` leaves the agent's default.
    pub permission_mode: Option<String>,
    /// Passed with the spec's `append_system_prompt` flag; `None` adds no flag.
    pub append_system_prompt_file: Option<PathBuf>,
    /// Pre-approved tools, in order.
    pub allowed_tools: Vec<String>,
    /// Withdrawn tools, in order.
    pub denied_tools: Vec<String>,
    /// The MCP tool the agent asks before anything not pre-approved.
    pub permission_prompt_tool: Option<String>,
    /// The tool server to register; `None` registers none.
    pub mcp_server: Option<HeadlessMcpServer>,
    /// The agent's working directory.
    pub workdir: PathBuf,
    /// Where the turn's registration files (the MCP config) are written.
    pub scratch_dir: PathBuf,
}

/// One rendered headless turn: what to spawn, what to write first, and how to read it.
#[derive(Clone)]
pub struct HeadlessInvocation {
    /// The resolved binary (`{binary}`).
    pub program: PathBuf,
    /// The rendered argv after the program.
    pub argv: Vec<String>,
    /// The spec's `headless.env`, set on the child on top of what the consumer passes.
    pub env: Vec<(String, String)>,
    /// Files the slots render (the MCP config), written before the spawn.
    pub files: Vec<(PathBuf, Vec<u8>)>,
    /// The adapter the spec names, which reads the child's stdout.
    pub adapter: Arc<dyn StreamAdapter>,
    /// The spec's `headless.exit_ok_if_structured`, handed to [`StreamAdapter::exit_ok`].
    pub exit_ok_if_structured: bool,
}

impl fmt::Debug for HeadlessInvocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeadlessInvocation")
            .field("program", &self.program)
            .field("argv", &self.argv)
            .field("env", &self.env)
            .field(
                "files",
                &self.files.iter().map(|(path, _)| path).collect::<Vec<_>>(),
            )
            .field("adapter", &self.adapter.name())
            .field("exit_ok_if_structured", &self.exit_ok_if_structured)
            .finish()
    }
}

/// Renders an agent's headless turns from its declaration.
pub trait HeadlessRenderer: Send + Sync + fmt::Debug {
    /// The agent's id (`claude`).
    fn agent_id(&self) -> &str;

    /// The tool server this agent registers when its consumer names no other: its `tddy-tools` dep
    /// with `--mcp`. `None` when the agent has no such dep.
    fn tool_server(&self) -> Option<HeadlessMcpServer>;

    /// `turn` rendered: the agent's `coding_agent.headless.argv` with `turn`'s parameters and its
    /// slots, the files those slots write, and the adapter its output is read through.
    fn render(&self, turn: &HeadlessTurn) -> Result<HeadlessInvocation, BackendError>;
}
