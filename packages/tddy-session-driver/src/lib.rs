//! `tddy-session-driver`: one agent session on a local, ssh or Coder target, launched through the
//! daemon's own launch code (`launch_host_pty`, `launch_on_target`), and driven over MCP.
//!
//! - [`target`]: what `--target` names, and the shell that reaches it.
//! - [`session`]: the session's lifecycle — start, screen, keys, waits, stop.
//! - [`hooks`]: the session's lifecycle hooks, wired and followed as a daemon target start's are.
//! - [`server`]: the MCP tools over it, served on stdin/stdout by the binary.
//!
//! Docs: packages/tddy-session-driver/docs/architecture.md.

pub mod hooks;
pub mod server;
pub mod session;
pub mod target;
