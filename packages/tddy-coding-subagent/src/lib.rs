//! A coding agent as a session subagent.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md

pub mod catalog;
pub mod confinement;
pub mod launcher;
pub mod parked_permissions;
pub mod permissions;
pub mod session;
pub mod transports;
pub mod user_permission_answers;

pub use catalog::{coding_agent_defs, CodingAgentDef, ACP_HARNESSES};
pub use confinement::{confinement_for, Confinement};
pub use launcher::{CodingAgentLaunch, CodingAgentLauncher};
pub use parked_permissions::ParkedPermissions;
pub use permissions::DenyAndRecord;
pub use session::{CancelHandle, CodingAgentSession};
pub use transports::{
    permission_policy_of, transport_of, PermissionPolicy, Transport, HEADLESS_HARNESSES,
};
pub use user_permission_answers::{AnsweredBy, PermissionAnswering};
