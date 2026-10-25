//! Recording a daemon-run conversation as its turns run.
//!
//! Every daemon-run conversation has one transcript, written through [`ConversationTranscript`]: each frame
//! appended to disk takes the next position in it and is published on the conversation's live channel, so a
//! replay that tails the channel sees what the transcript holds and nothing twice.
//!
//! A conversation whose session streams its updates ([`SubagentSession::updates`]) is recorded as they
//! arrive: [`ConversationRecorder::begin_turn`] writes the prompt, a recorder task writes the text and tool
//! calls the agent streams, and [`ConversationRecorder::end_turn`] writes only what a turn's end owns, the
//! pending permission request and the stop reason. Any other conversation is recorded whole when its turn
//! ends.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tddy_agent_backend::{AcpToolStatus, AcpUpdate};
use tddy_daemon_kernel::conversation_frames::{ConversationFrame, ConversationFrames};
use tddy_discovery::subagent::{PromptOutcome, SubagentError, SubagentSession, TurnRequest};
use tddy_rpc::Status;
use tddy_service::acp_replay::{agent_text_frame, read_acp_transcript, session_update_frame};
use tddy_service::proto::acp::{
    session_update, AcpAgentMessage, SessionUpdate, ToolCall, ToolCallId, ToolCallStatus,
    ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use tddy_service::subagent_conversation_replay::{
    append_conversation_frame, conversation_transcript_dir,
};
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::{broadcast, mpsc, oneshot, Notify};

use crate::conversation_transcript::{
    now_unix_ms, turn_end_frames, user_prompt_frame, whole_turn_frames, TurnEnd,
};

/// One conversation's transcript, written one frame at a time.
///
/// The position a frame takes and the append that writes it happen under one lock, so positions follow the
/// file's order. A frame is published once it is on disk.
pub struct ConversationTranscript {
    session_id: String,
    conversation_id: String,
    session_dir: PathBuf,
    frames: Arc<ConversationFrames>,
    next_seq: Mutex<u64>,
}

impl ConversationTranscript {
    /// The transcript of `conversation_id`, positioned after the frames it already holds.
    pub fn open(
        session_dir: &Path,
        session_id: &str,
        conversation_id: &str,
        frames: Arc<ConversationFrames>,
    ) -> io::Result<Self> {
        let existing =
            read_acp_transcript(&conversation_transcript_dir(session_dir, conversation_id))?;
        Ok(Self {
            session_id: session_id.to_string(),
            conversation_id: conversation_id.to_string(),
            session_dir: session_dir.to_path_buf(),
            frames,
            next_seq: Mutex::new(existing.len() as u64),
        })
    }

    /// Append `frame` to the transcript, then publish it to the conversation's live channel.
    pub fn append(&self, frame: AcpAgentMessage) -> io::Result<()> {
        let mut next_seq = self.next_seq.lock().unwrap_or_else(|e| e.into_inner());
        append_conversation_frame(&self.session_dir, &self.conversation_id, &frame)?;
        let seq = *next_seq;
        *next_seq += 1;
        self.frames.publish(
            &self.session_id,
            &self.conversation_id,
            ConversationFrame { seq, frame },
        );
        Ok(())
    }
}

/// The writer of one open conversation: its transcript, and how its turns reach it.
pub struct ConversationRecorder {
    transcript: Arc<ConversationTranscript>,
    /// Set when the session streams its updates; `None` for a conversation recorded whole at each turn's end.
    stream: Option<UpdateStream>,
}

/// A streamed conversation's recorder task, as its turns see it.
struct UpdateStream {
    /// Asks the recorder task to write every update already streamed, then answers.
    flush: mpsc::UnboundedSender<oneshot::Sender<()>>,
    /// The failure that stopped the recorder task, once it has stopped. A conversation whose stream failed
    /// refuses its next turn, naming the failure.
    failure: Arc<Mutex<Option<String>>>,
}

impl ConversationRecorder {
    /// Open the recorder for `conversation_id` on `session`, starting the recorder task when the session
    /// streams. Called once, when the conversation opens, before any turn runs: a subscriber taken later would
    /// miss the updates of the turns before it.
    pub fn open(
        session_dir: &Path,
        session_id: &str,
        conversation_id: &str,
        frames: Arc<ConversationFrames>,
        session: &dyn SubagentSession,
    ) -> io::Result<Self> {
        let transcript = Arc::new(ConversationTranscript::open(
            session_dir,
            session_id,
            conversation_id,
            frames,
        )?);
        let stream = session.updates().map(|updates| {
            let (flush, flush_requests) = mpsc::unbounded_channel();
            let failure = Arc::new(Mutex::new(None));
            tokio::spawn(record_updates(
                updates,
                flush_requests,
                Arc::clone(&transcript),
                Arc::clone(&failure),
            ));
            UpdateStream { flush, failure }
        });
        Ok(Self { transcript, stream })
    }

    /// Before a turn runs. A streamed conversation writes the turn's prompt now, so the transcript holds the
    /// question before any answer streams in. A conversation whose stream has failed refuses the turn, naming
    /// the failure.
    pub fn begin_turn(&self, prompt: Option<&str>) -> Result<(), String> {
        let Some(stream) = &self.stream else {
            return Ok(());
        };
        if let Some(failure) = stream_failure(stream) {
            return Err(failure);
        }
        match prompt {
            Some(prompt) => self
                .transcript
                .append(user_prompt_frame(prompt, now_unix_ms()))
                .map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    /// When a turn ends, whatever its outcome. A streamed conversation waits for the recorder task to write
    /// every update the turn produced, then writes the turn's end. A conversation recorded whole writes the
    /// whole turn once it answered, and nothing when it did not.
    ///
    /// The error names the first recording failure. The caller must then deliver no answer for the turn.
    pub async fn end_turn(&self, prompt: Option<&str>, end: TurnEnd<'_>) -> Result<(), String> {
        let Some(stream) = &self.stream else {
            return match end {
                TurnEnd::Answered(outcome) => {
                    for frame in whole_turn_frames(prompt, outcome, now_unix_ms()) {
                        self.transcript.append(frame).map_err(|e| e.to_string())?;
                    }
                    Ok(())
                }
                TurnEnd::Failed | TurnEnd::Closed => Ok(()),
            };
        };
        flush(stream).await;
        // The turn's end is written even when the stream failed, so the transcript still says the turn stopped.
        // The first failure is the one reported.
        let mut written = Ok(());
        for frame in turn_end_frames(end, now_unix_ms()) {
            if let Err(e) = self.transcript.append(frame) {
                written = Err(e.to_string());
                break;
            }
        }
        match stream_failure(stream) {
            Some(failure) => Err(failure),
            None => written,
        }
    }
}

/// Why a local turn did not answer.
pub(crate) enum TurnError {
    /// The conversation was closed while the turn was in flight.
    Closed { recording: Option<String> },
    /// The agent failed the turn.
    Agent {
        error: SubagentError,
        recording: Option<String>,
    },
    /// The turn's transcript could not be written, so the turn delivers no answer.
    Recording(String),
}

impl TurnError {
    /// What a failed turn reports: the roster badge's summary, when it reports one, and the caller's error.
    ///
    /// A closed conversation reports no badge, since its caller has been told. An agent failure leaves the
    /// agent idle, not ERROR: it is still attached and still promptable, and it is the *clone* that ERROR is
    /// reserved for. The summary is what says what happened.
    pub(crate) fn into_reply(self, conversation_id: &str) -> (Option<String>, Status) {
        match self {
            TurnError::Closed { recording } => (
                None,
                Status::failed_precondition(with_recording_failure(
                    format!(
                        "conversation '{conversation_id}' was closed while its turn was in flight"
                    ),
                    recording,
                )),
            ),
            TurnError::Recording(e) => (
                Some(format!("turn failed: {e}")),
                Status::internal(format!(
                    "conversation '{conversation_id}' turn could not be recorded: {e}"
                )),
            ),
            TurnError::Agent { error, recording } => (
                Some(format!("turn failed: {error}")),
                Status::internal(with_recording_failure(
                    format!("agent conversation '{conversation_id}' failed: {error}"),
                    recording,
                )),
            ),
        }
    }
}

/// Take one turn of a local conversation and record it as it runs.
///
/// The session is held from the turn's start to the end of its recording, so two turns on one conversation
/// are recorded one after the other: a queued turn's prompt is not written into the middle of the turn before
/// it. A conversation closed before or during the turn reports it closed, and the turn's end records the stop.
pub(crate) async fn take_recorded_turn(
    session: &tokio::sync::Mutex<Box<dyn SubagentSession>>,
    recorder: &ConversationRecorder,
    closed: &Notify,
    requested: TurnRequest,
    prompt: Option<&str>,
) -> Result<PromptOutcome, TurnError> {
    let mut session = tokio::select! {
        // Biased so a conversation already closed is reported as closed rather than racing one more turn out
        // of a model.
        biased;
        _ = closed.notified() => return Err(TurnError::Closed { recording: None }),
        guard = session.lock() => guard,
    };
    recorder.begin_turn(prompt).map_err(TurnError::Recording)?;
    let taken = tokio::select! {
        biased;
        _ = closed.notified() => None,
        outcome = session.take_turn(requested) => Some(outcome),
    };
    let end = match &taken {
        None => TurnEnd::Closed,
        Some(Ok(outcome)) => TurnEnd::Answered(outcome),
        Some(Err(_)) => TurnEnd::Failed,
    };
    let recorded = recorder.end_turn(prompt, end).await;
    drop(session);
    match taken {
        None => Err(TurnError::Closed {
            recording: recorded.err(),
        }),
        Some(Err(error)) => Err(TurnError::Agent {
            error,
            recording: recorded.err(),
        }),
        Some(Ok(outcome)) => recorded.map(|()| outcome).map_err(TurnError::Recording),
    }
}

fn stream_failure(stream: &UpdateStream) -> Option<String> {
    stream
        .failure
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Wait until the recorder task has written every update streamed so far. A task that has already stopped has
/// written what it could, and its failure is read from the slot.
async fn flush(stream: &UpdateStream) {
    let (answer, answered) = oneshot::channel();
    if stream.flush.send(answer).is_ok() {
        let _ = answered.await;
    }
}

/// Write each update a streamed conversation sends, as it arrives, until the session is gone.
///
/// Stops at the first failure, which is kept in the slot: a transcript is not written on past a gap. A flush
/// takes every update already queued before it answers, since a turn's updates are queued by the time the turn
/// ends.
async fn record_updates(
    mut updates: broadcast::Receiver<AcpUpdate>,
    mut flush_requests: mpsc::UnboundedReceiver<oneshot::Sender<()>>,
    transcript: Arc<ConversationTranscript>,
    failure: Arc<Mutex<Option<String>>>,
) {
    loop {
        tokio::select! {
            biased;
            Some(answer) = flush_requests.recv() => {
                loop {
                    match updates.try_recv() {
                        Ok(update) => {
                            if !record_update(&transcript, &failure, update) {
                                return;
                            }
                        }
                        Err(TryRecvError::Empty | TryRecvError::Closed) => break,
                        Err(TryRecvError::Lagged(missed)) => {
                            keep_failure(&failure, lagged(missed));
                            return;
                        }
                    }
                }
                let _ = answer.send(());
            }
            received = updates.recv() => match received {
                Ok(update) => {
                    if !record_update(&transcript, &failure, update) {
                        return;
                    }
                }
                Err(RecvError::Lagged(missed)) => {
                    keep_failure(&failure, lagged(missed));
                    return;
                }
                Err(RecvError::Closed) => return,
            }
        }
    }
}

/// Append one streamed update. `false` when it could not be written, after its failure is kept.
fn record_update(
    transcript: &ConversationTranscript,
    failure: &Mutex<Option<String>>,
    update: AcpUpdate,
) -> bool {
    let Some(frame) = update_frame(update, now_unix_ms()) else {
        return true;
    };
    match transcript.append(frame) {
        Ok(()) => true,
        Err(e) => {
            keep_failure(failure, format!("the transcript could not be written: {e}"));
            false
        }
    }
}

fn keep_failure(failure: &Mutex<Option<String>>, message: String) {
    failure
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert(message);
}

fn lagged(missed: u64) -> String {
    format!("the recorder fell {missed} updates behind the coding agent, so the transcript would have a gap")
}

/// The ACP frame one streamed update is replayed as. Reasoning and plans are not recorded: a transcript holds
/// what a turn said and did, as a turn recorded whole does.
fn update_frame(update: AcpUpdate, at: i64) -> Option<AcpAgentMessage> {
    let update = match update {
        AcpUpdate::Text(text) => return Some(agent_text_frame(&text, at)),
        AcpUpdate::ToolCall {
            id,
            title,
            kind,
            raw_input,
        } => session_update::Update::ToolCall(ToolCall {
            tool_call_id: Some(ToolCallId { value: id }),
            title,
            kind: tool_kind_of(&kind) as i32,
            status: ToolCallStatus::InProgress as i32,
            raw_input: raw_input.map(|input| input.to_string()),
            ..Default::default()
        }),
        AcpUpdate::ToolCallUpdate {
            id,
            title,
            status,
            raw_output,
        } => session_update::Update::ToolCallUpdate(ToolCallUpdate {
            tool_call_id: Some(ToolCallId { value: id }),
            fields: Some(ToolCallUpdateFields {
                title,
                status: status.map(|status| tool_status_of(status) as i32),
                raw_output: raw_output.map(|output| output.to_string()),
                ..Default::default()
            }),
        }),
        AcpUpdate::Thought(_) | AcpUpdate::Plan(_) => return None,
    };
    Some(session_update_frame(
        SessionUpdate {
            update: Some(update),
        },
        at,
    ))
}

/// The ACP tool kind a driver's wire spelling names. A kind this crate does not know reads as `other`, as the
/// protocol reads an unknown kind.
fn tool_kind_of(kind: &str) -> ToolKind {
    match kind {
        "read" => ToolKind::Read,
        "edit" => ToolKind::Edit,
        "delete" => ToolKind::Delete,
        "move" => ToolKind::Move,
        "search" => ToolKind::Search,
        "execute" => ToolKind::Execute,
        "think" => ToolKind::Think,
        "fetch" => ToolKind::Fetch,
        _ => ToolKind::Other,
    }
}

fn tool_status_of(status: AcpToolStatus) -> ToolCallStatus {
    match status {
        AcpToolStatus::Pending => ToolCallStatus::Pending,
        AcpToolStatus::InProgress => ToolCallStatus::InProgress,
        AcpToolStatus::Completed => ToolCallStatus::Completed,
        AcpToolStatus::Failed => ToolCallStatus::Failed,
    }
}

/// `message`, with the recording failure of a turn that also failed named after it.
fn with_recording_failure(message: String, recording: Option<String>) -> String {
    match recording {
        Some(failure) => format!("{message}; its transcript could not be written: {failure}"),
        None => message,
    }
}
