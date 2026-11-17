use crate::project_notification_log::ProjectNotificationRecord;
use serde::{Deserialize, Serialize};
use tddy_daemon_kernel::session_notification::{
    AgentNotification, NotificationLevel, QuestionNotice, QuestionNoticeItem, QuestionNoticeOption,
    SessionEndNotice, SessionMessage, SessionNotificationKind, SessionNotificationSource,
};

/// A [`ProjectNotificationRecord`] as JSON.
#[derive(Serialize, Deserialize)]
pub(super) struct RecordDto {
    seq: u64,
    session_id: String,
    label: String,
    kind: KindDto,
    source: SourceDto,
    text: String,
    agent: Option<AgentDto>,
    question: Option<QuestionDto>,
    /// Absent from every record but a session-ended one, and from a line written before records
    /// carried how a session ended (`#activity-wake` 1/3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ended: Option<EndedDto>,
    /// Absent from every record but an activity-status one, and from a line written before
    /// records carried the status they were raised for (`#activity-wake` 2/3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    activity_status: Option<String>,
    at_unix_ms: u64,
    /// Absent from a line written before records named their orchestrator (`#orchestrators`
    /// 4/11), and from a record of a session no orchestrator started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    orchestrator_session_id: Option<String>,
    /// Absent from every record but a `message` one, and from a line written before records
    /// carried what one session sent another (`#activity-wake` 5/5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<MessageDto>,
}

/// A [`SessionMessage`] as JSON.
#[derive(Serialize, Deserialize)]
struct MessageDto {
    to_session_id: String,
    message_id: String,
    /// Absent from a message that replies to nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_to: Option<String>,
    text: String,
}

impl From<&SessionMessage> for MessageDto {
    fn from(message: &SessionMessage) -> Self {
        Self {
            to_session_id: message.to_session_id.clone(),
            message_id: message.message_id.clone(),
            reply_to: message.reply_to.clone(),
            text: message.text.clone(),
        }
    }
}

impl From<MessageDto> for SessionMessage {
    fn from(dto: MessageDto) -> Self {
        Self {
            to_session_id: dto.to_session_id,
            message_id: dto.message_id,
            reply_to: dto.reply_to,
            text: dto.text,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum KindDto {
    Activity,
    AttentionRequired,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SourceDto {
    ActivityStatus,
    AgentToolCall,
    Presenter,
    Agent,
    PendingQuestion,
    SessionEnded,
    Message,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LevelDto {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Serialize, Deserialize)]
struct AgentDto {
    title: String,
    body_markdown: String,
    level: LevelDto,
    sound: bool,
    link: String,
}

/// How a session's agent ended. `exit_code` is absent when no code was seen.
#[derive(Serialize, Deserialize)]
struct EndedDto {
    outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
}

#[derive(Serialize, Deserialize)]
struct QuestionDto {
    request_id: String,
    kind: String,
    tool_name: String,
    summary: String,
    questions: Vec<QuestionItemDto>,
}

#[derive(Serialize, Deserialize)]
struct QuestionItemDto {
    header: String,
    question: String,
    options: Vec<OptionDto>,
    multi_select: bool,
}

#[derive(Serialize, Deserialize)]
struct OptionDto {
    label: String,
    description: String,
}

impl From<&ProjectNotificationRecord> for RecordDto {
    fn from(record: &ProjectNotificationRecord) -> Self {
        Self {
            seq: record.seq,
            session_id: record.session_id.clone(),
            label: record.label.clone(),
            kind: record.kind.into(),
            source: record.source.into(),
            text: record.text.clone(),
            agent: record.agent.as_ref().map(AgentDto::from),
            question: record.question.as_ref().map(QuestionDto::from),
            ended: record.ended.as_ref().map(EndedDto::from),
            activity_status: record.activity_status.clone(),
            message: record.message.as_ref().map(MessageDto::from),
            at_unix_ms: record.at_unix_ms,
            orchestrator_session_id: record.orchestrator_session_id.clone(),
        }
    }
}

impl From<RecordDto> for ProjectNotificationRecord {
    fn from(dto: RecordDto) -> Self {
        Self {
            seq: dto.seq,
            session_id: dto.session_id,
            label: dto.label,
            kind: dto.kind.into(),
            source: dto.source.into(),
            text: dto.text,
            agent: dto.agent.map(AgentNotification::from),
            question: dto.question.map(QuestionNotice::from),
            ended: dto.ended.map(SessionEndNotice::from),
            activity_status: dto.activity_status,
            message: dto.message.map(SessionMessage::from),
            at_unix_ms: dto.at_unix_ms,
            orchestrator_session_id: dto.orchestrator_session_id,
        }
    }
}

impl From<SessionNotificationKind> for KindDto {
    fn from(kind: SessionNotificationKind) -> Self {
        match kind {
            SessionNotificationKind::Activity => Self::Activity,
            SessionNotificationKind::AttentionRequired => Self::AttentionRequired,
        }
    }
}

impl From<KindDto> for SessionNotificationKind {
    fn from(kind: KindDto) -> Self {
        match kind {
            KindDto::Activity => Self::Activity,
            KindDto::AttentionRequired => Self::AttentionRequired,
        }
    }
}

impl From<SessionNotificationSource> for SourceDto {
    fn from(source: SessionNotificationSource) -> Self {
        match source {
            SessionNotificationSource::ActivityStatus => Self::ActivityStatus,
            SessionNotificationSource::AgentToolCall => Self::AgentToolCall,
            SessionNotificationSource::Presenter => Self::Presenter,
            SessionNotificationSource::Agent => Self::Agent,
            SessionNotificationSource::PendingQuestion => Self::PendingQuestion,
            SessionNotificationSource::SessionEnded => Self::SessionEnded,
            SessionNotificationSource::Message => Self::Message,
        }
    }
}

impl From<SourceDto> for SessionNotificationSource {
    fn from(source: SourceDto) -> Self {
        match source {
            SourceDto::ActivityStatus => Self::ActivityStatus,
            SourceDto::AgentToolCall => Self::AgentToolCall,
            SourceDto::Presenter => Self::Presenter,
            SourceDto::Agent => Self::Agent,
            SourceDto::PendingQuestion => Self::PendingQuestion,
            SourceDto::SessionEnded => Self::SessionEnded,
            SourceDto::Message => Self::Message,
        }
    }
}

impl From<NotificationLevel> for LevelDto {
    fn from(level: NotificationLevel) -> Self {
        match level {
            NotificationLevel::Info => Self::Info,
            NotificationLevel::Success => Self::Success,
            NotificationLevel::Warning => Self::Warning,
            NotificationLevel::Error => Self::Error,
        }
    }
}

impl From<LevelDto> for NotificationLevel {
    fn from(level: LevelDto) -> Self {
        match level {
            LevelDto::Info => Self::Info,
            LevelDto::Success => Self::Success,
            LevelDto::Warning => Self::Warning,
            LevelDto::Error => Self::Error,
        }
    }
}

impl From<&SessionEndNotice> for EndedDto {
    fn from(ended: &SessionEndNotice) -> Self {
        Self {
            outcome: ended.outcome.clone(),
            exit_code: ended.exit_code,
        }
    }
}

impl From<EndedDto> for SessionEndNotice {
    fn from(ended: EndedDto) -> Self {
        Self {
            outcome: ended.outcome,
            exit_code: ended.exit_code,
        }
    }
}

impl From<&AgentNotification> for AgentDto {
    fn from(agent: &AgentNotification) -> Self {
        Self {
            title: agent.title.clone(),
            body_markdown: agent.body_markdown.clone(),
            level: agent.level.into(),
            sound: agent.sound,
            link: agent.link.clone(),
        }
    }
}

impl From<AgentDto> for AgentNotification {
    fn from(agent: AgentDto) -> Self {
        Self {
            title: agent.title,
            body_markdown: agent.body_markdown,
            level: agent.level.into(),
            sound: agent.sound,
            link: agent.link,
        }
    }
}

impl From<&QuestionNotice> for QuestionDto {
    fn from(question: &QuestionNotice) -> Self {
        Self {
            request_id: question.request_id.clone(),
            kind: question.kind.clone(),
            tool_name: question.tool_name.clone(),
            summary: question.summary.clone(),
            questions: question
                .questions
                .iter()
                .map(QuestionItemDto::from)
                .collect(),
        }
    }
}

impl From<QuestionDto> for QuestionNotice {
    fn from(question: QuestionDto) -> Self {
        Self {
            request_id: question.request_id,
            kind: question.kind,
            tool_name: question.tool_name,
            summary: question.summary,
            questions: question
                .questions
                .into_iter()
                .map(QuestionNoticeItem::from)
                .collect(),
        }
    }
}

impl From<&QuestionNoticeItem> for QuestionItemDto {
    fn from(item: &QuestionNoticeItem) -> Self {
        Self {
            header: item.header.clone(),
            question: item.question.clone(),
            options: item.options.iter().map(OptionDto::from).collect(),
            multi_select: item.multi_select,
        }
    }
}

impl From<QuestionItemDto> for QuestionNoticeItem {
    fn from(item: QuestionItemDto) -> Self {
        Self {
            header: item.header,
            question: item.question,
            options: item
                .options
                .into_iter()
                .map(QuestionNoticeOption::from)
                .collect(),
            multi_select: item.multi_select,
        }
    }
}

impl From<&QuestionNoticeOption> for OptionDto {
    fn from(option: &QuestionNoticeOption) -> Self {
        Self {
            label: option.label.clone(),
            description: option.description.clone(),
        }
    }
}

impl From<OptionDto> for QuestionNoticeOption {
    fn from(option: OptionDto) -> Self {
        Self {
            label: option.label,
            description: option.description,
        }
    }
}
