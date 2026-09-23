//! The tool-call protocol between an agent's `tddy-tools` process and the host running its
//! session: the relay listener, its client, and the channels submit results and transitions travel
//! through.
//!
//! Extracted from `tddy-core`, which re-exports the module at its old path.

pub mod toolcall;

// The session-action surface the listener serves, named at the `crate::` path it used inside
// `tddy-core`.
use tddy_session_actions::session_actions;
