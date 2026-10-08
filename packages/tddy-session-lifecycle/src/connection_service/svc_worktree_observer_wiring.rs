use crate::connection_service::session_worktree_observer::SessionWorktreeObserver;
use crate::connection_service::DaemonSessionHost;

impl DaemonSessionHost {
    /// Install the observer told of every started session's worktree (builder).
    #[must_use]
    pub fn with_worktree_observer(
        mut self,
        observer: std::sync::Arc<dyn SessionWorktreeObserver>,
    ) -> Self {
        self.debug_assert_rpc_families_not_installed("with_worktree_observer");
        self.worktree_observer = Some(observer);
        self
    }
}
