//! The changeset manifest lives in [`tddy_changeset::changeset`]. Re-exported at its old path for
//! existing callers, together with the session-continue goal the workflow engine chooses from it.

pub use crate::workflow::start_goal_for_session_continue;
pub use tddy_changeset::changeset::*;
