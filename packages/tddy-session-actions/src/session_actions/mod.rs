//! Declarative session actions live in [`tddy_session_store::session_actions`]. Re-exported at its
//! old path for existing callers; new code should name `tddy_session_store::session_actions`
//! directly.
//!
//! Listing and invoking from a session directory alone stays here: it reads the session's
//! `changeset.yaml` through [`crate::read_changeset`], which belongs to the workflow layer.

pub use tddy_session_store::session_actions::*;

mod session_dir;

pub use session_dir::{
    invoke_action_in_session_dir, list_actions_in_session_dir, ListActionsResponse,
};
