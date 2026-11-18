//! `tddy-session-driver`: one agent session on a local, ssh or Coder target, launched through the
//! daemon's own launch code (`launch_host_pty`, `launch_on_target`), and driven over MCP.
//!
//! - [`target`]: what `--target` names, and the shell that reaches it.
//! - [`session`]: the session's lifecycle — start, screen, keys, waits, stop.
//! - [`server`]: the MCP tools over it, served on stdin/stdout by the binary.
//!
//! PRD: docs/dev/1-WIP/PRD-2026-11-18-session-driver-cli.md.

pub mod server;
pub mod session;
pub mod target;
