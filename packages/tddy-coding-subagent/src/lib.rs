//! A coding agent as a session subagent.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

pub mod catalog;
pub mod confinement;
pub mod launcher;
pub mod permissions;
pub mod session;

pub use catalog::{
    coding_agent_defs, coding_agent_defs_in, CodingAgentDef, ACP_HARNESSES, CLAUDE_ACP, CODEX_ACP,
    OPENCODE,
};
pub use confinement::{
    confinement_for, opencode_withdrawal_config, refuse_unless_confinable, withdrawal_session_meta,
    Confinement, OPENCODE_CONFIG_CONTENT,
};
pub use launcher::{
    AgentTransport, CodingAgentLaunch, CodingAgentLauncher, TDDY_TOOLS_MCP_ARG, TDDY_TOOLS_SERVER,
};
pub use permissions::DenyAndRecord;
pub use session::{CancelHandle, CodingAgentSession};
