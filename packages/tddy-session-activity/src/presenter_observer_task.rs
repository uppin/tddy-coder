//! gRPC client: subscribe to a child `tddy-coder`'s [`PresenterObserver`] and drive the two places
//! that care about its events — the daemon's [`OperatorChannel`] (Telegram's chat surface, on a
//! daemon that has one) and the session-notification bus.
//!
//! [`PresenterObserver`]: tddy_service::gen::presenter_observer_client::PresenterObserverClient
//! [`OperatorChannel`]: tddy_daemon_kernel::operator_channel::OperatorChannel

pub use crate::presenter_observer_spawn;

use std::time::Duration;

use tonic::transport::Endpoint;

use tddy_daemon_kernel::operator_channel::{NoOperatorChannel, SharedOperatorChannel};
use tddy_service::gen::presenter_observer_client::PresenterObserverClient;
use tddy_service::gen::ObserveRequest;

use crate::session_notification_publishing::SessionNotificationPublishing;
use crate::session_notifications::notification_for_presenter_event;

const OBSERVER_CONNECT_MAX_ATTEMPTS: u32 = 90;
const OBSERVER_RETRY_DELAY_MS: u64 = 100;

/// Spawn a background task that connects to `127.0.0.1:{grpc_port}` and processes a workflow
/// session's presenter events.
///
/// The two are independent. `channel` drives the chat surface — elicitation keyboards, state
/// lines — and is [`NoOperatorChannel`] on a daemon with no `telegram:` block. `notifications`
/// publishes the same events onto the session-notification bus so the session's drawer row shows a
/// dot, and is absent only when the caller has no bus.
///
/// They are separate parameters rather than one bundle because gating the observer on Telegram is
/// how a workflow session's indicator came to be silently unavailable on every Telegram-less
/// daemon — which is most of them. With neither a wired channel nor a bus there is nothing to
/// observe for, so the task is not spawned.
pub fn spawn_presenter_observer_task(
    channel: SharedOperatorChannel,
    notifications: Option<SessionNotificationPublishing>,
    session_id: &str,
    grpc_port: u16,
) {
    if channel.id() == NoOperatorChannel::ID && notifications.is_none() {
        log::debug!(
            target: "tddy_daemon::session_notifications",
            "presenter observer for session {session_id}: neither Telegram nor a notification bus is configured — not observing"
        );
        return;
    }
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        match run_presenter_observer_loop(channel, notifications, session_id, grpc_port).await {
            Ok(()) => {}
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon::telegram",
                    "presenter observer task ended with error: {e}"
                );
            }
        }
    });
}

async fn connect_observer_endpoint(grpc_port: u16) -> anyhow::Result<tonic::transport::Channel> {
    let uri = format!("http://127.0.0.1:{}", grpc_port);
    let mut last_err = None::<String>;
    for attempt in 0..OBSERVER_CONNECT_MAX_ATTEMPTS {
        match Endpoint::from_shared(uri.clone())?.connect().await {
            Ok(ch) => return Ok(ch),
            Err(e) => {
                last_err = Some(e.to_string());
                log::debug!(
                    target: "tddy_daemon::telegram",
                    "presenter observer connect attempt {} to {} failed: {}",
                    attempt + 1,
                    uri,
                    last_err.as_deref().unwrap_or("")
                );
                tokio::time::sleep(Duration::from_millis(OBSERVER_RETRY_DELAY_MS)).await;
            }
        }
    }
    anyhow::bail!(
        "gRPC PresenterObserver connect failed after {} attempts (last_err={})",
        OBSERVER_CONNECT_MAX_ATTEMPTS,
        last_err.unwrap_or_default()
    )
}

async fn run_presenter_observer_loop(
    channel: SharedOperatorChannel,
    notifications: Option<SessionNotificationPublishing>,
    session_id: String,
    grpc_port: u16,
) -> anyhow::Result<()> {
    let grpc = connect_observer_endpoint(grpc_port).await?;
    let mut client = PresenterObserverClient::new(grpc);
    let mut stream = client.observe_events(ObserveRequest {}).await?.into_inner();

    while let Some(result) = stream.message().await? {
        channel.on_presenter_event(&session_id, &result).await?;
        // Published alongside the chat surface rather than through it: the elicitation keyboards
        // stay where they are (the Telegram channel declines `Presenter` notifications), and the
        // indicator gets the same event whether or not a chat is listening.
        if let Some(ref publishing) = notifications {
            publishing
                .publish_for_session(&session_id, |label, os_user| {
                    notification_for_presenter_event(
                        &session_id,
                        os_user,
                        label,
                        &result,
                        tddy_daemon_kernel::now_unix_ms(),
                    )
                })
                .await;
        }
    }
    Ok(())
}
