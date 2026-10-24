//! Until the main agent can answer them, a coding subagent's permission requests are denied.

use std::sync::Mutex;

use tddy_agent_backend::{PermissionDecision, PermissionHandler, PermissionRequest};

/// The kind ACP gives an option that rejects one call.
const REJECT_ONCE_KIND: &str = "reject_once";

/// Denies every permission request (its first `reject_once` option, or cancelled when it offers
/// none) and keeps it, so the turn's messages can say what was denied.
#[derive(Debug, Default)]
pub struct DenyAndRecord {
    denied: Mutex<Vec<PermissionRequest>>,
}

impl DenyAndRecord {
    /// The requests denied since the last call, oldest first; draining them.
    pub fn take_denied(&self) -> Vec<PermissionRequest> {
        let mut denied = self.denied.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *denied)
    }
}

#[async_trait::async_trait]
impl PermissionHandler for DenyAndRecord {
    async fn decide(&self, request: PermissionRequest) -> PermissionDecision {
        let decision = match request
            .options
            .iter()
            .find(|option| option.kind == REJECT_ONCE_KIND)
        {
            Some(option) => PermissionDecision::Selected {
                option_id: option.option_id.clone(),
            },
            None => PermissionDecision::Cancelled,
        };
        self.denied
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(request);
        decision
    }
}
