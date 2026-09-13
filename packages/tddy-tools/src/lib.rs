//! tddy-tools library: the MCP server and the CLI's supporting surface.
//!
//! The binary is the primary interface. JSON Schema validation and the goal registry it serves
//! `get-schema` / `list-schemas` from live in `tddy_workflow_recipes::{schema, schema_manifest}`,
//! next to the `goals.json` they are generated from. The toolcall relay client and the CLI's
//! request/response wire shapes live in `tddy_core::toolcall`, beside the listener that serves
//! them. Reaching this session's daemon — transport detection and tool dispatch over the sandbox
//! socket, daemon HTTP or LiveKit RPC — is `tddy_session_tool_client`, re-exported below so the
//! modules that dispatch through it, and the suites that drive it, keep one spelling. The live
//! agent roster and the subagent conversation runtime are `tddy_discovery::{roster,
//! subagent_runtime}` for the same reason — this crate advertises those tools and no longer
//! implements them.

pub mod action_tools;
pub mod list_models;
pub mod mcp_primitives;
pub mod relay;
pub mod server;
pub mod session_actions_cli;

/// The in-session tool client, re-exported at the path it was reached by while it lived here.
pub use tddy_session_tool_client as session_tool_client;

/// The session's live agent roster, re-exported at the path it was reached by while it lived here.
///
/// It is `tddy_discovery::roster` now: every type it manipulates is that crate's, and it needs
/// `tddy-service`'s roster protos, which `tddy-discovery` can depend on and `tddy-core` cannot.
pub use tddy_discovery::roster as session_agents;
