//! A coding agent as a session subagent.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Coding agents

pub mod approval_routes;
pub mod catalog;
pub mod confinement;
pub mod launcher;
pub mod parked_permissions;
mod permission_records;
pub mod permissions;
pub mod session;
pub mod transports;
pub mod user_permission_answers;

pub use approval_routes::ApprovalRoutes;
pub use catalog::{
    coding_agent_defs, coding_agent_defs_from, coding_agent_defs_in, CodingAgentDef,
};
pub use confinement::{
    confinable_acp_agents, confinement_of_spec, refuse_unless_registry_confinable,
    refuse_unless_spec_confinable, Confinement, OPENCODE_CONFIG_CONTENT,
};
pub use launcher::{
    no_launch_hooks, AgentTransport, CodingAgentLaunch, CodingAgentLauncher, LaunchedAgent,
    CONVERSATION_ID_ENV, TDDY_TOOLS_MCP_ARG, TDDY_TOOLS_SERVER,
};
pub use parked_permissions::ParkedPermissions;
pub use permissions::DenyAndRecord;
pub use session::{CancelHandle, CodingAgentSession};
pub use transports::{
    permission_policy_of, permission_policy_of_spec, transport_and_policy_wire,
    transport_and_policy_wire_of_spec, transport_of, transport_of_spec, PermissionPolicy,
    Transport,
};
pub use user_permission_answers::{AnsweredBy, PermissionAnswering};
