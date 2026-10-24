//! A coding agent as a session subagent.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md

pub mod catalog;
pub mod confinement;
pub mod launcher;
pub mod permissions;
pub mod session;

pub use catalog::{coding_agent_defs, coding_agent_defs_in, CodingAgentDef, ACP_HARNESSES};
pub use confinement::{confinement_for, withdrawal_env, withdrawal_session_meta, Confinement};
pub use launcher::{AgentTransport, CodingAgentLaunch, CodingAgentLauncher};
pub use permissions::DenyAndRecord;
pub use session::{CancelHandle, CodingAgentSession};
