//! An operator channel on the session-notification bus, and the drawer indicator on a daemon that
//! has no channel at all.
//!
//! PRD `docs/ft/daemon/telegram-notifications.md`. The
//! channel decides what it wants; the bus asks before it delivers, so a channel that sends over a
//! network is never handed what it would only discard.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_daemon_kernel::operator_channel::{OperatorChannel, SharedOperatorChannel};
use tddy_service::gen::app_mode_proto::Variant;
use tddy_service::gen::server_message::Event;
use tddy_service::gen::{AppModeProto, AppModeSelect, ModeChanged, ServerMessage};
use tddy_session_activity::session_notifications::{
    notification_for_presenter_event, OperatorChannelSubscriber, SessionNotification,
    SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
    SessionNotificationSubscriber,
};

const SESSION_ID: &str = "01900000-0000-7000-8000-EEFF00000001";
const OS_USER: &str = "testuser";
const LABEL: &str = "my-feature-branch";
const AT: u64 = 1_756_000_000_000;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

fn an_attention_notification() -> SessionNotification {
    SessionNotification {
        session_id: SESSION_ID.to_string(),
        os_user: OS_USER.to_string(),
        label: LABEL.to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: format!("🔔 Session {LABEL}: Claude Code needs your input."),
        agent: None,
        at_unix_ms: AT,
        question: None,
    }
}

fn an_activity_notification() -> SessionNotification {
    SessionNotification {
        kind: SessionNotificationKind::Activity,
        text: format!("Session {LABEL}: agent is working"),
        ..an_attention_notification()
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

/// A channel that wants attention notifications only, and records what it was handed.
#[derive(Default)]
struct AnAttentionOnlyChannel {
    delivered: Mutex<Vec<SessionNotification>>,
}

impl AnAttentionOnlyChannel {
    fn delivered(&self) -> Vec<SessionNotification> {
        self.delivered.lock().unwrap().clone()
    }
}

#[async_trait]
impl OperatorChannel for AnAttentionOnlyChannel {
    fn id(&self) -> &'static str {
        "attention-only"
    }

    async fn on_presenter_event(
        &self,
        _session_id: &str,
        _event: &ServerMessage,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn wants_notification(&self, notification: &SessionNotification) -> bool {
        notification.kind == SessionNotificationKind::AttentionRequired
    }

    async fn on_notification(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        self.delivered.lock().unwrap().push(notification.clone());
        Ok(())
    }
}

fn a_bus_with(channel: &Arc<AnAttentionOnlyChannel>) -> SessionNotificationBus {
    let shared: SharedOperatorChannel = channel.clone();
    SessionNotificationBus::new().with_subscriber(Arc::new(OperatorChannelSubscriber::new(shared)))
}

// ---------------------------------------------------------------------------
// The channel on the bus
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_channel_on_the_bus_receives_the_notification_it_wants() {
    // Given
    let channel = Arc::new(AnAttentionOnlyChannel::default());
    let bus = a_bus_with(&channel);

    // When
    bus.publish(an_attention_notification()).await;

    // Then
    assert_eq!(channel.delivered(), vec![an_attention_notification()]);
}

#[tokio::test]
async fn a_channel_on_the_bus_is_not_handed_a_notification_it_declines() {
    // Given
    let channel = Arc::new(AnAttentionOnlyChannel::default());
    let bus = a_bus_with(&channel);

    // When
    bus.publish(an_activity_notification()).await;

    // Then
    assert_eq!(channel.delivered(), Vec::<SessionNotification>::new());
}

#[test]
fn the_subscriber_is_named_after_its_channel() {
    // Given
    let channel: SharedOperatorChannel = Arc::new(AnAttentionOnlyChannel::default());

    // When
    let subscriber = OperatorChannelSubscriber::new(channel);

    // Then — the bus's delivery-failure log line names the channel that failed
    assert_eq!(subscriber.name(), "attention-only");
}

// ---------------------------------------------------------------------------
// The drawer indicator needs no channel
// ---------------------------------------------------------------------------

/// The presenter-gate rule is the core's: this crate builds without any Telegram crate, and a
/// workflow session waiting on a select still asks for the operator's attention.
#[test]
fn a_select_prompt_raises_attention_with_no_channel_wired() {
    // When
    let notification =
        notification_for_presenter_event(SESSION_ID, OS_USER, LABEL, &a_select_prompt(), AT)
            .expect("a select prompt is something to raise a dot for");

    // Then
    assert_eq!(
        (notification.kind, notification.source),
        (
            SessionNotificationKind::AttentionRequired,
            SessionNotificationSource::Presenter
        )
    );
}
