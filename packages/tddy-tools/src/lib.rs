//! tddy-tools library: the MCP server and the CLI's supporting surface.
//!
//! The binary is the primary interface. JSON Schema validation and the goal registry it serves
//! `get-schema` / `list-schemas` from live in `tddy_workflow_recipes::{schema, schema_manifest}`,
//! next to the `goals.json` they are generated from. The toolcall relay client and the CLI's
//! request/response wire shapes live in `tddy_core::toolcall`, beside the listener that serves
//! them.

pub mod action_tools;
pub mod list_models;
pub mod mcp_primitives;
pub mod relay;
pub mod server;
pub mod session_actions_cli;
pub mod session_agents;
pub mod session_tool_client;
