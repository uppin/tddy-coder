//! One thing that happened in one session, as the session-notification bus carries it.
//!
//! The kernel's rather than the bus's (`tddy_session_activity::session_notifications`, which
//! re-exports these types) so the operator-channel port can name it.

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
/// Subscribers filter on this: Telegram's operator channel takes the activity-status path only,
/// because the presenter's elicitations already reach it through
/// [`OperatorChannel::on_presenter_event`](crate::operator_channel::OperatorChannel::on_presenter_event)
/// with their inline keyboards attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionNotificationSource {
    /// `ReportSessionStatus` — the per-worktree claude-cli / cursor-cli hooks.
    ActivityStatus,
    /// `ReportAgentActivity` — the agent's own tool loop.
    AgentToolCall,
    /// `PresenterObserver.ObserveEvents` — a `tddy-coder` workflow session's presenter.
    Presenter,
    /// `ReportOperatorNotification` — the agent's own words, sent with `tddy-tools notify`. Carries
    /// an [`AgentNotification`] (`#home-inbox` 1/8).
    Agent,
}

/// How an agent grades what it is telling its operator. Chosen by the agent (`tddy-tools notify
/// --level`); a surface may colour or iconify by it, and none of them filters on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationLevel {
    /// Neutral news — the default when the agent names no level.
    #[default]
    Info,
    /// Something the operator asked for has finished well.
    Success,
    /// Something needs a look, but nothing has stopped.
    Warning,
    /// Something failed or is blocked.
    Error,
}

/// What an agent wrote for its operator: the structured content of a
/// [`SessionNotificationSource::Agent`] notification.
///
/// Carried beside [`SessionNotification::text`] rather than folded into it, because each surface
/// renders it differently — Telegram turns the markdown body into its HTML subset, a desktop
/// notification shows it as plain text — and none of them can recover structure from a flattened
/// line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentNotification {
    /// One line, trimmed, 1–200 characters
    /// ([`crate::agent_notification_limits::validate_agent_notification`]).
    pub title: String,
    /// Markdown, at most 2,000 characters; empty when the agent sent a title alone.
    pub body_markdown: String,
    pub level: NotificationLevel,
    /// Whether the agent asked for an audible alert. Telegram sends a message without it silently.
    pub sound: bool,
    /// The session's web route, `#/sessions/<id>?host=<daemon instance>`.
    pub link: String,
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
    /// What the agent wrote, for a [`SessionNotificationSource::Agent`] notification; `None` for
    /// every other source, whose `text` is the whole of what they say.
    pub agent: Option<AgentNotification>,
    /// Epoch milliseconds. The web ages activity out of its blink window against this, so it is
    /// the moment the event happened, never the moment a client read it.
    pub at_unix_ms: u64,
}
