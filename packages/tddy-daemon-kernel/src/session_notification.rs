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
