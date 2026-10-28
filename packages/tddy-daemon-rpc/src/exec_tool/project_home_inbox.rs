//! `WaitProjectNotifications` and `ReadProjectNotifications` answered on this host
//! (`#home-inbox` 7/8).
//!
//! The tools ([`tddy_tool_engine::project_home_inbox_tools`]) read this host's project notification
//! log, `tddy_session_activity::project_notification_log`, for the caller's project and operating
//! system user, excluding the caller's own session. A call naming another host is forwarded to that
//! host whole and read there (`#start-parity` 1/9): there is no merged view across hosts.

use async_trait::async_trait;
use tddy_tool_engine::project_home_inbox_tools::{NotificationsPage, ProjectHomeInboxPort};
use tddy_worktree_service::session_target::HomeCaller;

use super::project_home::DaemonProjectHome;

#[async_trait]
impl ProjectHomeInboxPort for DaemonProjectHome {
    async fn read_notifications(
        &self,
        home: &HomeCaller,
        cursor: &str,
        limit: Option<usize>,
    ) -> Result<NotificationsPage, String> {
        // TODO(home-wait): implement — `ProjectNotificationLog::read` with the scope
        // `{ home.project_id, the caller's OS user, home.session_id }`, records as JSON.
        let _ = (&self.notification_log, home, cursor, limit);
        Err("read_notifications: not implemented".to_string())
    }

    async fn wait_notifications(
        &self,
        home: &HomeCaller,
        cursor: &str,
        timeout_ms: u64,
    ) -> Result<NotificationsPage, String> {
        // TODO(home-wait): implement — `ProjectNotificationLog::wait`, as above.
        let _ = (&self.notification_log, home, cursor, timeout_ms);
        Err("wait_notifications: not implemented".to_string())
    }
}
