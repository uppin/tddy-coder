//! Session actions run against one session: listing and invoking them from a session directory,
//! the background jobs that run them, and the pipeline that validates and chains their results.
//!
//! These sit above `tddy-changeset` because each reads the session's `changeset.yaml`; the
//! declarative action store itself lives in `tddy-session-store`, beneath both.
//!
//! Extracted from `tddy-core`, which re-exports every module at its old path.

pub mod session_action_jobs;
pub mod session_action_pipeline;
pub mod session_actions;

// The storage layer and the changeset reader, named at the `crate::` paths these modules used
// inside `tddy-core`.
use tddy_changeset::read_changeset;
use tddy_session_store::error::WorkflowError;
use tddy_session_store::{atomic_file, error, output};
