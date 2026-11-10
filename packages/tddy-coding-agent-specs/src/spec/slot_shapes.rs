use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Where an agent takes its MCP server registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum McpSlot {
    /// A file in `format`, passed by `flag <path>`, then `strict_flag` when the spec declares one.
    File {
        flag: String,
        format: McpFormat,
        /// Loads the registered servers and no other MCP configuration (`--strict-mcp-config`),
        /// passed after the path. Optional: an agent without such a flag omits it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strict_flag: Option<String>,
    },
    /// A file in `format` written where the agent looks for it, passed by no flag
    /// (`#agent-tools 7/12`: Cursor reads `$HOME/.cursor/mcp.json`). `path` may name `{home}` — the
    /// agent's home on this launch ([`crate::slots::SlotInputs::home`]); every other character is
    /// literal.
    Placed { path: String, format: McpFormat },
    /// The agent takes no MCP registration (`#agent-tools 8/12`: OpenCode, Muse and Warp on
    /// today's launch). A launch that passes a tool server is refused rather than dropped, as an
    /// `unsupported` tool slot is.
    Unsupported,
    /// The registration as argv config overrides, with no file (`#agent-tools 9/12`: Codex's
    /// `-c mcp_servers.<name>.command=…`). Each field is `flag <key_prefix>.<name>.<field>=<toml>`
    /// for `command`, `args` and `env_vars`. The server's environment is **named** (`env_vars`),
    /// never valued: the agent forwards those variables from its own environment, so the
    /// credential stays off the argv another user's `ps` reads.
    Inline { flag: String, key_prefix: String },
    /// A file passed by `flag <path>` in the servers-by-name shape `{"<name>": {command, args,
    /// env}}` (`#agent-tools 9/12`: Oz's `--mcp <path>`), written owner-only under the launch's
    /// scratch directory as [`crate::slots::headless_mcp::ARGV_MCP_FILE`].
    Argv { flag: String },
    /// A file in `format` written at `path` under the agent's working directory, passed by no flag
    /// (`#agent-tools 9/12`: a headless Cursor reads its worktree's `.cursor/mcp.json`). The file
    /// is the conversation's, so it is listed in `RenderedSlots::written` and removed when the
    /// conversation closes or the start-up sweep finds it.
    Workdir { path: PathBuf, format: McpFormat },
    /// The registration as ACP `session/new` (and `session/load`) `mcp_servers`, with no file and
    /// no flag (`#agent-tools 11/12`: every ACP agent). Rendered by
    /// [`crate::slots::acp::acp_mcp_servers`].
    AcpSession,
}

/// The shape of an MCP registration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpFormat {
    /// `{"mcpServers": {"<name>": {"command", "args", "env"?}}}`.
    ClaudeMcpJson,
    /// Cursor's `.cursor/mcp.json`: the same `mcpServers` object, at the placed path
    /// (`#agent-tools 7/12`).
    CursorMcpJson,
}

/// A tool slot: the flag an agent takes it by, or the declaration that it has none.
///
/// `tool_deny: unsupported` is how a spec says "this agent cannot withdraw its native tools" (Cursor,
/// Codex, Oz). It is spec data rather than an absent field, so the gap is reviewable in the YAML and
/// a caller that needs the slot is refused by name instead of silently getting nothing.
///
/// An ACP agent takes its withdrawn tools another way (`#agent-tools 11/12`), each written as a map
/// keyed by its kind: `{ acp_meta: <dotted path> }` — a list at that path of `session/new` `_meta`
/// (Claude's `claudeCode.options.disallowedTools`) — and `{ config_env: <VAR> }` — inline JSON
/// config `{"tools": {<tool>: false}}` in that variable (OpenCode's `OPENCODE_CONFIG_CONTENT`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolSlot {
    Flag(FlagSlot),
    Unsupported(Unsupported),
    /// The tools as a list at `path` (dotted) in the ACP `session/new` `_meta`.
    AcpMeta {
        #[serde(rename = "acp_meta")]
        path: String,
    },
    /// The tools switched off in inline JSON config handed in variable `env`.
    ConfigEnv {
        #[serde(rename = "config_env")]
        env: String,
    },
}

/// The YAML word `unsupported`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unsupported {
    #[serde(rename = "unsupported")]
    Unsupported,
}

impl ToolSlot {
    /// The flag, when the agent has one.
    pub fn flag(&self) -> Option<&FlagSlot> {
        match self {
            ToolSlot::Flag(flag) => Some(flag),
            ToolSlot::Unsupported(_) | ToolSlot::AcpMeta { .. } | ToolSlot::ConfigEnv { .. } => {
                None
            }
        }
    }
}

/// A slot rendered as `flag <value>` argv pairs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlagSlot {
    pub flag: String,
}

/// Where an agent reads its instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstructionsSlot {
    /// Context files a launch's preamble is prepended to, relative to the agent's working
    /// directory; one the directory lacks is left alone.
    pub files: Vec<PathBuf>,
    /// The flag a system-prompt file is appended with; `None` for an agent without one.
    #[serde(default)]
    pub append_system_prompt: Option<FlagSlot>,
    /// A rules file, relative to the agent's working directory, that a launch's preamble is
    /// written to **whole** (`#agent-tools 7/12`: Cursor's `.cursor/rules/<name>.mdc`); `None` for
    /// an agent without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_file: Option<PathBuf>,
}

/// Which Rust hooks adapter writes the agent's lifecycle-hooks configuration. The hook protocol is
/// not declarable; a file can only name an adapter (`claude-settings-local`, `cursor-hooks-json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HooksSlot {
    pub adapter: String,
}
