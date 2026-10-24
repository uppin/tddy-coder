//! A coding subagent's permission requests, parked until the caller answers them.

use tddy_agent_backend::{PermissionDecision, PermissionHandler, PermissionRequest};
use tddy_discovery::subagent::{PermissionAnswer, PermissionRequestView};

/// The [`PermissionHandler`] a coding-agent conversation installs: a request is parked — the
/// agent's `session/request_permission` stays unanswered — and the tddy turn ends with it; the
/// caller's [`PermissionAnswer`] later resolves it, and the agent carries on.
///
/// One request at a time: a conversation with a parked request refuses a new prompt.
#[derive(Debug, Default)]
pub struct ParkedPermissions {
    // TODO(permission-yield): implement — the parked request and its oneshot, the next-request
    // notification the session waits on
    _private: (),
}

impl ParkedPermissions {
    /// The request parked now, if any.
    pub fn pending(&self) -> Option<PermissionRequestView> {
        // TODO(permission-yield): implement
        None
    }

    /// Deliver `answer` to the parked request it names. A different `request_id` — or no parked
    /// request — is refused naming the pending one.
    pub fn answer(&self, _answer: &PermissionAnswer) -> Result<(), String> {
        // TODO(permission-yield): implement
        Err("TODO(permission-yield): ParkedPermissions::answer is not implemented".to_string())
    }
}

#[async_trait::async_trait]
impl PermissionHandler for ParkedPermissions {
    async fn decide(&self, _request: PermissionRequest) -> PermissionDecision {
        // TODO(permission-yield): implement
        PermissionDecision::Cancelled
    }
}
