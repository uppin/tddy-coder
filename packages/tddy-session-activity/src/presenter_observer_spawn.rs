use std::sync::Arc;

/// What the presenter observer reads of the host that starts it: the data root a session's
/// sessions base is resolved from, the daemon's operator channel and the notification bus.
#[derive(Clone)]
pub struct PresenterObserverDeps {
    pub tddy_data_dir: std::path::PathBuf,
    pub operator_channel: tddy_daemon_kernel::operator_channel::SharedOperatorChannel,
    pub session_notification_bus: Option<Arc<crate::session_notifications::SessionNotificationBus>>,
}

impl PresenterObserverDeps {
    /// Start the presenter observer for a freshly spawned workflow session: the operator channel
    /// (Telegram's surface) when this daemon has one, and — when it has a bus
    /// and can resolve `os_user`'s sessions directory to read the session's label from — the
    /// notification publish that raises its indicator.
    ///
    /// The two are independent. Gating the observer on Telegram would leave a workflow session's
    /// drawer dot permanently still on every daemon without a `telegram:` block, which is most of
    /// them; `spawn_presenter_observer_task` declines only when there is *neither* a channel that
    /// follows presenter events nor a bus.
    pub fn maybe_spawn_presenter_observer(&self, os_user: &str, session_id: &str, grpc_port: u16) {
        let publishing = self.session_notification_bus.as_ref().and_then(|bus| {
                    match crate::user_sessions_path::sessions_base_for_user(
                        os_user,
                        Some(&self.tddy_data_dir),
                    ) {
                        Some(sessions_base) => {
                            Some(crate::session_notification_publishing::SessionNotificationPublishing {
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
            self.operator_channel.clone(),
            publishing,
            session_id,
            grpc_port,
        );
    }
}
