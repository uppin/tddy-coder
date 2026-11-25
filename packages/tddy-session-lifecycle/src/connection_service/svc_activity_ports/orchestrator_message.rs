//! A target child's report, posted to its orchestrator as a session message (`#orchestra` 2/9).
//!
//! The record is the one `PostSessionMessage` publishes for a same-host child
//! (`tddy_daemon_rpc`'s `DaemonSessionMessages::post_message`): a `message` notification from the
//! child to its orchestrator, published on the host's bus so the project log, the waker and the
//! long polls see it as they see every message. A child's orchestrator is always within its reach,
//! so the addressing rule reduces to reading the orchestrator from the child's own changeset.

use crate::connection_service::DaemonSessionHost;
use tddy_agent_launch::back_channel::RemoteMessage;
use tddy_daemon_kernel::session_notification::SessionMessage;
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
};

impl DaemonSessionHost {
    /// Publish `message` as a session message from its child session to the child's orchestrator.
    /// Refused, naming why, for a host with no notification bus, a child with no orchestrator, an
    /// orchestrator on another daemon (a cross-host report is not carried) and a text
    /// `PostSessionMessage` would refuse.
    pub(super) async fn post_remote_message_to_orchestrator(
        &self,
        message: RemoteMessage,
    ) -> Result<(), String> {
        tddy_session_activity::session_messages::refuse_a_bad_message_text(&message.text)?;
        let bus = self
            .session_notification_bus()
            .ok_or("this host has no session notification bus")?;
        let sessions_base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            &message.os_user,
            Some(&self.tddy_data_dir),
        )
        .ok_or_else(|| format!("no sessions path for {}", message.os_user))?;
        let orchestrator = local_orchestrator_of(&sessions_base, &message.session_id)?;
        let agents = self.coding_agents.agents_shared();
        bus.publish(SessionNotification {
            label: tddy_session_activity::session_notification_publishing::resolve_session_label(
                Some(&agents),
                &sessions_base,
                &message.session_id,
            ),
            session_id: message.session_id,
            os_user: message.os_user,
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::Message,
            text: message.text.clone(),
            agent: None,
            question: None,
            ended: None,
            activity_status: None,
            message: Some(SessionMessage {
                to_session_id: orchestrator,
                message_id: format!("m-{}", uuid::Uuid::new_v4()),
                reply_to: None,
                text: message.text,
                to_daemon_instance_id: None,
            }),
            at_unix_ms: tddy_daemon_kernel::now_unix_ms(),
        })
        .await;
        Ok(())
    }
}

/// The orchestrator `session_id`'s changeset names, on this daemon. Refused, naming why, for a
/// changeset that cannot be read, a session with no orchestrator, and one whose orchestrator runs
/// on another daemon (a cross-host report is not carried).
fn local_orchestrator_of(
    sessions_base: &std::path::Path,
    session_id: &str,
) -> Result<String, String> {
    let child_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    let changeset = tddy_core::read_changeset(&child_dir)
        .map_err(|e| format!("cannot read session {session_id}'s changeset: {e}"))?;
    let orchestrator = changeset
        .orchestrator_session_id
        .filter(|orchestrator| !orchestrator.trim().is_empty())
        .ok_or_else(|| format!("session {session_id} has no orchestrator"))?;
    if changeset
        .orchestrator_daemon_instance_id
        .is_some_and(|host| !host.trim().is_empty())
    {
        return Err(format!(
            "session {session_id}'s orchestrator {orchestrator} runs on another daemon, which a \
             report does not reach"
        ));
    }
    Ok(orchestrator)
}
