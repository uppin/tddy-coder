//! The changeset manifest lives in [`tddy_changeset::changeset`]. Re-exported at its old path for
//! existing callers, together with the session-continue goal the workflow engine chooses from it.

pub use tddy_changeset::changeset::*;
pub use tddy_workflow_engine::start_goal_for_session_continue;
