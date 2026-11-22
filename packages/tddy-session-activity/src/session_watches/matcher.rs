use crate::project_notification_log::ProjectNotificationRecord;
use crate::session_watches::SessionWatch;
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
    /// A commit landed on the project's remote trunk, or a force-push rewrote it (`#orchestra`
    /// 5/9). The orchestrator's own activity: the one kind a watch may ask for on its owner's own
    /// session.
    TrunkCommit,
}

impl WatchKind {
    /// Every kind, in the order the tools list them.
    pub const ALL: [WatchKind; 8] = [
        WatchKind::Question,
        WatchKind::Permission,
        WatchKind::TurnFinished,
        WatchKind::WaitingForInput,
        WatchKind::AgentNotification,
        WatchKind::Ended,
        WatchKind::Message,
        WatchKind::TrunkCommit,
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
            WatchKind::TrunkCommit => "trunk_commit",
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
            SessionNotificationSource::TrunkCommit => Some(Self::TrunkCommit),
        }
    }
}

/// Which records a watch matches: those of `session_ids`, or — with `started_by_me` — of the
/// sessions its owner started; and of `kinds` only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchFilter {
    pub session_ids: Vec<String>,
    pub started_by_me: bool,
    pub kinds: Vec<WatchKind>,
}

/// Whether `watch` matches `record`, `owner_inbox_position` being the `seq` up to which the
/// watch's owner has read the messages addressed to it. Its session is named, or it started the
/// session, and the record is of a kind it asks for, and it is never the owner's own session —
/// bar the owner's own trunk activity. The record's project and OS user are the log read's, which
/// is the owner's.
///
/// A **message** record matches by who it is addressed to and who sent it: an inbox watch
/// ([`WatchFilter::is_inbox_of`] its owner) matches the messages addressed to its owner, whoever
/// sent them; any other watch matches by sender, through the session selection. And a message
/// addressed to the owner that the owner's inbox position has passed matches no watch — it was
/// read already, and it is delivered once (`crate::session_messages`).
pub(super) fn watch_matches(
    watch: &SessionWatch,
    record: &ProjectNotificationRecord,
    owner_inbox_position: u64,
) -> bool {
    // A watch never matches its owner's own session — except for the owner's trunk activity, which
    // is the orchestrator's own and exactly what its self-watch asks for.
    if record.session_id == watch.owner.session_id
        && record.source != SessionNotificationSource::TrunkCommit
    {
        return false;
    }
    if record.source == SessionNotificationSource::Message {
        let Some(message) = &record.message else {
            return false;
        };
        let addressed_to_the_owner = message.to_session_id == watch.owner.session_id;
        if addressed_to_the_owner && record.seq <= owner_inbox_position {
            return false;
        }
        if watch.filter.is_inbox_of(&watch.owner.session_id) {
            return addressed_to_the_owner;
        }
        if !watch.filter.kinds.contains(&WatchKind::Message) {
            return false;
        }
        return watch
            .filter
            .session_ids
            .iter()
            .any(|id| id == &record.session_id)
            || (watch.filter.started_by_me
                && record.orchestrator_session_id.as_deref()
                    == Some(watch.owner.session_id.as_str()));
    }
    let selected = watch
        .filter
        .session_ids
        .iter()
        .any(|id| id == &record.session_id)
        || (watch.filter.started_by_me
            && record.orchestrator_session_id.as_deref() == Some(watch.owner.session_id.as_str()));
    selected && WatchKind::of(record).is_some_and(|kind| watch.filter.kinds.contains(&kind))
}

#[cfg(test)]
mod trunk_commit_tests {
    use super::*;
    use crate::session_watches::WatchOwner;
    use tddy_daemon_kernel::session_notification::SessionNotificationKind;

    const ORCHESTRATOR: &str = "01a1fdd8-0000-7000-8000-00000000000a";

    fn an_orchestrators_watch(kinds: Vec<WatchKind>) -> SessionWatch {
        SessionWatch {
            watch_id: "w-1".to_string(),
            owner: WatchOwner {
                project_id: "p".to_string(),
                session_id: ORCHESTRATOR.to_string(),
                os_user: "dev".to_string(),
            },
            filter: WatchFilter {
                session_ids: vec![ORCHESTRATOR.to_string()],
                started_by_me: false,
                kinds,
            },
            created_at_unix_ms: 0,
            expires_at_unix_ms: u64::MAX,
            position: 0,
            created_seq: 0,
        }
    }

    fn a_trunk_record_of(session_id: &str) -> ProjectNotificationRecord {
        ProjectNotificationRecord {
            seq: 1,
            session_id: session_id.to_string(),
            label: "orchestrator".to_string(),
            kind: SessionNotificationKind::Activity,
            source: SessionNotificationSource::TrunkCommit,
            text: "origin/main: b Add the outbox (dev)".to_string(),
            agent: None,
            question: None,
            ended: None,
            activity_status: None,
            message: None,
            at_unix_ms: 0,
            orchestrator_session_id: None,
        }
    }

    #[test]
    fn a_trunk_record_is_a_trunk_commit_event() {
        assert_eq!(
            WatchKind::of(&a_trunk_record_of(ORCHESTRATOR)),
            Some(WatchKind::TrunkCommit)
        );
    }

    #[test]
    fn an_orchestrators_self_watch_matches_its_own_trunk_activity() {
        let watch = an_orchestrators_watch(vec![WatchKind::TrunkCommit]);

        assert!(watch_matches(&watch, &a_trunk_record_of(ORCHESTRATOR), 0));
    }

    #[test]
    fn a_self_watch_still_matches_no_other_kind_of_its_own_session() {
        let watch = an_orchestrators_watch(vec![WatchKind::TrunkCommit, WatchKind::Ended]);
        let own_end = ProjectNotificationRecord {
            source: SessionNotificationSource::SessionEnded,
            ..a_trunk_record_of(ORCHESTRATOR)
        };

        assert!(!watch_matches(&watch, &own_end, 0));
    }
}
