//! gRPC client: subscribe to a child `tddy-coder`'s [`PresenterObserver`] and drive the two places
//! that care about its events — the daemon's [`OperatorChannel`] (Telegram's chat surface, on a
//! daemon that has one) and the session-notification bus.
//!
//! [`PresenterObserver`]: tddy_service::gen::presenter_observer_client::PresenterObserverClient
//! [`OperatorChannel`]: tddy_daemon_kernel::operator_channel::OperatorChannel

pub use crate::presenter_observer_spawn;

use std::time::Duration;

use tonic::transport::Endpoint;

use tddy_daemon_kernel::operator_channel::{OperatorChannel, SharedOperatorChannel};
use tddy_service::gen::presenter_observer_client::PresenterObserverClient;
use tddy_service::gen::{ObserveRequest, ServerMessage};

use crate::session_notification_publishing::SessionNotificationPublishing;
use crate::session_notifications::notification_for_presenter_event;

const OBSERVER_CONNECT_MAX_ATTEMPTS: u32 = 90;
const OBSERVER_RETRY_DELAY_MS: u64 = 100;

/// Spawn a background task that connects to `127.0.0.1:{grpc_port}` and processes a workflow
/// session's presenter events.
///
/// The two are independent. `channel` drives the chat surface — elicitation keyboards, state
/// lines — and is [`NoOperatorChannel`](tddy_daemon_kernel::operator_channel::NoOperatorChannel) on
/// a daemon with no `telegram:` block. `notifications`
/// publishes the same events onto the session-notification bus so the session's drawer row shows a
/// dot, and is absent only when the caller has no bus.
///
/// They are separate parameters rather than one bundle because gating the observer on Telegram is
/// how a workflow session's indicator came to be silently unavailable on every Telegram-less
/// daemon — which is most of them. With neither a channel that follows presenter events nor a bus
/// there is nothing to observe for, so the task is not spawned.
pub fn spawn_presenter_observer_task(
    channel: SharedOperatorChannel,
    notifications: Option<SessionNotificationPublishing>,
    session_id: &str,
    grpc_port: u16,
) {
    if !presenter_stream_has_a_follower(channel.as_ref(), notifications.is_some()) {
        log::debug!(
            target: "tddy_daemon::session_notifications",
            "presenter observer for session {session_id}: neither an operator channel nor a notification bus follows presenter events — not observing"
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

/// Whether anything would follow a session's presenter stream: a channel that follows presenter
/// events, or a notification bus to publish them onto.
fn presenter_stream_has_a_follower(
    channel: &dyn OperatorChannel,
    publishes_notifications: bool,
) -> bool {
    channel.follows_presenter_events() || publishes_notifications
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
    let mut stream = open_presenter_stream(grpc_port).await?;
    follow_presenter_events(
        &mut stream,
        channel.as_ref(),
        notifications.as_ref(),
        &session_id,
    )
    .await
}

/// A session's presenter events in stream order: `None` once the stream has ended.
#[async_trait::async_trait]
trait PresenterEvents: Send {
    async fn next_event(&mut self) -> anyhow::Result<Option<ServerMessage>>;
}

#[async_trait::async_trait]
impl PresenterEvents for tonic::Streaming<ServerMessage> {
    async fn next_event(&mut self) -> anyhow::Result<Option<ServerMessage>> {
        Ok(self.message().await?)
    }
}

/// Deliver every event of `events` until the stream ends, or until a read or the channel fails.
async fn follow_presenter_events(
    events: &mut dyn PresenterEvents,
    channel: &dyn OperatorChannel,
    notifications: Option<&SessionNotificationPublishing>,
    session_id: &str,
) -> anyhow::Result<()> {
    while let Some(event) = events.next_event().await? {
        deliver_presenter_event(channel, notifications, session_id, &event).await?;
    }
    Ok(())
}

/// The child's presenter event stream, once its gRPC server answers.
async fn open_presenter_stream(grpc_port: u16) -> anyhow::Result<tonic::Streaming<ServerMessage>> {
    let grpc = connect_observer_endpoint(grpc_port).await?;
    let mut client = PresenterObserverClient::new(grpc);
    Ok(client.observe_events(ObserveRequest {}).await?.into_inner())
}

/// Hand one presenter event of `session_id` to the channel, then to the bus when there is one.
///
/// Published alongside the chat surface rather than through it: the elicitation keyboards stay
/// where they are (the Telegram channel declines `Presenter` notifications), and the indicator
/// gets the same event whether or not a chat is listening. A channel error ends the observer, as
/// [`OperatorChannel::on_presenter_event`] says it does.
async fn deliver_presenter_event(
    channel: &dyn OperatorChannel,
    notifications: Option<&SessionNotificationPublishing>,
    session_id: &str,
    event: &ServerMessage,
) -> anyhow::Result<()> {
    channel.on_presenter_event(session_id, event).await?;
    if let Some(publishing) = notifications {
        publishing
            .publish_for_session(session_id, |label, os_user| {
                notification_for_presenter_event(
                    session_id,
                    os_user,
                    label,
                    event,
                    tddy_daemon_kernel::now_unix_ms(),
                )
            })
            .await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::session_notifications::{
        RecordingSessionNotificationSubscriber, SessionNotificationBus, SessionNotificationKind,
        SessionNotificationSource,
    };
    use tddy_daemon_kernel::operator_channel::NoOperatorChannel;
    use tddy_daemon_kernel::session_notification::SessionNotification;
    use tddy_service::gen::app_mode_proto::Variant;
    use tddy_service::gen::server_message::Event;
    use tddy_service::gen::{AppModeProto, AppModeSelect, ModeChanged};

    const SESSION_ID: &str = "01900000-0000-7000-8000-EEFF00000002";
    const OS_USER: &str = "testuser";

    /// A chat channel: it follows every presenter stream it is handed, and records what it got.
    #[derive(Default)]
    struct AChatChannel {
        presenter_events: Mutex<Vec<(String, ServerMessage)>>,
    }

    #[async_trait::async_trait]
    impl OperatorChannel for AChatChannel {
        fn id(&self) -> &'static str {
            "chat"
        }

        async fn on_presenter_event(
            &self,
            session_id: &str,
            event: &ServerMessage,
        ) -> anyhow::Result<()> {
            self.presenter_events
                .lock()
                .unwrap()
                .push((session_id.to_string(), event.clone()));
            Ok(())
        }

        fn wants_notification(&self, _notification: &SessionNotification) -> bool {
            false
        }

        async fn on_notification(&self, _notification: &SessionNotification) -> anyhow::Result<()> {
            Ok(())
        }
    }

    const PUBLISHES_NOTIFICATIONS: bool = true;
    const NO_NOTIFICATION_BUS: bool = false;

    #[test]
    fn with_no_channel_and_no_bus_nothing_follows_the_presenter_stream() {
        // When
        let followed = presenter_stream_has_a_follower(&NoOperatorChannel, NO_NOTIFICATION_BUS);

        // Then
        assert!(!followed);
    }

    #[test]
    fn with_no_channel_the_notification_bus_still_follows_the_presenter_stream() {
        // When
        let followed = presenter_stream_has_a_follower(&NoOperatorChannel, PUBLISHES_NOTIFICATIONS);

        // Then — the bus is what lights a workflow session's drawer dot
        assert!(followed);
    }

    #[test]
    fn a_chat_channel_follows_the_presenter_stream_without_a_bus() {
        // When
        let followed =
            presenter_stream_has_a_follower(&AChatChannel::default(), NO_NOTIFICATION_BUS);

        // Then
        assert!(followed);
    }

    /// A channel that can no longer follow the session it is handed.
    struct AChannelThatGaveUp;

    #[async_trait::async_trait]
    impl OperatorChannel for AChannelThatGaveUp {
        fn id(&self) -> &'static str {
            "gave-up"
        }

        async fn on_presenter_event(
            &self,
            _session_id: &str,
            _event: &ServerMessage,
        ) -> anyhow::Result<()> {
            anyhow::bail!("the chat is gone")
        }

        fn wants_notification(&self, _notification: &SessionNotification) -> bool {
            false
        }

        async fn on_notification(&self, _notification: &SessionNotification) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn a_select_prompt() -> ServerMessage {
        ServerMessage {
            event: Some(Event::ModeChanged(ModeChanged {
                mode: Some(AppModeProto {
                    variant: Some(Variant::Select(AppModeSelect::default())),
                }),
            })),
        }
    }

    /// Publishing onto a bus whose only subscriber is `indicators`, for sessions under an empty
    /// sessions directory — every label falls to the short session id.
    fn publishing_to(
        indicators: &Arc<RecordingSessionNotificationSubscriber>,
        sessions_base: &std::path::Path,
    ) -> SessionNotificationPublishing {
        let bus = SessionNotificationBus::new().with_subscriber(indicators.clone());
        SessionNotificationPublishing {
            bus: Arc::new(bus),
            os_user: OS_USER.to_string(),
            sessions_base: sessions_base.to_path_buf(),
        }
    }

    #[tokio::test]
    async fn a_presenter_event_reaches_the_channel() {
        // Given
        let channel = AChatChannel::default();

        // When
        deliver_presenter_event(&channel, None, SESSION_ID, &a_select_prompt())
            .await
            .expect("the chat channel accepts every event");

        // Then
        assert_eq!(
            *channel.presenter_events.lock().unwrap(),
            vec![(SESSION_ID.to_string(), a_select_prompt())]
        );
    }

    #[tokio::test]
    async fn a_presenter_gate_raises_attention_on_the_bus_with_no_channel_wired() {
        // Given
        let sessions_base = tempfile::tempdir().unwrap();
        let indicators = Arc::new(RecordingSessionNotificationSubscriber::new());
        let publishing = publishing_to(&indicators, sessions_base.path());

        // When
        deliver_presenter_event(
            &NoOperatorChannel,
            Some(&publishing),
            SESSION_ID,
            &a_select_prompt(),
        )
        .await
        .expect("the null channel accepts every event");

        // Then — the drawer dot of a Telegram-less daemon
        let kinds: Vec<_> = indicators
            .received()
            .into_iter()
            .map(|n| (n.session_id, n.os_user, n.kind, n.source))
            .collect();
        assert_eq!(
            kinds,
            vec![(
                SESSION_ID.to_string(),
                OS_USER.to_string(),
                SessionNotificationKind::AttentionRequired,
                SessionNotificationSource::Presenter
            )]
        );
    }

    #[tokio::test]
    async fn a_channel_that_gave_up_ends_the_observer_before_the_bus_hears_of_the_event() {
        // Given
        let sessions_base = tempfile::tempdir().unwrap();
        let indicators = Arc::new(RecordingSessionNotificationSubscriber::new());
        let publishing = publishing_to(&indicators, sessions_base.path());

        // When
        let outcome = deliver_presenter_event(
            &AChannelThatGaveUp,
            Some(&publishing),
            SESSION_ID,
            &a_select_prompt(),
        )
        .await;

        // Then
        assert_eq!(
            (outcome.is_err(), indicators.received()),
            (true, Vec::new())
        );
    }

    /// Presenter events read from a list, then the end of the stream.
    struct AStreamOf(std::collections::VecDeque<ServerMessage>);

    #[async_trait::async_trait]
    impl PresenterEvents for AStreamOf {
        async fn next_event(&mut self) -> anyhow::Result<Option<ServerMessage>> {
            Ok(self.0.pop_front())
        }
    }

    /// A stream whose first read fails, as a child that died mid-stream does.
    struct ABrokenStream;

    #[async_trait::async_trait]
    impl PresenterEvents for ABrokenStream {
        async fn next_event(&mut self) -> anyhow::Result<Option<ServerMessage>> {
            anyhow::bail!("the child went away")
        }
    }

    fn a_running_state() -> ServerMessage {
        ServerMessage {
            event: Some(Event::ModeChanged(ModeChanged {
                mode: Some(AppModeProto {
                    variant: Some(Variant::Running(Default::default())),
                }),
            })),
        }
    }

    #[tokio::test]
    async fn every_event_of_the_stream_reaches_the_channel_in_order() {
        // Given
        let channel = AChatChannel::default();
        let mut events = AStreamOf([a_select_prompt(), a_running_state()].into());

        // When
        follow_presenter_events(&mut events, &channel, None, SESSION_ID)
            .await
            .expect("the stream ends cleanly");

        // Then
        assert_eq!(
            *channel.presenter_events.lock().unwrap(),
            vec![
                (SESSION_ID.to_string(), a_select_prompt()),
                (SESSION_ID.to_string(), a_running_state())
            ]
        );
    }

    #[tokio::test]
    async fn a_failed_read_ends_the_observer_with_the_error() {
        // When
        let outcome =
            follow_presenter_events(&mut ABrokenStream, &NoOperatorChannel, None, SESSION_ID).await;

        // Then
        assert_eq!(
            outcome.map_err(|e| e.to_string()),
            Err("the child went away".to_string())
        );
    }

    #[tokio::test]
    async fn a_channel_that_gave_up_stops_the_stream_at_that_event() {
        // Given
        let mut events = AStreamOf([a_select_prompt(), a_running_state()].into());

        // When
        let outcome =
            follow_presenter_events(&mut events, &AChannelThatGaveUp, None, SESSION_ID).await;

        // Then — the second event is never read
        assert_eq!((outcome.is_err(), events.0.len()), (true, 1));
    }
}
