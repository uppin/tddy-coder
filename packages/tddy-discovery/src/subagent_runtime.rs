//! Every conversation this session has open with a subagent, the turns running on them, and what
//! each has spent.
//!
//! Moved here from `tddy-tools`' `server.rs` by `#unbundle` node 5. It is logic over this crate's
//! own types — [`crate::subagent::SubagentSession`], [`crate::subagent::PromptOutcome`],
//! [`crate::openai::TokenUsage`] — and over [`crate::roster`]'s conversation link; nothing in it
//! speaks MCP. What stayed behind in `tddy-tools` is the MCP surface that drives it: the
//! `subagent_*` tool bodies, their input schemas and the `ToolRouter` they are advertised through.
//!
//! Two properties are the reason the table is here rather than inside any one server instance:
//!
//! - a conversation outlives the `tools/call` that opened it, so the table is process-wide
//!   ([`subagent_sessions`]);
//! - a turn outlives the call that *started* it, so its result is published into the table rather
//!   than returned from the task ([`PendingTurns`]), and any number of awaiters can collect it.

use std::collections::HashMap;
use std::sync::OnceLock;

use tddy_core::spawn_env::env_non_empty;
use tddy_service::proto::connection::SessionAgentStatus;

use crate::subagent::{PromptOutcome, SubagentError, SubagentSession};

/// The error envelope a subagent tool returns, so a failure is a *result* an agent can read rather
/// than a transport error it never sees.
///
/// Here rather than in `tddy-tools`' `mcp_primitives`, because the runtime mints these answers
/// itself: a turn that fails and a conversation cancelled underneath its awaiters are both
/// resolved with one. `tddy-tools`' tool bodies use the same function, so there is exactly one
/// spelling of the envelope the main agent reads.
pub fn subagent_error_json(message: impl std::fmt::Display) -> String {
    serde_json::json!({ "error": message.to_string(), "is_error": true }).to_string()
}

/// One open subagent conversation plus the accounting metadata that lives alongside the session
/// (its agent name, model, turn count and cumulative token usage).
///
/// The accounting is **copied out of the session** — at open, and again at the end of every turn —
/// rather than read back through it. A turn runs in its own task holding [`Self::session`], so a
/// record read off the session would either block behind the turn or report a partially-billed one;
/// the fields here report the conversation as of its last completed turn, which is the truthful
/// answer while another is in flight (docs/ft/coder/managed-codebase-subagents.md criterion 32).
pub struct SubagentConversation {
    pub agent: String,
    turns: u32,
    /// The model the session talks to, as it named itself at open.
    model: String,
    /// Token usage across the turns that have **ended**. A turn in flight has spent nothing this
    /// session can attribute to it yet.
    usage: crate::openai::TokenUsage,
    /// The turn loop, behind the lock that serializes turns on *this* conversation and nothing
    /// else. A conversation's history is one sequence, so two turns must not run against it at
    /// once; `tokio::sync::Mutex` is fair, so waiting for it is the queue (criterion 30).
    pub session: std::sync::Arc<tokio::sync::Mutex<Box<dyn SubagentSession>>>,
    /// The daemon-side conversation this one is a handle to, when the turn loop runs elsewhere.
    /// Ending it here has to close it there too: the owning daemon otherwise keeps the loop, and
    /// its own registration of it, for the life of its process.
    pub remote: Option<crate::roster::RemoteConversationHandle>,
}

impl SubagentConversation {
    /// Adopt a freshly opened session, taking its accounting identity from the session itself.
    pub fn opened(
        agent: String,
        session: Box<dyn SubagentSession>,
        remote: Option<crate::roster::RemoteConversationHandle>,
    ) -> Self {
        Self {
            agent,
            turns: 0,
            model: session.model().to_string(),
            usage: session.cumulative_usage(),
            session: std::sync::Arc::new(tokio::sync::Mutex::new(session)),
            remote,
        }
    }
}

/// Every conversation this process has run: the ones still open, and the accounting of the ones
/// that ended.
#[derive(Default)]
pub struct SubagentConversations {
    pub open: HashMap<String, SubagentConversation>,
    /// Conversations that ended — cancelled by the main agent, or whose agent was detached
    /// underneath them.
    ///
    /// Their tokens were spent, so they stay enumerable: the accounting file is rewritten wholesale
    /// from this table, and dropping a conversation before the rewrite erases its totals from the
    /// host's view of what the session cost.
    retired: Vec<tddy_core::token_accounting::ConversationRecord>,
    /// Turns that outlived the call that started them, keyed by the `responseId` their caller was
    /// given. They live in the same table as the conversations they belong to, so one lock covers
    /// both a turn's accounting and the publication of its result.
    pub pending: PendingTurns,
}

impl SubagentConversations {
    /// End `conversation_id`, keeping its accounting. Returns whether it was open.
    pub fn retire(&mut self, conversation_id: &str) -> bool {
        let Some(conversation) = self.open.remove(conversation_id) else {
            return false;
        };
        self.retired
            .push(conversation_record(conversation_id, &conversation));
        true
    }
}

pub type SubagentSessionTable = tokio::sync::Mutex<SubagentConversations>;

/// Process-wide session table — `PermissionServer` merges the subagent router at construction
/// time, but the conversation must survive across separate `tools/call` invocations, so the table
/// lives outside any single `PermissionServer` instance.
///
/// # Precondition: one session per process
///
/// This is a `OnceLock`, so every caller in the process shares **one** conversation table, keyed
/// by conversation id alone with no session in the key. That was sound where this code came from —
/// `tddy-tools` is one in-jail process serving exactly one session — but `tddy-discovery` is
/// linked by `tddy-daemon`, which serves **many sessions per process**. A second session calling
/// this in the same process would share the first's open conversations and its retired accounting,
/// and could retire or resolve a turn belonging to the other session. A multi-session host needs a
/// `SubagentSessionTable` per session (`SubagentConversations::default()` is all this constructs),
/// not this singleton. Nothing in `tddy-discovery` or `tddy-daemon` calls it today.
pub fn subagent_sessions() -> &'static SubagentSessionTable {
    static SESSIONS: OnceLock<SubagentSessionTable> = OnceLock::new();
    SESSIONS.get_or_init(|| tokio::sync::Mutex::new(SubagentConversations::default()))
}

/// Where a turn that outlived the call that started it has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnState {
    Running,
    /// The turn ended. Carries its already-serialized result — an outcome or a failure alike, since
    /// both are answers and only a turn still running is not.
    Done(String),
}

/// One turn that outlived its call: which conversation it belongs to, how its result reaches
/// everyone waiting, and how to stop it if the conversation is closed underneath it.
///
/// The **table** holds the `watch::Sender`, not the running task. A turn therefore publishes its
/// result by taking the table lock at its end — the lock it already needs to update the
/// conversation's accounting — and any number of awaiters hold a `Receiver` without holding the
/// table.
struct PendingTurn {
    conversation_id: String,
    publish: tokio::sync::watch::Sender<TurnState>,
    abort: tokio::task::AbortHandle,
}

/// Every turn this process deferred, keyed by the `responseId` its caller was given.
///
/// Entries are not consumed by collecting them: a tool result lost between this process and the
/// main agent would otherwise make a computed answer permanently unreachable
/// (docs/ft/coder/managed-codebase-subagents.md criterion 29).
#[derive(Default)]
pub struct PendingTurns {
    turns: HashMap<String, PendingTurn>,
}

impl PendingTurns {
    /// Register a turn about to run on `conversation_id` under the id its caller will be given.
    pub fn start(
        &mut self,
        response_id: &str,
        conversation_id: &str,
        abort: tokio::task::AbortHandle,
    ) {
        let (publish, _) = tokio::sync::watch::channel(TurnState::Running);
        self.turns.insert(
            response_id.to_string(),
            PendingTurn {
                conversation_id: conversation_id.to_string(),
                publish,
                abort,
            },
        );
    }

    /// A receiver for `response_id`, for a caller that wants to wait on it. `None` means no turn
    /// was ever registered under that id — which a tool reports as an error, never as "still
    /// running".
    pub fn watch(&self, response_id: &str) -> Option<tokio::sync::watch::Receiver<TurnState>> {
        self.turns
            .get(response_id)
            .map(|turn| turn.publish.subscribe())
    }

    /// Publish `result` as the answer to `response_id`, releasing everyone waiting on it.
    ///
    /// A turn that has already ended keeps the answer it ended with: a result landing after a
    /// cancel would otherwise overwrite the cancellation everyone was already told about.
    pub fn resolve(&mut self, response_id: &str, result: String) {
        let Some(turn) = self.turns.get(response_id) else {
            return;
        };
        turn.publish.send_if_modified(|state| match state {
            TurnState::Running => {
                *state = TurnState::Done(result);
                true
            }
            TurnState::Done(_) => false,
        });
    }

    /// Answer every turn still running on `conversation_id` with `reason`, and stop them.
    ///
    /// Every turn, not just the one in flight: prompts queued behind it belong to a conversation
    /// that is gone, so each one has a caller holding an id nothing else will ever answer. A turn
    /// that already ended is left as it ended — a cancel arriving after the answer must not replace
    /// it with a cancellation.
    pub fn cancel_conversation(&mut self, conversation_id: &str, reason: &str) {
        for turn in self.turns.values() {
            if turn.conversation_id != conversation_id {
                continue;
            }
            let stopped = turn.publish.send_if_modified(|state| match state {
                TurnState::Running => {
                    *state = TurnState::Done(subagent_error_json(reason));
                    true
                }
                TurnState::Done(_) => false,
            });
            if stopped {
                turn.abort.abort();
            }
        }
    }

    /// Drop a turn whose id its caller was never given — one that yielded inside its grace period,
    /// so nothing can ever name it.
    pub fn forget(&mut self, response_id: &str) {
        self.turns.remove(response_id);
    }
}

/// Block on `watched` until the turn behind it has ended, for at most `budget`.
///
/// `Some` is the turn's already-serialized result; `None` means it is still running, which is a
/// deferral rather than a failure — the turn keeps going and its id stays collectable.
pub async fn wait_for_turn(
    watched: &mut tokio::sync::watch::Receiver<TurnState>,
    budget: std::time::Duration,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        if let TurnState::Done(result) = &*watched.borrow_and_update() {
            return Some(result.clone());
        }
        // A closed channel is a turn whose entry was dropped while this caller held a receiver.
        // Nothing more will ever be published on it, so waiting on it again would never return.
        match tokio::time::timeout_at(deadline, watched.changed()).await {
            Ok(Ok(())) => continue,
            Ok(Err(_)) | Err(_) => return None,
        }
    }
}

pub fn prompt_outcome_json(outcome: PromptOutcome) -> String {
    serde_json::json!({
        "stopReason": outcome.stop_reason,
        "content": outcome.content,
        "usage": {
            "inputTokens": outcome.usage.input_tokens,
            "outputTokens": outcome.usage.output_tokens,
            "totalTokens": outcome.usage.total(),
        },
    })
    .to_string()
}

/// One conversation as the shared [`tddy_core::token_accounting::ConversationRecord`] shape used by
/// `subagent_list` and the accounting file.
fn conversation_record(
    id: &str,
    conv: &SubagentConversation,
) -> tddy_core::token_accounting::ConversationRecord {
    tddy_core::token_accounting::ConversationRecord {
        agent: conv.agent.clone(),
        id: id.to_string(),
        model: conv.model.clone(),
        input_tokens: conv.usage.input_tokens,
        output_tokens: conv.usage.output_tokens,
        total_tokens: conv.usage.total(),
        turns: conv.turns,
    }
}

/// Every conversation this process has run, open ones first. The retired ones are included because
/// their tokens were spent by this session: an accounting file that lists only what is still open
/// reports a detached agent's consumption as zero.
pub fn conversation_records(
    conversations: &SubagentConversations,
) -> Vec<tddy_core::token_accounting::ConversationRecord> {
    conversations
        .open
        .iter()
        .map(|(id, conv)| conversation_record(id, conv))
        .chain(conversations.retired.iter().cloned())
        .collect()
}

/// Overwrite the host-visible accounting file (`TDDY_TOOLS_ACCOUNTING_FILE`, pointed by the runner
/// into the session egress dir) with the current conversation list. A no-op when the env var is
/// unset; write failures are ignored — accounting is best-effort telemetry, never load-bearing.
pub fn write_accounting_file(conversations: &SubagentConversations) {
    let Some(path) = env_non_empty("TDDY_TOOLS_ACCOUNTING_FILE") else {
        return;
    };
    let payload = serde_json::json!({ "conversations": conversation_records(conversations) });
    if let Ok(text) = serde_json::to_string_pretty(&payload) {
        let _ = std::fs::write(&path, text);
    }
}

/// One prompt turn, and everything it needs to run without the table lock.
pub struct DeferredTurn {
    pub response_id: String,
    pub conversation_id: String,
    pub prompt_text: String,
    pub session: std::sync::Arc<tokio::sync::Mutex<Box<dyn SubagentSession>>>,
    /// The agent to report this conversation's state as, when the loop runs in this process.
    pub reported_agent: Option<String>,
}

/// Run one turn to completion, whether or not the call that started it is still waiting.
///
/// Acquires the conversation's turn lock first and prompts second, so a prompt that arrives
/// mid-turn queues behind the running one and runs against the history it leaves (criterion 30).
/// At the end it takes the table lock once — for the conversation's accounting, the accounting
/// file, and the publication of the result — so no awaiter can read a turn as done before what it
/// spent has been recorded.
pub async fn run_turn(turn: DeferredTurn) {
    let mut session = turn.session.lock().await;
    if let Some(agent_id) = turn.reported_agent.as_deref() {
        report_local_conversation_state(
            agent_id,
            SessionAgentStatus::Running,
            &format!("prompted: {}", turn.prompt_text),
        )
        .await;
    }
    let ended = TurnEnd::from(session.prompt(&turn.prompt_text).await);
    // Read while the turn lock is still held, and kept held until the table is updated: releasing
    // it first would let the next queued turn end and record its own totals underneath this one.
    let usage = session.cumulative_usage();

    let mut sessions = subagent_sessions().lock().await;
    if let Some(conv) = sessions.open.get_mut(&turn.conversation_id) {
        // A turn that failed still counts what it spent reaching that failure, but is not a turn
        // the conversation took: nothing was added to its history.
        conv.usage = usage;
        if ended.took_a_turn {
            conv.turns += 1;
        }
    }
    write_accounting_file(&sessions);
    sessions.pending.resolve(&turn.response_id, ended.result);
    drop(sessions);
    drop(session);

    if let Some(agent_id) = turn.reported_agent.as_deref() {
        // Idle either way, including on a failure: the agent is still attached and still
        // promptable, and ERROR on a roster row means the checkout is broken. The summary is what
        // says what happened.
        report_local_conversation_state(agent_id, SessionAgentStatus::Idle, &ended.summary).await;
    }
}

/// How a turn ended, in the three forms its end is recorded in: the result its caller collects, the
/// line the roster row shows, and whether the conversation's history grew by it.
struct TurnEnd {
    /// The serialized answer — an outcome or a failure, since both are answers.
    result: String,
    /// One line for the agent's roster row, saying what happened.
    summary: String,
    /// Whether this counts as a turn the conversation took. A failure spent tokens but added
    /// nothing to the history, so it is not one.
    took_a_turn: bool,
}

impl From<Result<PromptOutcome, SubagentError>> for TurnEnd {
    fn from(outcome: Result<PromptOutcome, SubagentError>) -> Self {
        match outcome {
            Ok(outcome) => {
                let chars = outcome
                    .content
                    .iter()
                    .map(|block| block.text.chars().count())
                    .sum::<usize>();
                Self {
                    result: prompt_outcome_json(outcome),
                    summary: format!("answered ({chars} chars)"),
                    took_a_turn: true,
                }
            }
            Err(e) => Self {
                result: subagent_error_json(&e),
                summary: format!("turn failed: {e}"),
                took_a_turn: false,
            },
        }
    }
}

/// Tell the facilitating daemon what a conversation **this process** runs is doing.
///
/// Only for a loop that runs here. A conversation the daemon runs is one the daemon already sees —
/// it serves the open and the prompt itself — and a second account of it from this side would race
/// the daemon's own, which is how a row gets parked at RUNNING after the turn it describes has
/// finished.
///
/// Best-effort throughout, including the connect: a status is a display signal, and failing a turn
/// because a badge could not be updated would trade a stale badge for a broken conversation. Logged
/// at `debug` rather than `error` for the same reason — a session with no daemon in the loop at all
/// (`tddy-sandbox-app`) reaches this on every turn, and it is not a fault there.
pub async fn report_local_conversation_state(
    agent_id: &str,
    status: SessionAgentStatus,
    summary: &str,
) {
    let link = match crate::roster::AgentConversationLink::connect().await {
        Ok(link) => link,
        Err(e) => {
            log::debug!(
                target: "tddy_discovery::subagent_runtime",
                "not reporting '{agent_id}' as {status:?}: {e}"
            );
            return;
        }
    };
    if let Err(e) = link.report_state(agent_id, status, summary).await {
        log::debug!(
            target: "tddy_discovery::subagent_runtime",
            "could not report '{agent_id}' as {status:?}: {e}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::TokenUsage;
    use crate::subagent::{ContentBlock, StopReason};

    /// A task that will never finish on its own — the abort handle a registered turn is held by,
    /// with nothing else about a real turn to get in the way.
    fn a_turn_still_running() -> tokio::task::AbortHandle {
        tokio::spawn(std::future::pending::<()>()).abort_handle()
    }

    fn an_end_turn_outcome(answer: &str) -> PromptOutcome {
        PromptOutcome {
            stop_reason: StopReason::EndTurn,
            content: vec![ContentBlock::text(answer)],
            usage: TokenUsage {
                input_tokens: 30,
                output_tokens: 12,
            },
        }
    }

    /// What a `watch` receiver is holding right now, as a test reads it.
    fn state_of(receiver: &tokio::sync::watch::Receiver<TurnState>) -> TurnState {
        receiver.borrow().clone()
    }

    /// Registering a turn is what makes its id answerable at all — before it resolves, the honest
    /// answer to "is it done" is "no", not "I have never heard of it".
    #[tokio::test]
    async fn a_started_turn_is_watchable_and_reads_as_running() {
        // Given a turn registered against its conversation
        let mut pending = PendingTurns::default();
        pending.start("response-1", "conv-1", a_turn_still_running());

        // When a caller asks to watch it
        let watched = pending
            .watch("response-1")
            .expect("a started turn must be watchable");

        // Then it reads as still running
        assert_eq!(state_of(&watched), TurnState::Running);
    }

    /// Every caller parked on a turn has to be released by it — an await that arrived while the
    /// turn ran and one that arrived after must not get different answers.
    #[tokio::test]
    async fn resolving_a_turn_answers_everyone_watching_it() {
        // Given two callers already watching one running turn
        let mut pending = PendingTurns::default();
        pending.start("response-1", "conv-1", a_turn_still_running());
        let first = pending
            .watch("response-1")
            .expect("a started turn must be watchable");
        let second = pending
            .watch("response-1")
            .expect("a started turn must be watchable");

        // When the turn ends
        pending.resolve(
            "response-1",
            prompt_outcome_json(an_end_turn_outcome("src/auth.rs:1-50")),
        );

        // Then both hold the same finished result
        let done = TurnState::Done(prompt_outcome_json(an_end_turn_outcome("src/auth.rs:1-50")));
        assert_eq!(state_of(&first), done);
        assert_eq!(state_of(&second), done);
    }

    /// A tool result lost between this process and the main agent must not make a computed answer
    /// permanently unreachable, so collecting one does not consume it.
    #[tokio::test]
    async fn a_resolved_turn_stays_claimable() {
        // Given a turn that has already ended and been collected once
        let mut pending = PendingTurns::default();
        pending.start("response-1", "conv-1", a_turn_still_running());
        pending.resolve(
            "response-1",
            prompt_outcome_json(an_end_turn_outcome("src/auth.rs:1-50")),
        );
        let _collected = pending
            .watch("response-1")
            .expect("a resolved turn must be watchable");

        // When a later caller asks for it again
        let again = pending
            .watch("response-1")
            .expect("a claimed turn must stay watchable");

        // Then the same answer is still there
        assert_eq!(
            state_of(&again),
            TurnState::Done(prompt_outcome_json(an_end_turn_outcome("src/auth.rs:1-50")))
        );
    }

    /// An id nothing was ever stored under has to be distinguishable from one whose turn is slow —
    /// a caller told "running" would poll forever for an answer that is never coming.
    #[tokio::test]
    async fn a_turn_nobody_started_is_unknown_rather_than_running() {
        // Given a table with one turn in it
        let mut pending = PendingTurns::default();
        pending.start("response-1", "conv-1", a_turn_still_running());

        // When a caller asks about an id that was never handed out
        let watched = pending.watch("response-that-never-was");

        // Then there is nothing to watch, so the tool can refuse rather than wait
        assert!(
            watched.is_none(),
            "an unregistered response id must not read as a running turn"
        );
    }

    /// Closing a conversation has to answer everyone parked on its turns. Leaving them unresolved
    /// would strand an agent on an await that nothing will ever complete.
    #[tokio::test]
    async fn cancelling_a_conversation_resolves_every_turn_it_had_in_flight() {
        // Given two turns running on one conversation and one on another
        let mut pending = PendingTurns::default();
        pending.start("response-1", "conv-doomed", a_turn_still_running());
        pending.start("response-2", "conv-doomed", a_turn_still_running());
        pending.start("response-3", "conv-untouched", a_turn_still_running());

        // When the first conversation is closed
        pending.cancel_conversation("conv-doomed", "the agent was detached");

        // Then both of its turns are answered, naming why
        for response_id in ["response-1", "response-2"] {
            let watched = pending
                .watch(response_id)
                .expect("a cancelled turn stays watchable");
            let TurnState::Done(result) = state_of(&watched) else {
                panic!("{response_id} must be resolved by the cancel, not left running");
            };
            assert!(
                result.contains("the agent was detached"),
                "a cancelled turn must say why it will never answer; got: {result}"
            );
        }

        // And the other conversation's turn is untouched
        let untouched = pending
            .watch("response-3")
            .expect("an unrelated turn stays watchable");
        assert_eq!(state_of(&untouched), TurnState::Running);
    }

    /// A turn that yielded inside its grace period was answered on the call itself, so its id was
    /// never handed out. Keeping it would be a per-turn leak with no reader.
    #[tokio::test]
    async fn a_turn_whose_id_was_never_handed_out_is_forgotten() {
        // Given a turn that resolved before its caller gave up waiting
        let mut pending = PendingTurns::default();
        pending.start("response-1", "conv-1", a_turn_still_running());
        pending.resolve(
            "response-1",
            prompt_outcome_json(an_end_turn_outcome("src/auth.rs:1-50")),
        );

        // When the call that started it returns the outcome directly
        pending.forget("response-1");

        // Then nothing holds it any more
        assert!(
            pending.watch("response-1").is_none(),
            "a response id the caller was never given must not be retained"
        );
    }
}
