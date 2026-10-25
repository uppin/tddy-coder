//! A coding agent as a session subagent.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

pub mod approval_routes;
pub mod catalog;
pub mod confinement;
pub mod launcher;
pub mod parked_permissions;
pub mod permissions;
pub mod session;
pub mod transports;

pub use approval_routes::ApprovalRoutes;
pub use catalog::{
    coding_agent_defs, coding_agent_defs_in, CodingAgentDef, ACP_HARNESSES, CLAUDE_ACP, CODEX_ACP,
    OPENCODE,
};
pub use confinement::{
    confinement_for, opencode_withdrawal_config, refuse_unless_confinable, withdrawal_session_meta,
    Confinement, OPENCODE_CONFIG_CONTENT,
};
pub use launcher::{
    AgentTransport, CodingAgentLaunch, CodingAgentLauncher, CONVERSATION_ID_ENV,
    TDDY_TOOLS_MCP_ARG, TDDY_TOOLS_SERVER,
};
pub use parked_permissions::ParkedPermissions;
pub use permissions::DenyAndRecord;
pub use session::{CancelHandle, CodingAgentSession};
pub use transports::{
    permission_policy_of, transport_and_policy_wire, transport_of, PermissionPolicy, Transport,
    HEADLESS_HARNESSES,
};
