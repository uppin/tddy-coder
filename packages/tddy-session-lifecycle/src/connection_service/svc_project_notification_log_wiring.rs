//! The project notification log a [`DaemonSessionHost`] carries for the project-home inbox tools
//! (`#home-inbox` 7/8): installed by the composition root, read by the exec-tool handler.

use std::sync::Arc;

use tddy_session_activity::project_notification_log::ProjectNotificationLog;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Install the project notification log the project-home notification tools read (builder
    /// pattern). The caller also registers it as a subscriber on the bus it installs with
    /// [`Self::with_session_notification_bus`]: this only lets the exec-tool handler reach it
    /// (`#home-inbox` 7/8).
    pub fn with_project_notification_log(mut self, log: Arc<ProjectNotificationLog>) -> Self {
        self.project_notification_log = Some(log);
        self
    }

    /// The project notification log installed with [`Self::with_project_notification_log`], or
    /// `None` on a host with none.
    pub fn project_notification_log(&self) -> Option<Arc<ProjectNotificationLog>> {
        self.project_notification_log.clone()
    }
}
