//! `CodingAgentSession` — a `SubagentSession` whose turns a coding agent takes, over ACP or headless.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use async_trait::async_trait;
use tddy_agent_backend::{
    AcpAgentCommand, AcpConversation, AcpDriver, AcpStopReason, AcpToolStatus, AcpTurnEnd,
    AcpUpdate, AcpUpdateSink, ConversationDriver, HeadlessCommand, HeadlessDriver, HeadlessHarness,
    NoHooks, OpenSession,
};
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    preview_of, AnsweredBy, ContentBlock, MessageDescriptor, MessageId, MessageRole,
    PermissionAnswering, PermissionRequestView, PromptOutcome, StopReason, SubagentError,
    SubagentSession, ToolCallDescriptor, TurnRequest,
};
use tokio::sync::broadcast;
use tokio::sync::Mutex as AsyncMutex;

use crate::parked_permissions::{nothing_pending, nothing_pending_to_defer, ParkedPermissions};

/// The prompt a conversation is answering, shared with its cancel handles. A turn holds the slot
/// for as long as it runs, so a cancel waits for the turn to end before it returns.
type InFlightSlot = AsyncMutex<Option<InFlightPrompt>>;

/// How far a subscriber to a coding agent's updates may fall behind before it misses some.
const UPDATE_STREAM_CAPACITY: usize = 1024;

/// One conversation with a coding agent: an ACP session over one agent process, or a headless CLI
/// conversation that resumes its CLI's own session turn by turn.
///
/// A turn is one `session/prompt` (ACP) or one CLI invocation (headless); its outcome carries the
/// agent's text, `end_turn` / `cancelled` / …, and one message descriptor per tool call the turn
/// made. A permission request the agent raises pauses the turn: it ends with
/// [`StopReason::PermissionRequested`] and the request, and the prompt stays in flight until a later
/// turn answers the request and the agent finishes. A request the caller hands to the user holds the
/// turn instead, until either side answers it. Options that act on an OpenAI-loop transcript
/// (`fromMessageId`, `correction`, `replacement`, `yieldConditions`, `maxTurns`) are refused with
/// [`SubagentError::unsupported_by_coding_agent`].
///
/// Dropping the session drops its conversation, which terminates the agent process (or the CLI turn
/// in flight).
pub struct CodingAgentSession {
    conversation: Arc<dyn ConversationDriver>,
    /// The ACP conversation `conversation` is, when this session runs over ACP: its driver reports
    /// the agent process id. `None` for a headless session, whose CLI runs once per turn.
    acp: Option<Arc<AcpConversation>>,
    permissions: Arc<ParkedPermissions>,
    /// The prompt the agent is still answering, held across the turns a permission request pauses
    /// it for. `None` when no prompt is in flight.
    in_flight: Arc<InFlightSlot>,
    model: String,
    /// The ordinal the next message id is minted from.
    next_ordinal: u64,
    /// Each turn's question and answer, oldest first, for [`SubagentSession::tail`].
    history: Vec<String>,
    /// Receives every update of every later turn as it streams; set by [`Self::record_updates_with`].
    recorder: Option<AcpUpdateSink>,
    /// Every update of every turn as it streams, for [`SubagentSession::updates`]. Each update goes
    /// here alongside the recorder and the turn's own list, so all of them see the same stream.
    update_stream: broadcast::Sender<AcpUpdate>,
}

/// One `session/prompt` the session has sent and has not yet seen end.
struct InFlightPrompt {
    /// The text the prompt was sent with.
    prompt: String,
    /// Every update the prompt has produced so far, in order; described once the prompt ends.
    updates: Arc<Mutex<Vec<AcpUpdate>>>,
    /// The prompt's result. It is polled by whoever holds the slot, and is not polled between turns.
    ended: Pin<Box<dyn Future<Output = Result<AcpTurnEnd, SubagentError>> + Send>>,
    /// The result, once a cancel has driven the prompt to its end. No turn was waiting to take it
    /// then, so the next turn takes it instead.
    result: Option<Result<AcpTurnEnd, SubagentError>>,
}

/// Where a prompt stands once the session has waited on it.
enum Settled {
    /// The agent is waiting on the caller's answer to this request.
    Paused(PermissionRequestView),
    /// The prompt ended, or failed to.
    Ended(Result<AcpTurnEnd, SubagentError>),
}

impl CodingAgentSession {
    /// Spawn the agent from `command`, open (or resume) its ACP session, and hold both.
    pub async fn start(
        command: AcpAgentCommand,
        open: OpenSession,
        model: String,
    ) -> Result<Self, SubagentError> {
        let permissions = Arc::new(ParkedPermissions::default());
        let driver = AcpDriver::spawn(command, permissions.clone(), Arc::new(NoHooks))
            .map_err(|e| SubagentError::from(format!("could not start the coding agent: {e}")))?;
        Self::opened(driver, permissions, open, model).await
    }

    /// [`Self::start`] for an agent already running behind `stream` (started by the supervisor as
    /// the session's OS user): no process of this session's own; closing the stream ends the agent.
    pub async fn start_over(
        stream: std::os::unix::net::UnixStream,
        open: OpenSession,
        model: String,
    ) -> Result<Self, SubagentError> {
        let permissions = Arc::new(ParkedPermissions::default());
        let driver = AcpDriver::spawn_over(stream, permissions.clone(), Arc::new(NoHooks))
            .map_err(|e| SubagentError::from(format!("could not reach the coding agent: {e}")))?;
        Self::opened(driver, permissions, open, model).await
    }

    /// A headless conversation: `command` runs the harness's CLI once per turn, and every turn after
    /// the first resumes the session the CLI named on the first. Nothing runs until a turn is taken.
    pub async fn start_headless(
        harness: HeadlessHarness,
        command: HeadlessCommand,
        model: String,
    ) -> Result<Self, SubagentError> {
        let conversation: Arc<dyn ConversationDriver> =
            Arc::new(HeadlessDriver::new(harness, command));
        Ok(Self::with_conversation(
            conversation,
            None,
            Arc::new(ParkedPermissions::default()),
            model,
        ))
    }

    async fn opened(
        driver: AcpDriver,
        permissions: Arc<ParkedPermissions>,
        open: OpenSession,
        model: String,
    ) -> Result<Self, SubagentError> {
        let opened = driver
            .open_session(open)
            .await
            .map_err(|e| SubagentError::from(format!("could not open the coding agent: {e}")))?;
        let acp = Arc::new(AcpConversation {
            driver,
            session_id: opened.session_id,
        });
        Ok(Self::with_conversation(
            acp.clone(),
            Some(acp),
            permissions,
            model,
        ))
    }

    fn with_conversation(
        conversation: Arc<dyn ConversationDriver>,
        acp: Option<Arc<AcpConversation>>,
        permissions: Arc<ParkedPermissions>,
        model: String,
    ) -> Self {
        Self {
            conversation,
            acp,
            permissions,
            in_flight: Arc::new(AsyncMutex::new(None)),
            model,
            next_ordinal: 1,
            history: Vec::new(),
            recorder: None,
            update_stream: broadcast::channel(UPDATE_STREAM_CAPACITY).0,
        }
    }

    /// The handle this session's turns park their permission requests on. A caller that answers
    /// those requests (the daemon) holds this handle rather than the session.
    pub fn permissions(&self) -> Arc<ParkedPermissions> {
        Arc::clone(&self.permissions)
    }

    /// Send every ACP update of every later turn to `sink`, as it streams — how a daemon-run
    /// conversation's transcript records the agent's tool calls while the turn is still running.
    /// A turn already in flight keeps the recorder it began with.
    pub fn record_updates_with(&mut self, sink: AcpUpdateSink) {
        self.recorder = Some(sink);
    }

    /// What the daemon's `AnswerAgentPermission` answers this conversation's parked request through.
    /// It is the session's own parked requests, so a user's answer and the main agent's race for the
    /// same request, and the first one wins.
    pub fn permission_answering(&self) -> Arc<dyn PermissionAnswering> {
        self.permissions.clone()
    }

    /// A handle that cancels this session's running turn without holding the session — what the
    /// conversation table uses when `subagent_cancel` arrives while a prompt holds the session.
    // TODO(acp-subagent n3): the conversation table holds Box<dyn SubagentSession> and cannot reach
    // this; n3 permission-yield wires it.
    pub fn cancel_handle(&self) -> CancelHandle {
        CancelHandle {
            conversation: Arc::downgrade(&self.conversation),
            permissions: Arc::downgrade(&self.permissions),
            in_flight: Arc::downgrade(&self.in_flight),
        }
    }

    /// The agent process's id, while it runs. `None` for a headless session, whose CLI process
    /// exists only for the length of a turn.
    pub fn agent_pid(&self) -> Option<u32> {
        self.acp.as_ref().and_then(|acp| acp.driver.agent_pid())
    }

    /// One message of this turn, under the next id this conversation mints.
    fn message(&mut self, shape: MessageShape) -> MessageDescriptor {
        let id = MessageId::from_ordinal(self.next_ordinal);
        self.next_ordinal += 1;
        MessageDescriptor {
            id,
            role: shape.role,
            tool: shape.tool,
            tool_calls: shape.tool_calls,
            is_error: shape.is_error,
            preview: preview_of(&shape.preview),
            result_summary: None,
            worktree_change: None,
        }
    }

    /// The turn's messages: one per tool call, then the agent's text when it wrote any.
    fn describe_turn(&mut self, updates: Vec<AcpUpdate>, text: &str) -> Vec<MessageDescriptor> {
        let mut shapes: Vec<MessageShape> = collect_tool_calls(updates)
            .into_iter()
            .map(MessageShape::tool_call)
            .collect();
        if !text.is_empty() {
            shapes.push(MessageShape::assistant(text));
        }
        shapes
            .into_iter()
            .map(|shape| self.message(shape))
            .collect()
    }

    /// Send `prompt` as the conversation's next `session/prompt`. The prompt is sent when it is
    /// first polled, which [`settle`] does at once.
    fn begin_prompt(&self, prompt: String) -> InFlightPrompt {
        let updates = Arc::new(Mutex::new(Vec::new()));
        let sink_updates = Arc::clone(&updates);
        let recorder = self.recorder.clone();
        let stream = self.update_stream.clone();
        let sink: AcpUpdateSink = Arc::new(move |update| {
            if let Some(recorder) = &recorder {
                recorder(update.clone());
            }
            // A send with no subscriber is not an error: nothing is streaming this conversation.
            let _ = stream.send(update.clone());
            sink_updates
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(update);
        });
        let conversation = Arc::clone(&self.conversation);
        let text = prompt.clone();
        let ended = Box::pin(async move {
            conversation
                .prompt_turn(text, Some(sink))
                .await
                .map_err(|e| SubagentError::from(format!("the coding agent's turn failed: {e}")))
        });
        InFlightPrompt {
            prompt,
            updates,
            ended,
            result: None,
        }
    }

    /// Deliver the answer `request` carries, as the main agent's, before the in-flight slot is taken:
    /// the answer reaches the request it names whether or not a turn holds the slot, so it is not
    /// queued behind the turn that is waiting for it. Returns the id of the request answered, when
    /// one was. A turn carrying a prompt answers nothing here; [`Self::resume`] refuses it.
    fn deliver_main_agent_answer<'r>(
        &self,
        request: &'r TurnRequest,
    ) -> Result<Option<&'r str>, SubagentError> {
        if request.prompt_text().is_some() {
            return Ok(None);
        }
        match (
            request.permission_answer(),
            request.permission_deferred_to_user(),
        ) {
            (Some(_), Some(_)) => Err(SubagentError::from(
                "a turn cannot both answer a permission request and hand it to the user",
            )),
            (Some(answer), None) => {
                self.permissions
                    .answer_recording(answer, AnsweredBy::MainAgent)
                    .map_err(SubagentError::from)?;
                Ok(Some(answer.request_id()))
            }
            (None, _) => Ok(None),
        }
    }

    /// Check `request` against the prompt already in flight. A new prompt is refused while the turn
    /// is open, and so is a turn that asks nothing while a request is pending; each refusal names
    /// the pending request. A deferral is accepted only for the pending request it names: the turn
    /// then waits for that request's answer ([`settle`]), and the request is recorded as deferred to
    /// the user, so the user may answer it.
    fn resume(&self, request: &TurnRequest) -> Result<(), SubagentError> {
        if request.prompt_text().is_some() {
            return Err(match self.permissions.pending() {
                Some(pending) => refused_while_pending(&pending.request_id),
                None => SubagentError::from(
                    "the coding agent's previous turn is still open: resume it before prompting again",
                ),
            });
        }
        match (
            request.permission_deferred_to_user(),
            self.permissions.pending(),
        ) {
            (Some(deferred), Some(pending)) if pending.request_id == deferred => {
                self.permissions.defer_to_user(deferred);
                Ok(())
            }
            (Some(deferred), Some(pending)) => Err(SubagentError::from(format!(
                "permission request '{}' is the one pending; the deferral names '{deferred}'",
                pending.request_id
            ))),
            (Some(deferred), None) => Err(SubagentError::from(nothing_pending_to_defer(deferred))),
            (None, Some(pending)) => Err(refused_while_pending(&pending.request_id)),
            (None, None) => Ok(()),
        }
    }

    /// The outcome of a prompt that ended: its stop reason, its text, and the messages it appended
    /// across every turn it took.
    fn finish(
        &mut self,
        prompt: &str,
        updates: Vec<AcpUpdate>,
        end: AcpTurnEnd,
    ) -> Result<PromptOutcome, SubagentError> {
        let stop_reason = stop_reason_of(end.stop_reason)?;
        let messages = self.describe_turn(updates, &end.text);
        self.history.push(format!("user: {}", preview_of(prompt)));
        self.history
            .push(format!("assistant: {}", preview_of(&end.text)));
        let mut outcome = PromptOutcome::new(
            stop_reason,
            vec![ContentBlock::text(end.text)],
            TokenUsage::default(),
        );
        outcome.messages = messages;
        Ok(outcome)
    }
}

/// Wait until the prompt ends, or until the agent asks for a permission request that no caller
/// has answered yet — the point at which the turn yields it. The request named `deferred` is not
/// yielded: while it is pending the turn waits for it to be answered, by the user or the caller,
/// and then carries on as usual.
async fn settle(
    flight: &mut InFlightPrompt,
    permissions: &ParkedPermissions,
    deferred: Option<&str>,
) -> Settled {
    if let Some(result) = flight.result.take() {
        return Settled::Ended(result);
    }
    loop {
        if let Some(pending) = permissions.pending() {
            if deferred != Some(pending.request_id.as_str()) {
                return Settled::Paused(pending);
            }
        }
        tokio::select! {
            ended = flight.ended.as_mut() => return Settled::Ended(ended),
            () = permissions.parked() => {}
            () = permissions.resolved() => {}
        }
    }
}

/// The new prompt a turn that starts a prompt carries, or why it cannot start one.
fn new_prompt(request: &TurnRequest) -> Result<String, SubagentError> {
    if let Some(answer) = request.permission_answer() {
        return Err(SubagentError::from(nothing_pending(answer)));
    }
    if let Some(deferred) = request.permission_deferred_to_user() {
        return Err(SubagentError::from(nothing_pending_to_defer(deferred)));
    }
    request.prompt_text().map(str::to_string).ok_or_else(|| {
        SubagentError::from("a coding agent has nothing to resume: it takes a new prompt each turn")
    })
}

fn refused_while_pending(request_id: &str) -> SubagentError {
    SubagentError::from(format!(
        "permission request '{request_id}' is pending: answer it before the coding agent takes another turn"
    ))
}

/// Why an answer that was delivered is not continued by this call: the turn holding the request
/// is still running, and it carries on with the answer and returns its outcome to the call that
/// deferred the request, which collects it with `subagent_await`.
fn answer_delivered_while_held(request_id: &str) -> SubagentError {
    SubagentError::from(format!(
        "permission request '{request_id}' was answered and the answer was delivered to the coding agent; \
         the turn that deferred it carries on and returns its outcome to the call that deferred it, \
         which collects it with subagent_await"
    ))
}

/// One tool call a turn made, folded from its `tool_call` and `tool_call_update`s.
struct ToolCallRecord {
    id: String,
    name: String,
    arguments: String,
    failed: bool,
}

/// The turn's tool calls in the order the agent made them, each marked failed when any of its
/// updates said so.
fn collect_tool_calls(updates: Vec<AcpUpdate>) -> Vec<ToolCallRecord> {
    let mut calls: Vec<ToolCallRecord> = Vec::new();
    for update in updates {
        match update {
            AcpUpdate::ToolCall {
                id,
                title,
                raw_input,
                ..
            } => calls.push(ToolCallRecord {
                id,
                name: title,
                arguments: raw_input.map(|input| input.to_string()).unwrap_or_default(),
                failed: false,
            }),
            AcpUpdate::ToolCallUpdate { id, status, .. } => {
                if let Some(call) = calls.iter_mut().find(|call| call.id == id) {
                    call.failed |= status == Some(AcpToolStatus::Failed);
                }
            }
            AcpUpdate::Text(_) | AcpUpdate::Thought(_) | AcpUpdate::Plan(_) => {}
        }
    }
    calls
}

/// A message descriptor before it has an id; its preview not yet cut.
struct MessageShape {
    role: MessageRole,
    tool: Option<String>,
    tool_calls: Vec<ToolCallDescriptor>,
    is_error: bool,
    preview: String,
}

impl MessageShape {
    fn tool_call(call: ToolCallRecord) -> Self {
        Self {
            role: MessageRole::Tool,
            tool: Some(call.name.clone()),
            tool_calls: vec![ToolCallDescriptor {
                name: call.name,
                arguments: preview_of(&call.arguments),
            }],
            is_error: call.failed,
            preview: call.arguments,
        }
    }

    fn assistant(text: &str) -> Self {
        Self {
            role: MessageRole::Assistant,
            tool: None,
            tool_calls: Vec::new(),
            is_error: false,
            preview: text.to_string(),
        }
    }
}

/// How long [`CancelHandle::cancel`] waits for the cancelled turn to end. Past it the call returns
/// `Ok` and the turn is left to the session's drop, which stops the agent process, so an agent that
/// never ends its turn cannot hang `subagent_cancel`.
pub const CANCEL_SETTLE_DEADLINE: Duration = Duration::from_secs(10);

/// Cancels a [`CodingAgentSession`]'s running turn. Every permission request the turn is parked on
/// is answered as cancelled first, then the turn is cancelled (`session/cancel` over ACP). The call
/// returns once the turn has ended, or after [`CANCEL_SETTLE_DEADLINE`]; the turn then ends as
/// cancelled. Cloneable and `Send`, so it can be held apart from the session. It does not keep the
/// agent process alive: once the session is dropped there is nothing left to cancel.
#[derive(Clone)]
pub struct CancelHandle {
    conversation: Weak<dyn ConversationDriver>,
    permissions: Weak<ParkedPermissions>,
    in_flight: Weak<InFlightSlot>,
}

impl CancelHandle {
    pub async fn cancel(&self) -> Result<(), SubagentError> {
        // The session owns all three; once it is gone there is no turn left to cancel.
        let (Some(conversation), Some(permissions), Some(in_flight)) = (
            self.conversation.upgrade(),
            self.permissions.upgrade(),
            self.in_flight.upgrade(),
        ) else {
            return Ok(());
        };
        permissions.cancel_pending();
        conversation
            .cancel_turn()
            .await
            .map_err(|e| SubagentError::from(format!("could not cancel the coding agent: {e}")))?;
        // A turn that is running holds the slot, so this waits for it to end. A turn paused on a
        // request is driven to its end here, and its result waits for the next turn to take.
        let mut slot = in_flight.lock().await;
        if let Some(flight) = slot.as_mut().filter(|flight| flight.result.is_none()) {
            drive_cancelled_turn_to_its_end(flight, &permissions).await;
        }
        Ok(())
    }
}

/// The turn's stop reason as a subagent outcome spells it; a refusal is an error, not an outcome.
fn stop_reason_of(reason: AcpStopReason) -> Result<StopReason, SubagentError> {
    match reason {
        AcpStopReason::EndTurn => Ok(StopReason::EndTurn),
        AcpStopReason::MaxTokens => Ok(StopReason::MaxTokens),
        AcpStopReason::MaxTurnRequests => Ok(StopReason::MaxTurnRequests),
        AcpStopReason::Cancelled => Ok(StopReason::Cancelled),
        AcpStopReason::Refusal => Err(SubagentError::from("the coding agent refused the turn")),
    }
}

/// Refuse the request options that act on an OpenAI-loop transcript, naming the first one set.
fn refuse_loop_only(request: &TurnRequest) -> Result<(), SubagentError> {
    let refused = [
        (request.requested_max_turns().is_some(), "maxTurns"),
        (!request.yield_conditions().is_empty(), "yieldConditions"),
        (request.rewind_point().is_some(), "fromMessageId"),
        (request.correction().is_some(), "correction"),
        (request.replacement().is_some(), "replacement"),
    ];
    match refused.iter().find(|(set, _)| *set) {
        Some((_, option)) => Err(SubagentError::unsupported_by_coding_agent(option)),
        None => Ok(()),
    }
}

/// Wait, up to [`CANCEL_SETTLE_DEADLINE`], for a cancelled prompt to end, keeping its result for
/// the next turn to take. Past the deadline the prompt is left to the session's drop.
async fn drive_cancelled_turn_to_its_end(
    flight: &mut InFlightPrompt,
    permissions: &ParkedPermissions,
) {
    let waited = tokio::time::timeout(CANCEL_SETTLE_DEADLINE, async {
        loop {
            // A request the agent parks while it winds down is answered as cancelled too, so the
            // wait cannot stall on a question nothing will answer.
            permissions.cancel_pending();
            tokio::select! {
                end = flight.ended.as_mut() => break end,
                () = permissions.parked() => {}
            }
        }
    })
    .await;
    match waited {
        Ok(end) => flight.result = Some(end),
        Err(_) => log::warn!(
            "the coding agent did not end its cancelled turn within {CANCEL_SETTLE_DEADLINE:?}; \
             the turn is left to the session's drop"
        ),
    }
}

#[async_trait]
impl SubagentSession for CodingAgentSession {
    async fn take_turn(&mut self, request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        refuse_loop_only(&request)?;
        let answered = self.deliver_main_agent_answer(&request)?;
        let in_flight = Arc::clone(&self.in_flight);
        let mut slot = match answered {
            // Not waited on. A turn holding the slot is the one waiting for this answer, so waiting
            // behind it would never return; the answer is already with the coding agent.
            Some(request_id) => in_flight
                .try_lock()
                .map_err(|_| answer_delivered_while_held(request_id))?,
            None => in_flight.lock().await,
        };
        let mut flight = match slot.take() {
            Some(flight) => {
                if answered.is_none() {
                    if let Err(refusal) = self.resume(&request) {
                        *slot = Some(flight);
                        return Err(refusal);
                    }
                }
                flight
            }
            None => match answered {
                Some(request_id) => {
                    return Err(SubagentError::from(format!(
                        "permission request '{request_id}' was answered, but no prompt is in flight to continue"
                    )))
                }
                None => {
                    let prompt = new_prompt(&request)?;
                    self.begin_prompt(prompt)
                }
            },
        };
        match settle(
            &mut flight,
            &self.permissions,
            request.permission_deferred_to_user(),
        )
        .await
        {
            Settled::Paused(pending) => {
                *slot = Some(flight);
                let mut outcome = PromptOutcome::new(
                    StopReason::PermissionRequested,
                    Vec::new(),
                    TokenUsage::default(),
                );
                outcome.permission_request = Some(pending);
                Ok(outcome)
            }
            Settled::Ended(end) => {
                drop(slot);
                let InFlightPrompt {
                    prompt, updates, ..
                } = flight;
                let updates =
                    std::mem::take(&mut *updates.lock().unwrap_or_else(|e| e.into_inner()));
                self.finish(&prompt, updates, end?)
            }
        }
    }

    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.take_turn(TurnRequest::prompting(text)).await
    }

    fn model(&self) -> &str {
        &self.model
    }

    /// ACP reports no token usage, so there is nothing to total.
    fn cumulative_usage(&self) -> TokenUsage {
        TokenUsage::default()
    }

    /// ACP reports no context occupancy.
    fn context_tokens(&self) -> u64 {
        0
    }

    fn tail(&self, max_messages: usize) -> Vec<String> {
        let skip = self.history.len().saturating_sub(max_messages);
        self.history[skip..].to_vec()
    }

    fn updates(&self) -> Option<broadcast::Receiver<AcpUpdate>> {
        Some(self.update_stream.subscribe())
    }

    fn permission_answerer(&self) -> Option<Arc<dyn PermissionAnswering>> {
        Some(self.permission_answering())
    }
}
