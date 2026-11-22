use super::CliSessionManager;
use tddy_terminal_rpc::service::ControlChange as ControlChangeEvent;
use tddy_terminal_rpc::service::ControlClaim as ClaimOutcome;
use tokio::sync::broadcast;

use super::ControlLeaseInfo;

impl CliSessionManager {
    /// Attempt to claim exclusive input control of a session's terminals.
    ///
    /// - `steal = false`: grants only when unheld or already held by `screen_id`.
    /// - `steal = true`: always grants, evicting the previous holder and broadcasting a
    ///   [`ControlChangeEvent`] to all [`Self::subscribe_control`] subscribers.
    ///
    /// Returns a [`ClaimOutcome`] the RPC handler maps to [`ClaimTerminalControlResponse`].
    pub async fn claim_control(
        &self,
        session_id: &str,
        screen_id: &str,
        steal: bool,
    ) -> ClaimOutcome {
        let mut control = self.control.write().await;
        match control.get(session_id) {
            None => {
                let token = uuid::Uuid::new_v4().to_string();
                control.insert(
                    session_id.to_string(),
                    ControlLeaseInfo {
                        control_token: token.clone(),
                        holder_screen_id: screen_id.to_string(),
                    },
                );
                ClaimOutcome::Granted {
                    control_token: token,
                }
            }
            Some(lease) if lease.holder_screen_id == screen_id => ClaimOutcome::Granted {
                control_token: lease.control_token.clone(),
            },
            Some(lease) if !steal => ClaimOutcome::Denied {
                holder_screen_id: lease.holder_screen_id.clone(),
            },
            Some(_) => {
                let token = uuid::Uuid::new_v4().to_string();
                control.insert(
                    session_id.to_string(),
                    ControlLeaseInfo {
                        control_token: token.clone(),
                        holder_screen_id: screen_id.to_string(),
                    },
                );
                drop(control);
                let _ = self.control_tx.send(ControlChangeEvent {
                    session_id: session_id.to_string(),
                    holder_screen_id: screen_id.to_string(),
                });
                ClaimOutcome::Granted {
                    control_token: token,
                }
            }
        }
    }

    /// Return `true` iff `control_token` matches the active control lease for `session_id`.
    ///
    /// An empty `control_token` is accepted when the session has no active lease (uncontrolled).
    /// A session with no active lease is considered uncontrolled: all inputs are accepted.
    pub async fn verify_control(&self, session_id: &str, control_token: &str) -> bool {
        let control = self.control.read().await;
        match control.get(session_id) {
            None => true,
            Some(lease) => lease.control_token == control_token,
        }
    }

    /// Give up `session_id`'s lease when `control_token` is the current one, leaving the session
    /// uncontrolled and broadcasting the change; `false`, and nothing changed, when it is not
    /// (`#orchestra` 7/9).
    pub async fn release_control(&self, session_id: &str, control_token: &str) -> bool {
        let mut control = self.control.write().await;
        match control.get(session_id) {
            Some(lease) if lease.control_token == control_token => {
                control.remove(session_id);
                drop(control);
                // An empty holder is how the change reads "nobody controls it".
                let _ = self.control_tx.send(ControlChangeEvent {
                    session_id: session_id.to_string(),
                    holder_screen_id: String::new(),
                });
                true
            }
            _ => false,
        }
    }

    /// Drop `session_id`'s lease, whoever holds it: the session ended, so nobody controls it.
    pub async fn forget_control(&self, session_id: &str) {
        self.control.write().await.remove(session_id);
    }

    /// Return the current control lease for `session_id`, or `None` if uncontrolled.
    pub async fn current_control(&self, session_id: &str) -> Option<ControlLeaseInfo> {
        let control = self.control.read().await;
        control.get(session_id).cloned()
    }

    /// Subscribe to control-change events across all sessions.
    ///
    /// Each [`ControlChangeEvent`] identifies the affected `session_id` and the new holder's
    /// `holder_screen_id`. The displaced screen should render the "Claim terminal" CTA.
    pub fn subscribe_control(&self) -> broadcast::Receiver<ControlChangeEvent> {
        self.control_tx.subscribe()
    }
}

#[cfg(test)]
mod release_tests {
    use super::*;

    async fn a_held_lease(manager: &CliSessionManager, screen: &str) -> String {
        match manager.claim_control("child", screen, true).await {
            ClaimOutcome::Granted { control_token } => control_token,
            ClaimOutcome::Denied { holder_screen_id } => {
                panic!("denied; held by {holder_screen_id}")
            }
        }
    }

    #[tokio::test]
    async fn the_holder_releases_and_the_session_is_uncontrolled() {
        let manager = CliSessionManager::new();
        let token = a_held_lease(&manager, "project-home:home").await;

        let released = manager.release_control("child", &token).await;

        assert!(released);
        assert!(manager.current_control("child").await.is_none());
    }

    #[tokio::test]
    async fn after_a_release_another_screen_claims_without_stealing() {
        let manager = CliSessionManager::new();
        let token = a_held_lease(&manager, "project-home:home").await;
        manager.release_control("child", &token).await;

        let claim = manager.claim_control("child", "browser-1", false).await;

        assert!(matches!(claim, ClaimOutcome::Granted { .. }), "{claim:?}");
    }

    #[tokio::test]
    async fn a_stale_token_releases_nothing() {
        let manager = CliSessionManager::new();
        let stale = a_held_lease(&manager, "project-home:home").await;
        a_held_lease(&manager, "browser-1").await;

        let released = manager.release_control("child", &stale).await;

        assert!(!released);
        assert_eq!(
            manager
                .current_control("child")
                .await
                .map(|l| l.holder_screen_id),
            Some("browser-1".to_string())
        );
    }

    #[tokio::test]
    async fn forgetting_an_ended_sessions_lease_leaves_it_uncontrolled() {
        let manager = CliSessionManager::new();
        a_held_lease(&manager, "browser-1").await;

        manager.forget_control("child").await;

        assert!(manager.current_control("child").await.is_none());
    }
}
