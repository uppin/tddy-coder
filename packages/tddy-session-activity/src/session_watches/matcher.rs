use crate::project_notification_log::ProjectNotificationRecord;
use serde::{Deserialize, Serialize};
use tddy_daemon_kernel::session_notification::SessionNotificationSource;

/// One kind of event a watch can ask for, over the log's records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchKind {
    /// A session asks a question: a pending `AskUserQuestion`, or a presenter's elicitation.
    Question,
    /// A session asks permission to run a tool.
    Permission,
    /// A session finished its turn (`Done`).
    TurnFinished,
    /// A session waits for its next prompt (`WaitingForInput` with no question).
    WaitingForInput,
    /// A session's agent wrote its operator (`tddy-tools notify`).
    AgentNotification,
    /// A session's agent ended (`#activity-wake` 1/3).
    Ended,
    /// A session sent a message — to the orchestrator itself or to another session
    /// (`#activity-wake` 5/5). Selected by sender.
    Message,
}

impl WatchKind {
    /// Every kind, in the order the tools list them.
    pub const ALL: [WatchKind; 7] = [
        WatchKind::Question,
        WatchKind::Permission,
        WatchKind::TurnFinished,
        WatchKind::WaitingForInput,
        WatchKind::AgentNotification,
        WatchKind::Ended,
        WatchKind::Message,
    ];

    /// The kind's name, as the tools take it.
    pub fn as_str(self) -> &'static str {
        match self {
            WatchKind::Question => "question",
            WatchKind::Permission => "permission",
            WatchKind::TurnFinished => "turn_finished",
            WatchKind::WaitingForInput => "waiting_for_input",
            WatchKind::AgentNotification => "agent_notification",
            WatchKind::Ended => "ended",
            WatchKind::Message => "message",
        }
    }

    /// The kind named `name`, or `None` for a name no kind has.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }

    /// The kind `record` is, or `None` for a record no watch can ask for — among them an
    /// activity-status record carrying a question, which is the same prompt its
    /// `pending_question` record already announced.
    pub fn of(record: &ProjectNotificationRecord) -> Option<Self> {
        match record.source {
            SessionNotificationSource::SessionEnded => Some(Self::Ended),
            SessionNotificationSource::Agent => Some(Self::AgentNotification),
            SessionNotificationSource::Presenter => Some(Self::Question),
            SessionNotificationSource::PendingQuestion => {
                match record.question.as_ref().map(|q| q.kind.as_str()) {
                    Some("permission") => Some(Self::Permission),
                    _ => Some(Self::Question),
                }
            }
            SessionNotificationSource::ActivityStatus => {
                if record.question.is_some() {
                    return None;
                }
                match record.activity_status.as_deref() {
                    Some("Done") => Some(Self::TurnFinished),
                    Some("WaitingForInput") => Some(Self::WaitingForInput),
                    _ => None,
                }
            }
            SessionNotificationSource::AgentToolCall => None,
            SessionNotificationSource::Message => Some(Self::Message),
        }
    }
}
