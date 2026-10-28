//! Session notifications: what happened in a session that an operator should know about.
//!
//! A notification is a domain event, not a message to a chat. A publish site raises one onto
//! [`SessionNotificationBus`] and each subscriber decides whether it wants it: the notification
//! stream relays every one of them to connected browsers
//! ([`crate::session_notification_subscribers::SessionNotificationStreamSubscriber`]), which turn
//! them into the drawer's per-row indicator, and the daemon's operator channel (Telegram, when one
//! is configured) is registered through [`OperatorChannelSubscriber`] and sends the
//! attention-worthy ones to a chat.
//!
//! PRD: `docs/ft/daemon/session-notifications.md`.
//!
//! Two rules hold everywhere in here:
//!
//! - **The bus owns the copy.** [`SessionNotification::text`] is finished operator-facing prose; a
//!   subscriber delivers it verbatim rather than re-rendering it, so every surface reads the same
//!   sentence.
//! - **Publishing never fails a hook** (PRD NFR3). A publish is a side effect of an RPC that must
//!   still answer `ok`, so a subscriber's error is logged and swallowed here.
//!
//! Secrets never enter a notification: `text` is built from the session's label and a fixed
//! sentence, and nothing in this module reads a bot token or a session token (PRD NFR4).
//!
//! # What stayed in `tddy-daemon`
//!
//! Resolving a session id to the label a notification names it by reads the session-list
//! enrichment, and `ListSessions` — the family that enrichment serves — stays in the daemon. So
//! `tddy_daemon::session_notifications` keeps `resolve_session_label` and the
//! `SessionNotificationPublishing` context built on it, and re-exports everything declared here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use tokio::sync::broadcast;

use tddy_service::gen::server_message::Event;
use tddy_service::gen::ServerMessage;

use tddy_core::SessionActivityStatus;
use tddy_daemon_kernel::operator_channel::{awaits_operator, SharedOperatorChannel};

/// The notification itself is the kernel's, so the operator-channel port can name it; it is
/// re-exported here under the paths every publish site and subscriber already uses.
pub use tddy_daemon_kernel::session_notification::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
};

/// A consumer of session notifications.
///
/// `wants` is asked before `deliver`, so a subscriber that sends over a network is never handed an
/// event it would only discard.
#[async_trait]
pub trait SessionNotificationSubscriber: Send + Sync {
    /// Short identifier for log lines.
    fn name(&self) -> &'static str;

    /// Whether this subscriber wants `notification`.
    fn wants(&self, notification: &SessionNotification) -> bool;

    /// Deliver `notification`. An error is logged by the bus and never propagated to the RPC that
    /// published (PRD NFR3).
    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()>;

    /// A receiver on this subscriber's fan-out channel, for the subscriber that exists to relay
    /// notifications to connected clients. `None` — the default — for subscribers that deliver
    /// somewhere else entirely: a Telegram send has nothing to hand a browser.
    fn client_relay(&self) -> Option<broadcast::Receiver<SessionNotification>> {
        None
    }
}

/// Fans one publish out to every subscriber that wants it, in registration order.
pub struct SessionNotificationBus {
    subscribers: Vec<Arc<dyn SessionNotificationSubscriber>>,
}

impl Default for SessionNotificationBus {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionNotificationBus {
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }

    /// Register `subscriber` (builder). Delivery order is registration order.
    ///
    /// Generic over the concrete subscriber rather than taking an
    /// `Arc<dyn SessionNotificationSubscriber>`: a caller holding its own `Arc<MySubscriber>` — as
    /// every caller that wants to inspect its subscriber afterwards does — can pass a clone of it
    /// directly, instead of having to name the trait object at the call site.
    pub fn with_subscriber<S: SessionNotificationSubscriber + 'static>(
        mut self,
        subscriber: Arc<S>,
    ) -> Self {
        self.subscribers.push(subscriber);
        self
    }

    /// Offer `notification` to every interested subscriber, awaiting each in turn.
    ///
    /// A failing subscriber costs only its own delivery: the error is logged and the remaining
    /// subscribers are still offered the event (PRD NFR3).
    pub async fn publish(&self, notification: SessionNotification) {
        for subscriber in &self.subscribers {
            if !subscriber.wants(&notification) {
                continue;
            }
            if let Err(e) = subscriber.deliver(&notification).await {
                log::warn!(
                    target: "tddy_daemon::session_notifications",
                    "subscriber {} failed to deliver a {:?} notification for session {}: {e:#}",
                    subscriber.name(),
                    notification.kind,
                    notification.session_id
                );
            }
        }
    }

    /// A receiver on the first registered subscriber that relays to connected clients, or `None`
    /// when this bus has no such subscriber.
    pub fn subscribe_clients(&self) -> Option<broadcast::Receiver<SessionNotification>> {
        self.subscribers
            .iter()
            .find_map(|subscriber| subscriber.client_relay())
    }
}

/// An operator channel on the bus: it wants what the channel wants, and delivers through it.
///
/// The channel is the kernel's port ([`tddy_daemon_kernel::operator_channel::OperatorChannel`]), so
/// a chat integration implements one trait for both of its inbound paths — presenter events and
/// these notifications — and never this crate's subscriber trait.
pub struct OperatorChannelSubscriber {
    channel: SharedOperatorChannel,
}

impl OperatorChannelSubscriber {
    pub fn new(channel: SharedOperatorChannel) -> Self {
        Self { channel }
    }
}

#[async_trait]
impl SessionNotificationSubscriber for OperatorChannelSubscriber {
    /// The channel's id, so the bus's delivery-failure log line names the channel that failed.
    fn name(&self) -> &'static str {
        self.channel.id()
    }

    fn wants(&self, notification: &SessionNotification) -> bool {
        self.channel.wants_notification(notification)
    }

    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        self.channel.on_notification(notification).await
    }
}

/// A subscriber that records what it was offered — the library's own test double, kept beside the
/// bus (as `InMemoryTelegramSender` is kept beside the Telegram sender) so a spec in any crate can
/// assert what a publish reached without writing its own.
pub struct RecordingSessionNotificationSubscriber {
    /// `None` wants every notification; `Some(kind)` wants that kind only.
    wanted_kind: Option<SessionNotificationKind>,
    received: StdMutex<Vec<SessionNotification>>,
}

impl Default for RecordingSessionNotificationSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingSessionNotificationSubscriber {
    /// Records every notification published.
    pub fn new() -> Self {
        Self {
            wanted_kind: None,
            received: StdMutex::new(Vec::new()),
        }
    }

    /// Records notifications of `kind` only, and declines the rest.
    pub fn wanting_only(kind: SessionNotificationKind) -> Self {
        Self {
            wanted_kind: Some(kind),
            received: StdMutex::new(Vec::new()),
        }
    }

    /// Everything delivered so far, in delivery order.
    pub fn received(&self) -> Vec<SessionNotification> {
        self.received
            .lock()
            .expect("RecordingSessionNotificationSubscriber mutex")
            .clone()
    }
}

#[async_trait]
impl SessionNotificationSubscriber for RecordingSessionNotificationSubscriber {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn wants(&self, notification: &SessionNotification) -> bool {
        match self.wanted_kind {
            None => true,
            Some(kind) => notification.kind == kind,
        }
    }

    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        self.received
            .lock()
            .expect("RecordingSessionNotificationSubscriber mutex")
            .push(notification.clone());
        Ok(())
    }
}

/// The notification a reported activity status raises, or `None` when the status carries nothing
/// an operator needs to see.
///
/// `Ended` raises nothing: a session that has ended is grey on liveness alone, and a notification
/// would only blink a dead row. An unrecognised status raises nothing either — `ReportSessionStatus`
/// rejects those before they reach here, and inventing a notification for one would put an
/// unclassified event on an operator's screen.
pub fn notification_for_activity_status(
    session_id: &str,
    os_user: &str,
    label: &str,
    status: &str,
    at_unix_ms: u64,
) -> Option<SessionNotification> {
    let (kind, text) = match SessionActivityStatus::from_wire(status)? {
        SessionActivityStatus::WaitingForInput => (
            SessionNotificationKind::AttentionRequired,
            format!(
                "🔔 Session {label}: Claude Code needs your input (permission, question, or your next prompt). Attach via the web UI or `tddy-tools pty-relay`."
            ),
        ),
        SessionActivityStatus::Done => (
            SessionNotificationKind::AttentionRequired,
            format!("✅ Session {label}: Claude Code finished this turn. Attach to continue."),
        ),
        SessionActivityStatus::Started => (
            SessionNotificationKind::Activity,
            format!("Session {label}: agent started"),
        ),
        SessionActivityStatus::Running => (
            SessionNotificationKind::Activity,
            format!("Session {label}: agent is working"),
        ),
        SessionActivityStatus::ExecutingTool => (
            SessionNotificationKind::Activity,
            format!("Session {label}: agent is running a tool"),
        ),
        SessionActivityStatus::Ended => return None,
    };

    Some(SessionNotification {
        session_id: session_id.to_string(),
        os_user: os_user.to_string(),
        label: label.to_string(),
        kind,
        source: SessionNotificationSource::ActivityStatus,
        text,
        agent: None,
        at_unix_ms,
        question: None,
    })
}

/// The notification a claude-cli session's pending question raises: one per question, attention
/// required, and carrying what is asked (`#home-inbox` 3/8). The text is the sentence a drawer
/// tooltip shows; a chat is sent [`QuestionAlert`](tddy_telegram_control) instead.
pub fn notification_for_pending_question(
    session_id: &str,
    os_user: &str,
    label: &str,
    pending: &tddy_core::pending_question::PendingQuestion,
    at_unix_ms: u64,
) -> SessionNotification {
    let text = match pending.tool_name.as_str() {
        tool if tool == tddy_core::ASK_USER_QUESTION_TOOL => {
            format!("🔔 Session {label} asks: {}", pending.summary())
        }
        tool => format!(
            "🔔 Session {label} wants to run {tool}: {}",
            pending.summary()
        ),
    };
    SessionNotification {
        session_id: session_id.to_string(),
        os_user: os_user.to_string(),
        label: label.to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::PendingQuestion,
        text,
        agent: None,
        at_unix_ms,
        question: Some(crate::pending_question_notice::question_notice_for(pending)),
    }
}

/// The notification the agent's own tool call raises. Always activity: a tool call is the agent
/// working, and no chat is disturbed by one.
pub fn notification_for_agent_tool_call(
    session_id: &str,
    os_user: &str,
    label: &str,
    tool_name: &str,
    at_unix_ms: u64,
) -> SessionNotification {
    SessionNotification {
        session_id: session_id.to_string(),
        os_user: os_user.to_string(),
        label: label.to_string(),
        kind: SessionNotificationKind::Activity,
        source: SessionNotificationSource::AgentToolCall,
        text: format!("Session {label}: {tool_name}"),
        agent: None,
        at_unix_ms,
        question: None,
    }
}

/// The notification a presenter event raises, or `None` for an event that says nothing about
/// whether the session is working or waiting.
///
/// The Telegram channel declines these (`source == Presenter`): a workflow session's
/// elicitations already reach a chat through the channel's own presenter path, keyboards and
/// per-chat FIFO included. What is new here is the indicator — a workflow session's dot goes
/// yellow while the presenter holds a gate, and blinks green while it moves through states.
pub fn notification_for_presenter_event(
    session_id: &str,
    os_user: &str,
    label: &str,
    message: &ServerMessage,
    at_unix_ms: u64,
) -> Option<SessionNotification> {
    let (kind, text) = match message.event.as_ref()? {
        Event::ModeChanged(mode_changed) => {
            if !awaits_operator(mode_changed) {
                // An autonomous mode (`Running`, `Done`) — the presenter changed screens, which is
                // not by itself something to raise a dot for.
                return None;
            }
            (
                SessionNotificationKind::AttentionRequired,
                format!("🔔 Session {label}: waiting for your answer."),
            )
        }
        Event::StateChanged(state_changed) => (
            SessionNotificationKind::Activity,
            format!(
                "Session {label}: {} -> {}",
                state_changed.from, state_changed.to
            ),
        ),
        Event::GoalStarted(goal_started) => (
            SessionNotificationKind::Activity,
            format!("Session {label}: goal started: {}", goal_started.goal),
        ),
        Event::BackendSelected(backend) => (
            SessionNotificationKind::Activity,
            format!(
                "Session {label}: using {} ({})",
                backend.agent, backend.model
            ),
        ),
        _ => return None,
    };

    Some(SessionNotification {
        session_id: session_id.to_string(),
        os_user: os_user.to_string(),
        label: label.to_string(),
        kind,
        source: SessionNotificationSource::Presenter,
        text,
        agent: None,
        at_unix_ms,
        question: None,
    })
}

/// Per-session dedupe of already-delivered notification text.
///
/// The bus publishes every reported status so an indicator can stay alive; a subscriber that turns
/// each one into a message must suppress the repeats itself. Keyed by session so two sessions
/// reporting the same thing are two notifications, not one.
///
/// `pub` rather than the `pub(crate)` it carried inside `tddy-daemon`: what needs it is the
/// Telegram operator channel, which lives in `tddy-telegram-control` and keeps one per daemon on
/// its hooks. A crate boundary runs between the two.
#[derive(Default)]
pub struct LastDeliveredPerSession {
    last: StdMutex<HashMap<String, String>>,
}

impl LastDeliveredPerSession {
    /// Records `text` as the session's most recent delivery and answers whether it differs from
    /// the one before it — i.e. whether this notification is worth sending.
    pub fn record_and_is_new(&self, session_id: &str, text: &str) -> bool {
        let mut last = match self.last.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                // A poisoned dedupe map would otherwise suppress every later notification for
                // every session; recovering it costs at most one repeated message.
                log::warn!(
                    target: "tddy_daemon::session_notifications",
                    "notification dedupe map was poisoned; recovering it"
                );
                poisoned.into_inner()
            }
        };
        let previous = last.insert(session_id.to_string(), text.to_string());
        previous.as_deref() != Some(text)
    }
}
