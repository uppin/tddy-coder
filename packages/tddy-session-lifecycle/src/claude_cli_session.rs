//! Backward-compatible re-export of [`crate::cli_session_manager`].
//!
//! New code should import from `cli_session_manager` directly; this module preserves the
//! historical `claude_cli_session` path used across tests and dependent crates.

pub use crate::cli_session_manager::*;
