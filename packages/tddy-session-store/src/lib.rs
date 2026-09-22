//! Session storage on disk: atomic file writes, session directories, the workflow error type and
//! declarative session actions.
//!
//! Extracted from `tddy-core`, which re-exports every module at its old path.

pub mod atomic_file;
pub mod error;
pub mod output;
pub mod session_actions;
