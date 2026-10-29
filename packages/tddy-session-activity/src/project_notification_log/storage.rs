//! The project notification log's JSON form (`#home-inbox` 7/8): the line a record is persisted as,
//! and the object a record is answered as. They are one shape, so a record a wait returns is the
//! record the file holds.
//!
//! A project's file is `<data dir>/projects/<project_id>/notifications.jsonl`, one JSON object per
//! line. Lines are appended as records arrive, and the file is rewritten to the log's window when
//! it has grown well past it, so it stays bounded. Sequence numbers live in the lines, so a replay
//! continues them and a number is never reused.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tddy_daemon_kernel::session_notification::{
    AgentNotification, NotificationLevel, QuestionNotice, QuestionNoticeItem, QuestionNoticeOption,
    SessionNotificationKind, SessionNotificationSource,
};

use super::{Held, ProjectNotificationRecord};

const NOTIFICATIONS_FILE: &str = "notifications.jsonl";
const LOG_TARGET: &str = "tddy_session_activity::project_notification_log";

/// A record as the answers of the two tools spell it.
pub fn record_json(
    record: &ProjectNotificationRecord,
) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(RecordDto::from(record))
}

/// Where `project_id`'s records are kept under `data_dir`. An id that cannot name a directory is
/// refused rather than joined onto the path.
pub(super) fn project_file(data_dir: &Path, project_id: &str) -> anyhow::Result<PathBuf> {
    let names_a_directory = !project_id.is_empty()
        && project_id != "."
        && project_id != ".."
        && !project_id.contains(['/', '\\', '\0']);
    anyhow::ensure!(
        names_a_directory,
        "project id {project_id:?} cannot name a directory"
    );
    Ok(projects_dir(data_dir)
        .join(project_id)
        .join(NOTIFICATIONS_FILE))
}

fn projects_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("projects")
}

/// One project's file as it was read back at start.
pub(super) struct Replayed {
    pub(super) project_id: String,
    /// Its records, in the order the file holds them.
    pub(super) records: Vec<Held>,
    /// The lines the file holds, the malformed ones included.
    pub(super) lines: usize,
}

/// Every project with a notification file under `data_dir`. A daemon that has never recorded one
/// has none, and an unreadable file is an error, so a log is never started over records it could
/// not read.
pub(super) fn replay_all(data_dir: &Path) -> anyhow::Result<Vec<Replayed>> {
    let dir = projects_dir(data_dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(anyhow::anyhow!("read {}: {e}", dir.display())),
    };
    let mut replayed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| anyhow::anyhow!("read {}: {e}", dir.display()))?;
        let Ok(project_id) = entry.file_name().into_string() else {
            continue;
        };
        let path = entry.path().join(NOTIFICATIONS_FILE);
        if !path.is_file() {
            continue;
        }
        let (records, lines) = replay_file(&path)?;
        replayed.push(Replayed {
            project_id,
            records,
            lines,
        });
    }
    Ok(replayed)
}

fn replay_file(path: &Path) -> anyhow::Result<(Vec<Held>, usize)> {
    end_on_a_line_break(path)?;
    let file =
        std::fs::File::open(path).map_err(|e| anyhow::anyhow!("open {}: {e}", path.display()))?;
    let mut records = Vec::new();
    let mut lines = 0;
    for (number, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        lines += 1;
        match serde_json::from_str::<Line>(&line) {
            Ok(stored) => records.push(stored.into()),
            Err(e) => log::warn!(
                target: LOG_TARGET,
                "skipping line {} of {}: {e}",
                number + 1,
                path.display()
            ),
        }
    }
    Ok((records, lines))
}

/// A crash part-way through an append leaves a line with no line break; the next append would
/// then run on into it and be lost with it. Ending the file on a line break first keeps the torn
/// line alone, where the replay skips it.
fn end_on_a_line_break(path: &Path) -> anyhow::Result<()> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .open(path)
        .map_err(|e| anyhow::anyhow!("open {}: {e}", path.display()))?;
    let length = file
        .metadata()
        .map_err(|e| anyhow::anyhow!("stat {}: {e}", path.display()))?
        .len();
    if length == 0 {
        return Ok(());
    }
    let mut last = [0u8; 1];
    file.seek(SeekFrom::Start(length - 1))
        .and_then(|_| file.read_exact(&mut last))
        .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
    if last[0] != b'\n' {
        log::warn!(target: LOG_TARGET, "{} ends in a torn line; ending it", path.display());
        file.write_all(b"\n")
            .map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))?;
    }
    Ok(())
}

/// Append `held` to the file at `path`, creating its directory and file as needed.
pub(super) fn append(path: &Path, held: &Held) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create {}: {e}", parent.display()))?;
    }
    let mut line = serde_json::to_string(&Line::from(held))
        .map_err(|e| anyhow::anyhow!("serialize a notification record: {e}"))?;
    line.push('\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| anyhow::anyhow!("open {}: {e}", path.display()))?;
    file.write_all(line.as_bytes())
        .map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))
}

/// Replace the file at `path` with `window`'s records, written to a sibling and renamed over it so
/// a failure part-way leaves the previous file whole.
pub(super) fn rewrite<'a>(
    path: &Path,
    window: impl Iterator<Item = &'a Held>,
) -> anyhow::Result<()> {
    let mut body = String::new();
    for held in window {
        let line = serde_json::to_string(&Line::from(held))
            .map_err(|e| anyhow::anyhow!("serialize a notification record: {e}"))?;
        body.push_str(&line);
        body.push('\n');
    }
    let temporary = path.with_extension("jsonl.tmp");
    let mut file = std::fs::File::create(&temporary)
        .map_err(|e| anyhow::anyhow!("create {}: {e}", temporary.display()))?;
    // Flushed to disk before the rename, so a crash after it never leaves an empty or short file
    // where the previous one stood.
    file.write_all(body.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| anyhow::anyhow!("write {}: {e}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .map_err(|e| anyhow::anyhow!("replace {}: {e}", path.display()))?;
    // The rename itself is durable once the directory holding it is.
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| anyhow::anyhow!("sync {}: {e}", parent.display()))?;
    }
    Ok(())
}

// ── the JSON form ────────────────────────────────────────────────────────────────────────────────

/// One line of the file: the record, and the OS user whose session made it.
#[derive(Serialize, Deserialize)]
struct Line {
    os_user: String,
    #[serde(flatten)]
    record: RecordDto,
}

impl From<&Held> for Line {
    fn from(held: &Held) -> Self {
        Self {
            os_user: held.os_user.clone(),
            record: RecordDto::from(&held.record),
        }
    }
}

impl From<Line> for Held {
    fn from(line: Line) -> Self {
        Self {
            os_user: line.os_user,
            record: line.record.into(),
        }
    }
}

/// A [`ProjectNotificationRecord`] as JSON.
#[derive(Serialize, Deserialize)]
struct RecordDto {
    seq: u64,
    session_id: String,
    label: String,
    kind: KindDto,
    source: SourceDto,
    text: String,
    agent: Option<AgentDto>,
    question: Option<QuestionDto>,
    at_unix_ms: u64,
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
            at_unix_ms: record.at_unix_ms,
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
            at_unix_ms: dto.at_unix_ms,
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
