//! `WaitProjectNotifications` and `ReadProjectNotifications` answered on this host
//! (`#home-inbox` 7/8).
//!
//! The tools ([`tddy_tool_engine::project_home_inbox_tools`]) read this host's project notification
//! log, `tddy_session_activity::project_notification_log`, for the caller's project and operating
//! system user, excluding the caller's own session. A call naming another host is forwarded to that
//! host whole and read there (`#start-parity` 1/9): there is no merged view across hosts.

use std::time::Duration;

use async_trait::async_trait;
use tddy_session_activity::project_notification_log::{
    record_json, ProjectNotificationLog, ProjectNotificationRecord, ProjectNotificationScope,
};
use tddy_tool_engine::project_home_inbox_tools::{
    NotificationsPage, ProjectHomeInboxPort, NO_NOTIFICATION_LOG,
};
use tddy_tool_engine::session_follow::FollowPage;
use tddy_worktree_service::session_target::HomeCaller;

use super::project_home::DaemonProjectHome;

impl DaemonProjectHome {
    /// The log this host keeps, or a refusal when none is installed.
    fn log(&self) -> Result<&ProjectNotificationLog, String> {
        self.notification_log
            .as_deref()
            .ok_or_else(|| NO_NOTIFICATION_LOG.to_string())
    }

    /// The records the caller may see: its own project, its own OS user, not its own session.
    fn scope_of(&self, home: &HomeCaller) -> ProjectNotificationScope {
        ProjectNotificationScope {
            project_id: home.project_id.clone(),
            os_user: self.os_user.clone(),
            exclude_session_id: home.session_id.clone(),
        }
    }
}

/// A page of the log's records, each as its JSON object.
fn json_page(
    page: FollowPage<ProjectNotificationRecord>,
) -> Result<FollowPage<serde_json::Value>, String> {
    let items = page
        .items
        .iter()
        .map(record_json)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("serialize a notification record: {e}"))?;
    Ok(FollowPage {
        items,
        next_cursor: page.next_cursor,
        has_more: page.has_more,
        clamped_to: page.clamped_to,
    })
}

#[async_trait]
impl ProjectHomeInboxPort for DaemonProjectHome {
    async fn read_notifications(
        &self,
        home: &HomeCaller,
        cursor: &str,
        limit: Option<usize>,
    ) -> Result<NotificationsPage, String> {
        let read = self.log()?.read(&self.scope_of(home), cursor, limit)?;
        Ok(NotificationsPage {
            page: json_page(read.page)?,
            lost: (read.lost > 0).then_some(read.lost),
            timed_out: None,
        })
    }

    async fn wait_notifications(
        &self,
        home: &HomeCaller,
        cursor: &str,
        timeout_ms: u64,
    ) -> Result<NotificationsPage, String> {
        let wait = self
            .log()?
            .wait(
                &self.scope_of(home),
                cursor,
                Duration::from_millis(timeout_ms),
            )
            .await?;
        Ok(NotificationsPage {
            page: json_page(wait.page)?,
            lost: (wait.lost > 0).then_some(wait.lost),
            timed_out: Some(wait.timed_out),
        })
    }
}
