//! Changeset manifest — unified workflow state, sessions, and model configuration.
//!
//! Replaces `.session` and `.impl-session` with a single `changeset.yaml` file.
//!
//! The manifest is four concerns, each its own module: the [`model`] it stores, the PR [`stack`]
//! an orchestrator session carries beside it, the [`io`] that persists both, and what [`merge`]
//! makes of them when a session continues. This module publishes them and defines nothing.

pub mod io;
pub mod merge;
pub mod model;
pub mod stack;

pub use io::*;
pub use merge::*;
pub use model::*;
pub use stack::*;
