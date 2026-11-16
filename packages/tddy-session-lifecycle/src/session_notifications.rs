//! How this daemon names and publishes its sessions' notifications.
//!
//! The notification domain itself — the bus, the event, the subscriber trait and the three
//! builders — moved to [`tddy_session_activity::session_notifications`] with `#unbundle` node 7,
//! and is re-exported below so a call site names one module rather than two.
//!
//! What could not go is here: a session's display label is read from
//! [`crate::session_list_enrichment`], which serves `ListSessions` — family C, which stays in the
//! daemon deliberately. [`SessionNotificationPublishing`] is the publish context built on that
//! label, so it stays with it.

pub use tddy_session_activity::session_notifications::*;

pub use session_notification_publishing::*;
pub use tddy_session_activity::session_notification_publishing;

pub use agent_end_notifications::{forward_agent_ends, notification_for_agent_end};

/// A session's agent ending, published as a notification (`#activity-wake` 1/3).
///
/// Here rather than beside the other builders in `tddy-session-activity` because the end is the
/// session manager's ([`crate::cli_session_manager::AgentEnd`]), which that crate does not depend
/// on.
mod agent_end_notifications {
    use std::sync::Arc;

    use tddy_daemon_kernel::SessionsBaseResolver;
    use tokio::sync::broadcast;
    use tokio::task::JoinHandle;

    use super::{
        SessionNotification, SessionNotificationBus, SessionNotificationKind,
        SessionNotificationSource,
    };
    use crate::cli_session_manager::AgentEnd;

    /// The notification `end` raises: attention required, source
    /// [`SessionNotificationSource::SessionEnded`], text `⏹ Session {label} ended: {outcome}`, and
    /// the end itself in `ended`. `os_user` is the owner the end was resolved to.
    pub fn notification_for_agent_end(
        end: &AgentEnd,
        os_user: &str,
        label: &str,
        at_unix_ms: u64,
    ) -> SessionNotification {
        // TODO(session-ended-notice): implement — the text and the `ended` part.
        SessionNotification {
            session_id: end.session_id.clone(),
            os_user: os_user.to_string(),
            label: label.to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::SessionEnded,
            text: String::new(),
            agent: None,
            question: None,
            ended: None,
            at_unix_ms,
        }
    }

    /// Publish every end `ends` announces on `bus`, until the manager is dropped.
    ///
    /// An end names its OS user, or none when its agent ran as the daemon's own user,
    /// `daemon_os_user`. The owner's sessions base comes from `sessions_base_for`, and the label
    /// from the rule the drawer uses. An end whose owner has no sessions base is logged and
    /// dropped: a notification naming no owner would reach nobody. Ends missed by lagging are
    /// counted in a warning.
    pub fn forward_agent_ends(
        ends: broadcast::Receiver<AgentEnd>,
        bus: Arc<SessionNotificationBus>,
        agents: Option<Arc<tddy_coding_agent_specs::CodingAgentRegistry>>,
        sessions_base_for: SessionsBaseResolver,
        daemon_os_user: String,
    ) -> JoinHandle<()> {
        // TODO(session-ended-notice): implement — resolve, label and publish each end.
        let _ = (bus, agents, sessions_base_for, daemon_os_user);
        let mut ends = ends;
        tokio::spawn(async move {
            loop {
                match ends.recv().await {
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        })
    }
    #[cfg(test)]
    mod tests {
        use tddy_daemon_kernel::session_notification::SessionEndNotice;

        use super::*;

        #[test]
        fn an_agent_end_becomes_an_attention_notification_naming_its_outcome() {
            // Given a session's agent that exited with code 3
            let end = AgentEnd {
                session_id: "s-1".to_string(),
                os_user: None,
                outcome: "exit code 3".to_string(),
                exit_code: Some(3),
            };

            // When it is turned into a notification for its owner
            let notification =
                notification_for_agent_end(&end, "ada", "fix-login", 1_790_000_000_000);

            // Then it asks for attention, says how the session ended, and carries the end
            assert_eq!(
                notification,
                SessionNotification {
                    session_id: "s-1".to_string(),
                    os_user: "ada".to_string(),
                    label: "fix-login".to_string(),
                    kind: SessionNotificationKind::AttentionRequired,
                    source: SessionNotificationSource::SessionEnded,
                    text: "⏹ Session fix-login ended: exit code 3".to_string(),
                    agent: None,
                    question: None,
                    ended: Some(SessionEndNotice {
                        outcome: "exit code 3".to_string(),
                        exit_code: Some(3),
                    }),
                    at_unix_ms: 1_790_000_000_000,
                }
            );
        }
    }
}
