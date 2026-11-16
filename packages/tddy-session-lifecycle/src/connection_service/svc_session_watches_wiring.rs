//! The session watches a [`DaemonSessionHost`] carries for the project-home watch tools
//! (`#activity-wake` 2/3): installed by the composition root, read by the exec-tool handler.

use std::sync::Arc;

use tddy_session_activity::session_watches::SessionWatchStore;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Install the session watches the project-home watch tools keep, and every orchestrator's
    /// tool answer is checked against (builder pattern).
    pub fn with_session_watches(mut self, watches: Arc<SessionWatchStore>) -> Self {
        self.session_watches = Some(watches);
        self
    }

    /// The session watches installed with [`Self::with_session_watches`], or `None` on a host
    /// with none.
    pub fn session_watches(&self) -> Option<Arc<SessionWatchStore>> {
        self.session_watches.clone()
    }
}
