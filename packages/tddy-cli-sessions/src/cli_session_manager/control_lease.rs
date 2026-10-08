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
