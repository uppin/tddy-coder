//! Until the main agent can answer them, a coding subagent's permission requests are denied.

use std::sync::Mutex;

use tddy_agent_backend::{PermissionDecision, PermissionHandler, PermissionRequest};

/// Denies every permission request (its first `reject_once` option, or cancelled when it offers
/// none) and keeps it, so the turn's messages can say what was denied.
#[derive(Debug, Default)]
pub struct DenyAndRecord {
    denied: Mutex<Vec<PermissionRequest>>,
}

impl DenyAndRecord {
    /// The requests denied since the last call, oldest first; draining them.
    pub fn take_denied(&self) -> Vec<PermissionRequest> {
        // TODO(acp-subagent): implement
        let _ = &self.denied;
        Vec::new()
    }
}

#[async_trait::async_trait]
impl PermissionHandler for DenyAndRecord {
    async fn decide(&self, _request: PermissionRequest) -> PermissionDecision {
        // TODO(acp-subagent): implement
        PermissionDecision::Cancelled
    }
}
