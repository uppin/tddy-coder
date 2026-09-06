//! The standalone sandboxed-session app, as a library.
//!
//! `tddy-sandbox-app` was a binary and nothing else until a session appeared whose confinement
//! claim can only be settled against a real kernel jail: `--codebase-mode sandboxed`, where the
//! checkout is inside the jail and the agent is not (`docs/ft/coder/sandboxed-codebase-mode.md`).
//! A test can spawn the binary, but it cannot dispatch a tool call through the socket the agent's
//! MCP server dispatches through — so the session lifecycle lives here, and `src/main.rs` is the
//! command-line front-end over it.
//!
//! The platform split is the same one the binary has always had: macOS spawns its Seatbelt jail
//! in-process, Linux drives a running `tddy-daemon` over gRPC, and neither module is compiled on
//! the other host.

pub mod bridge;
pub mod codebase_mode;
pub mod config;
#[cfg(target_os = "linux")]
pub mod daemon_client;
// Only `bridge` dispatches session actions, so the module stays inside the crate.
#[cfg(target_os = "macos")]
pub(crate) mod host_actions;
#[cfg(target_os = "macos")]
pub mod host_agent;
#[cfg(target_os = "macos")]
pub mod sandboxed_session;
#[cfg(target_os = "macos")]
pub mod spawn;
