use std::sync::Arc;

use tddy_rpc::Status;
use tokio::sync::broadcast;

use crate::bridge::resolved_terminal_id;
use crate::proto::terminal_session::TerminalControlEvent;
use crate::service::TerminalSessionServiceImpl;
use crate::service::{ControlChange, TerminalControl};

/// Forward the lease changes of one session as `TerminalControlEvent`s.
///
/// `you_are_controller` is re-derived from the watcher's own token on every change rather than
/// inferred from the holder id: the two disagree exactly when this screen is the one being
/// displaced, which is the case the CTA exists for.
pub(super) async fn relay_control_changes(
    session_id: String,
    control_token: String,
    control: Arc<dyn TerminalControl>,
    mut changes: broadcast::Receiver<ControlChange>,
    tx: tokio::sync::mpsc::Sender<Result<TerminalControlEvent, Status>>,
) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match changes.recv().await {
            Ok(change) if change.session_id == session_id => {
                let you_are_controller = control.verify(&session_id, &control_token).await;
                let event = TerminalControlEvent {
                    holder_screen_id: change.holder_screen_id,
                    you_are_controller,
                };
                if tx.send(Ok(event)).await.is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

/// How many control events are buffered for a `WatchTerminalControl` subscriber before the relay
/// waits on it.
///
/// A lease changes when a human clicks "Claim terminal", so a screen that cannot keep up with
/// sixteen of those is not reading its stream at all — and making the relay wait is the right
/// answer there, rather than growing a queue nobody drains.
pub(super) const CONTROL_EVENT_CHANNEL_CAPACITY: usize = 16;

impl TerminalSessionServiceImpl {
    /// Whether an open may resize the PTY to the grid it measured.
    ///
    /// Opening a terminal is watching, so it is served without a lease — but the grid it carries
    /// resizes the PTY, and that is driving: a phone merely looking at a session a desktop drives
    /// would otherwise reflow the desktop's terminal to the phone's width. So the grid is honoured
    /// under the same rule as input ([`Self::controlled_terminal`]), and a watcher's open is served
    /// at whatever size the driving screen set.
    pub(super) async fn may_resize_on_open(
        &self,
        session_id: &str,
        terminal_id: &str,
        control_token: &str,
    ) -> bool {
        let terminal_id = resolved_terminal_id(terminal_id);
        let gated = self
            .ports
            .terminals
            .get_terminal(session_id, terminal_id)
            .await
            .is_some_and(|session| session.requires_control());
        !gated || self.ports.control.verify(session_id, control_token).await
    }

    /// A `verify_control` closure for the bidi bridge, which re-checks every subsequent input chunk
    /// so a screen that loses the lease mid-stream stops being able to type.
    pub(super) fn control_verifier(
        &self,
    ) -> impl Fn(&str, &str) -> ControlVerification + Send + Sync + use<> {
        let control = Arc::clone(&self.ports.control);
        move |session_id: &str, control_token: &str| {
            let control = Arc::clone(&control);
            let session_id = session_id.to_string();
            let control_token = control_token.to_string();
            Box::pin(async move { control.verify(&session_id, &control_token).await })
        }
    }
}

/// The future [`TerminalSessionServiceImpl::control_verifier`] hands the bridge. Boxed because the
/// closure it comes from is returned from a method, so its `async move` body has no nameable type.
type ControlVerification = std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>>;
