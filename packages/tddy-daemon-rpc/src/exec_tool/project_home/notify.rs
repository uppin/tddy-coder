use crate::exec_tool::project_home::DaemonProjectHome;
use async_trait::async_trait;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Request;
use tddy_service::proto::activity::{ActivityService as _, ReportOperatorNotificationRequest};
use tddy_tool_engine::project_home_notify_tools::{OperatorNotice, ProjectHomeNotifyPort};
use tddy_worktree_service::session_target::HomeCaller;

#[async_trait]
impl ProjectHomeNotifyPort for DaemonProjectHome {
    /// Publish `notice` for the calling home session through this host's
    /// `ActivityService.ReportOperatorNotification`, presenting the hook token the session's own
    /// `.session.yaml` records — the credential `tddy-tools notify` presents from its environment
    /// (`#home-inbox` 11/11). The caller was authenticated by its session token, and its role read from
    /// that record, before this is reached.
    async fn notify_operator(
        &self,
        home: &HomeCaller,
        notice: OperatorNotice,
    ) -> Result<(), String> {
        let session_dir = unified_session_dir_path(&self.sessions_base, &home.session_id);
        let metadata = tddy_core::read_session_metadata(&session_dir)
            .map_err(|e| format!("cannot read session {}'s record ({e})", home.session_id))?;
        let can_notify = matches!(
            metadata.session_type.as_deref(),
            Some("claude-cli" | "cursor-cli")
        );
        let hook_token = metadata
            .hook_token
            .filter(|token| !token.is_empty())
            .filter(|_| can_notify)
            .ok_or_else(|| {
                "this session has no hook token (only claude-cli and cursor-cli sessions can notify)"
                    .to_string()
            })?;
        self.activity
            .report_operator_notification(Request::direct(ReportOperatorNotificationRequest {
                session_id: home.session_id.clone(),
                hook_token,
                os_user: self.os_user.clone(),
                title: notice.title,
                body_markdown: notice.body_markdown,
                level: notice.level as i32,
                sound: notice.sound,
            }))
            .await
            .map(|_| ())
            .map_err(|status| status.message().to_string())
    }
}
