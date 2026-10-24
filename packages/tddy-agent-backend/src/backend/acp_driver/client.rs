//! The driver's side of the connection: a session update is recorded on the turn running on its
//! session, and a permission request is answered by the installed handler.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use agent_client_protocol as acp;

use super::mapping;
use super::{AcpUpdate, AcpUpdateSink, PermissionHandler};
use crate::backend::ProgressSink;
use crate::error::BackendError;

/// The turns running on one connection, keyed by session. A turn is recorded from the moment its
/// prompt is sent until the agent answers it.
#[derive(Clone, Default)]
pub(super) struct Turns(Rc<RefCell<HashMap<String, Turn>>>);

struct Turn {
    output: String,
    updates: Option<AcpUpdateSink>,
    progress: Option<ProgressSink>,
}

impl Turns {
    /// Start a turn on `session_id`. A session runs one turn at a time.
    pub(super) fn begin(
        &self,
        session_id: &str,
        updates: Option<AcpUpdateSink>,
        progress: Option<ProgressSink>,
    ) -> Result<(), BackendError> {
        let mut turns = self.0.borrow_mut();
        if turns.contains_key(session_id) {
            return Err(BackendError::InvocationFailed(format!(
                "a prompt is already running on session {session_id}"
            )));
        }
        turns.insert(
            session_id.to_string(),
            Turn {
                output: String::new(),
                updates,
                progress,
            },
        );
        Ok(())
    }

    /// End the turn on `session_id`, returning the agent's text for it; `None` when no turn was
    /// running there.
    pub(super) fn end(&self, session_id: &str) -> Option<String> {
        self.0
            .borrow_mut()
            .remove(session_id)
            .map(|turn| turn.output)
    }

    /// Deliver one update to the turn running on its session, if there is one.
    fn record(&self, session_id: &str, update: AcpUpdate) {
        let mut turns = self.0.borrow_mut();
        let Some(turn) = turns.get_mut(session_id) else {
            return;
        };
        if let AcpUpdate::Text(text) = &update {
            turn.output.push_str(text);
        }
        if let Some(progress) = &turn.progress {
            mapping::report_progress(&update, progress);
        }
        if let Some(updates) = &turn.updates {
            updates(update);
        }
    }
}

/// The client the connection hands the agent's requests to.
pub(super) struct DriverClient {
    turns: Turns,
    permissions: Arc<dyn PermissionHandler>,
}

impl DriverClient {
    pub(super) fn new(turns: Turns, permissions: Arc<dyn PermissionHandler>) -> Self {
        Self { turns, permissions }
    }
}

#[async_trait::async_trait(?Send)]
impl acp::Client for DriverClient {
    async fn session_notification(
        &self,
        notification: acp::SessionNotification,
    ) -> acp::Result<()> {
        let session_id = notification.session_id.0.to_string();
        if let Some(update) = mapping::acp_update(notification.update) {
            self.turns.record(&session_id, update);
        }
        Ok(())
    }

    async fn request_permission(
        &self,
        request: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        let decision = self
            .permissions
            .decide(mapping::permission_request(&request))
            .await;
        Ok(acp::RequestPermissionResponse::new(
            mapping::permission_outcome(decision),
        ))
    }
}
