//! Declarative session actions now live in [`tddy_session_store::session_actions`]; this path is
//! kept so callers are not edited.
//!
//! Listing and invoking from a session directory alone stays here: it reads the session's
//! `changeset.yaml` through [`crate::read_changeset`], which belongs to the workflow layer.

pub use tddy_session_store::session_actions::*;

mod session_dir;

pub use session_dir::{
    invoke_action_in_session_dir, list_actions_in_session_dir, ListActionsResponse,
};
