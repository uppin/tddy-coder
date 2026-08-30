//! Session notifications: what happened in a session that an operator should know about.
//!
//! A notification is a domain event, not a message to a chat. The daemon publishes one onto
//! [`SessionNotificationBus`] and each subscriber decides whether it wants it: Telegram sends the
//! attention-worthy ones (`session_notification_subscribers::TelegramNotificationSubscriber`), and
//! the notification stream relays every one of them to connected browsers
//! (`session_notification_subscribers::SessionNotificationStreamSubscriber`), which turn them into
//! the drawer's per-row indicator.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-08-29-session-notifications-as-indicators.md`.
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

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use tokio::sync::broadcast;

use tddy_service::gen::server_message::Event;
use tddy_service::gen::ServerMessage;

use tddy_core::session_label::session_display_label;
use tddy_core::SessionActivityStatus;

/// Whether a notification asks the operator for something, or merely reports progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionNotificationKind {
    /// The agent is working. Drives the drawer's blinking green dot; sent to no chat.
    Activity,
    /// The session is blocked on the operator, or has finished a turn and is waiting for them.
    AttentionRequired,
}

/// Which of the daemon's inbound paths raised a notification.
///
/// Subscribers filter on this: the Telegram subscriber takes the activity-status path only,
/// because the presenter's elicitations already reach Telegram through `telegram_notifier` with
/// their inline keyboards attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionNotificationSource {
    /// `ReportSessionStatus` — the per-worktree claude-cli / cursor-cli hooks.
    ActivityStatus,
    /// `ReportAgentActivity` — the agent's own tool loop.
    AgentToolCall,
    /// `PresenterObserver.ObserveEvents` — a `tddy-coder` workflow session's presenter.
    Presenter,
}

/// One thing that happened in one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionNotification {
    pub session_id: String,
    /// The OS user whose sessions directory this session lives in — the operator it belongs to.
    ///
    /// Server-side only, and deliberately kept off the wire: it is an authorization fact, not
    /// something a browser renders. `StreamSessionNotifications` matches it against the OS user
    /// the subscribing token maps to and relays only what matches, so a daemon serving several
    /// operators never hands one of them another's session ids, repository names or
    /// operator-facing prose. A notification that names no owner belongs to nobody and reaches no
    /// client — the safe answer to an unresolvable owner is to deliver it to no one, never to
    /// everyone.
    pub os_user: String,
    /// The session's display label — [`tddy_core::session_label::session_display_label`], so a
    /// chat message and a drawer row name the same session identically.
    pub label: String,
    pub kind: SessionNotificationKind,
    pub source: SessionNotificationSource,
    /// Operator-facing line. Telegram sends it verbatim; the web shows it as the dot's tooltip.
    pub text: String,
    /// Epoch milliseconds. The web ages activity out of its blink window against this, so it is
    /// the moment the event happened, never the moment a client read it.
    pub at_unix_ms: u64,
}

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
        at_unix_ms,
    })
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
        at_unix_ms,
    }
}

/// A session's display label, read from the same values `ListSessions` reports to the drawer:
/// `repo_path` from `.session.yaml` and `workflow_goal` from the session-list enrichment.
///
/// Reading a different source — the worktree recorded in `changeset.yaml`, say — would name
/// sessions correctly right up until the two disagreed, and the whole point of the shared rule is
/// that the chat and the drawer cannot disagree. A session directory that is missing or unreadable
/// (a hook outracing session creation, a session deleted with a report in flight) falls to the
/// short session id rather than failing: a label is display text, and there is always one.
pub fn resolve_session_label(sessions_base: &Path, session_id: &str) -> String {
    let session_dir = tddy_core::unified_session_dir_path(sessions_base, session_id);

    let repo_path = tddy_core::read_session_metadata(&session_dir)
        .map(|meta| meta.repo_path.unwrap_or_default())
        .unwrap_or_else(|e| {
            log::debug!(
                target: "tddy_daemon::session_notifications",
                "resolve_session_label: no readable .session.yaml for session {session_id}: {e}"
            );
            String::new()
        });

    // The worktree wins outright when there is one, and there is one for most sessions — so ask
    // that first and skip the enrichment entirely. This runs on every reported hook, twice per
    // agent tool call, and the enrichment is a second parse of the file just read plus, for a
    // workflow session, its `changeset.yaml`. Reading a value the rule will not consult is the
    // difference between a cheap label and a per-tool-call file-parse tax.
    if let Some(basename) = tddy_core::session_label::label_from_repo_path(&repo_path) {
        return basename;
    }

    let workflow_goal =
        crate::session_list_enrichment::session_list_status_from_session_dir(&session_dir)
            .map(|status| status.workflow_goal)
            .unwrap_or_else(|e| {
                log::debug!(
                    target: "tddy_daemon::session_notifications",
                    "resolve_session_label: could not enrich session {session_id}: {e}"
                );
                String::new()
            });

    session_display_label(&repo_path, &workflow_goal, session_id)
}

/// The notification a presenter event raises, or `None` for an event that says nothing about
/// whether the session is working or waiting.
///
/// The Telegram subscriber declines these (`source == Presenter`): a workflow session's
/// elicitations already reach a chat through [`crate::telegram_notifier`], keyboards and per-chat
/// FIFO included. What is new here is the indicator — a workflow session's dot goes yellow while
/// the presenter holds a gate, and blinks green while it moves through states.
pub fn notification_for_presenter_event(
    session_id: &str,
    os_user: &str,
    label: &str,
    message: &ServerMessage,
    at_unix_ms: u64,
) -> Option<SessionNotification> {
    let (kind, text) = match message.event.as_ref()? {
        Event::ModeChanged(mode_changed) => {
            if !crate::elicitation::mode_changed_requires_telegram_elicitation(mode_changed) {
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
        at_unix_ms,
    })
}

/// What a publish site needs to raise a notification for a session: the bus, the OS user the
/// session belongs to, and the sessions directory its label is read from.
///
/// The three travel together because they are one fact — *whose* session this is — seen from three
/// sides. A publish without a label source would fall back to the session id and quietly undo the
/// parity FR1 exists for; a publish without an owner would raise a notification no client is
/// allowed to receive.
#[derive(Clone)]
pub struct SessionNotificationPublishing {
    pub bus: Arc<SessionNotificationBus>,
    /// The OS user whose sessions directory `sessions_base` is, and therefore the owner every
    /// notification raised through this context names.
    pub os_user: String,
    pub sessions_base: std::path::PathBuf,
}

impl SessionNotificationPublishing {
    /// Resolve `session_id`'s label and publish `build(label, os_user)`, when it yields a
    /// notification.
    ///
    /// `build` is handed the owner rather than reading it from anywhere else, so a notification
    /// raised here cannot name a user other than the one whose sessions directory its label came
    /// from.
    pub async fn publish_for_session(
        &self,
        session_id: &str,
        build: impl FnOnce(&str, &str) -> Option<SessionNotification>,
    ) {
        let label = resolve_session_label(&self.sessions_base, session_id);
        if let Some(notification) = build(&label, &self.os_user) {
            self.bus.publish(notification).await;
        }
    }
}

/// Per-session dedupe of already-delivered notification text.
///
/// The bus publishes every reported status so an indicator can stay alive; a subscriber that turns
/// each one into a message must suppress the repeats itself. Keyed by session so two sessions
/// reporting the same thing are two notifications, not one.
#[derive(Default)]
pub(crate) struct LastDeliveredPerSession {
    last: StdMutex<HashMap<String, String>>,
}

impl LastDeliveredPerSession {
    /// Records `text` as the session's most recent delivery and answers whether it differs from
    /// the one before it — i.e. whether this notification is worth sending.
    pub(crate) fn record_and_is_new(&self, session_id: &str, text: &str) -> bool {
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
