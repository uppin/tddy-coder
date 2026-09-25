use super::DaemonSessionHost;

use std::sync::Arc;

impl DaemonSessionHost {
    /// Start the presenter observer for a freshly spawned workflow session: the injected
    /// presenter-event sink (Telegram's surface) when this daemon has one, and — when it has a bus
    /// and can resolve `os_user`'s sessions directory to read the session's label from — the
    /// notification publish that raises its indicator.
    ///
    /// The two are independent. Gating the observer on Telegram would leave a workflow session's
    /// drawer dot permanently still on every daemon without a `telegram:` block, which is most of
    /// them; `spawn_presenter_observer_task` declines only when *neither* sink exists.
    pub(crate) fn maybe_spawn_presenter_observer(
        &self,
        os_user: &str,
        session_id: &str,
        grpc_port: u16,
    ) {
        let publishing = self.session_notification_bus.as_ref().and_then(|bus| {
                    match crate::user_sessions_path::sessions_base_for_user(
                        os_user,
                        Some(&self.tddy_data_dir),
                    ) {
                        Some(sessions_base) => {
                            Some(crate::session_notifications::SessionNotificationPublishing {
                                bus: Arc::clone(bus),
                                sessions_base,
                                os_user: os_user.to_string(),
                            })
                        }
                        None => {
                            log::warn!(
                                "presenter observer for session {session_id}: no sessions base for os_user — its notifications will not be published"
                            );
                            None
                        }
                    }
                });
        crate::presenter_observer_task::spawn_presenter_observer_task(
            self.presenter_event_sink.clone(),
            publishing,
            session_id,
            grpc_port,
        );
    }
}
