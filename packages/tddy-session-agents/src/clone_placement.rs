//! `clone` placement on the daemon: a conversation of a clone-placed agent works on the conversation
//! worktree of its own, brought up to the caller's files before every turn.
//!
//! The worktree is `tddy-subagent-worktree`'s — created, synced and committed through its
//! [`ConversationWorktree`] as it stands. This module decides when each of those runs for a
//! conversation the daemon runs: at open, before every turn, and after every coding-agent turn.

use std::path::Path;

use async_trait::async_trait;
use tddy_discovery::openai::TokenUsage;
use tddy_discovery::subagent::{
    sync_notice, sync_refusal, PromptOutcome, SubagentError, SubagentSession, SyncAnswer,
    TurnRequest, WorktreeSync, WorktreeSyncPort, SYNC_NOTICE_PATHS,
};
use tddy_rpc::Status;
use tddy_subagent_worktree::{
    ConversationId, ConversationWorktree, ConversationWorktrees, SyncOutcome,
};

/// The subject a coding agent's turn is committed under on its conversation branch.
pub(crate) const CODING_AGENT_TURN_SUBJECT: &str = "A coding agent's turn";

/// The worktree of the conversation `conversation_id` on the session worktree `session_worktree`,
/// created now if the conversation has none yet. Its directory is where the conversation's agent
/// works, so the open is where it is cut.
pub(crate) async fn open_conversation_worktree(
    session_worktree: &Path,
    session_id: &str,
    conversation_id: &str,
) -> Result<ConversationWorktree, Status> {
    let conversation = ConversationId::parse(conversation_id)
        .map_err(|unsafe_id| Status::invalid_argument(unsafe_id.to_string()))?;
    ConversationWorktrees::new(session_worktree, session_id)
        .ensure(&conversation)
        .await
        .map_err(|e| {
            Status::internal(format!(
                "conversation '{conversation_id}' could not get its own worktree: {e}"
            ))
        })
}

/// The conversation worktree of `conversation_id` on the worktree of the session `session_id`, for
/// an agent whose launch does not already name that worktree.
pub(crate) async fn open_session_clone(
    session_id: &str,
    conversation_id: &str,
    session_dir: &Path,
    agent_id: &str,
) -> Result<ConversationWorktree, Status> {
    let metadata = crate::seed_codebase::read_session_metadata(session_id, session_dir)?;
    let session_worktree = crate::coding_agent::session_worktree(session_id, agent_id, &metadata)?;
    open_conversation_worktree(&session_worktree, session_id, conversation_id).await
}

/// What a sync found, in the shape a turn's sync answers with: the caller's files as they are
/// (`Nothing`), taken in (`Merged`), or in conflict with work the conversation has not pulled
/// (`Conflicted`, naming at most [`SYNC_NOTICE_PATHS`] of its paths).
fn sync_answer(outcome: SyncOutcome) -> SyncAnswer {
    match outcome {
        SyncOutcome::Unchanged => SyncAnswer::Nothing,
        SyncOutcome::Merged(sync) => SyncAnswer::Merged(sync),
        SyncOutcome::Conflicted { mut paths } => {
            let more_paths = paths.len().saturating_sub(SYNC_NOTICE_PATHS);
            paths.truncate(SYNC_NOTICE_PATHS);
            SyncAnswer::Conflicted { paths, more_paths }
        }
    }
}

/// Merge the caller's current files into `worktree`, as the turn-start sync of a turn loop.
async fn sync_with_caller(worktree: &ConversationWorktree) -> Result<SyncAnswer, SubagentError> {
    let outcome = worktree
        .sync_with_caller()
        .await
        .map_err(|e| SubagentError::from(e.to_string()))?;
    Ok(sync_answer(outcome))
}

/// The turn-start sync a model agent's turn loop asks for, through the port the loop already
/// consults before every turn: the loop announces what the merge changed to the model.
pub(crate) struct ConversationWorktreeSync {
    worktree: ConversationWorktree,
}

impl ConversationWorktreeSync {
    pub(crate) fn new(worktree: ConversationWorktree) -> Self {
        Self { worktree }
    }
}

#[async_trait]
impl WorktreeSyncPort for ConversationWorktreeSync {
    async fn sync(&self) -> Result<SyncAnswer, SubagentError> {
        sync_with_caller(&self.worktree).await
    }
}

/// A coding agent's conversation, run on its conversation worktree: the caller's files are merged
/// in before each turn, and what the turn changed is committed on the conversation branch after it.
///
/// A coding agent keeps its own history and its own tools, so it has no turn loop to hand the sync
/// notice to. The notice of a merge is held and rides on the next prompt the agent is sent: a turn
/// with no prompt (answering a permission request) sends none and leaves the notice held, and a
/// prompt-bearing turn that fails keeps its notice for the next one. A turn that skips the sync
/// (`syncWorktree: false`) merges nothing, so it holds no notice and is refused by no conflict. A
/// turn refused by a sync conflict never reaches the agent.
pub(crate) struct CodingAgentConversationTurns {
    session: Box<dyn SubagentSession>,
    worktree: ConversationWorktree,
    /// The notices of merges not yet sent to the agent, oldest first.
    pending_notices: Vec<String>,
}

impl CodingAgentConversationTurns {
    pub(crate) fn new(session: Box<dyn SubagentSession>, worktree: ConversationWorktree) -> Self {
        Self {
            session,
            worktree,
            pending_notices: Vec::new(),
        }
    }

    /// The held notices as one preface, in the order they were held, leaving none held. `None` when
    /// none is held.
    fn take_held_notices(&mut self) -> Option<String> {
        let held = std::mem::take(&mut self.pending_notices);
        (!held.is_empty()).then(|| held.join("\n\n"))
    }
}

/// Merge the caller's files into `worktree` before a coding agent's turn, refusing the turn when they
/// conflict with work the conversation has not pulled. Answers the merge when there was one, so its
/// notice can be held for the agent's next prompt.
///
/// A free function over the worktree alone: a turn future must not hold a borrow of the session,
/// which is not `Sync`.
async fn take_in_callers_files(
    worktree: &ConversationWorktree,
) -> Result<Option<WorktreeSync>, SubagentError> {
    match sync_with_caller(worktree).await? {
        SyncAnswer::Conflicted { paths, more_paths } => Err(sync_refusal(&paths, more_paths, None)),
        SyncAnswer::Merged(sync) => Ok(Some(sync)),
        SyncAnswer::Nothing => Ok(None),
    }
}

#[async_trait]
impl SubagentSession for CodingAgentConversationTurns {
    async fn take_turn(&mut self, request: TurnRequest) -> Result<PromptOutcome, SubagentError> {
        // A turn that skips the sync (`syncWorktree: false`) runs on the worktree as it stands.
        let merged = if request.syncs_worktree() {
            take_in_callers_files(&self.worktree).await?
        } else {
            None
        };
        if let Some(sync) = merged {
            self.pending_notices.push(sync_notice(&sync));
        }
        // The held notices ride on the prompt the agent is sent: the same words a model loop gives
        // its model. A turn with no new prompt (answering a permission request) has nothing to ride
        // on, so its notices stay held for the next prompt-bearing turn.
        let preface = if request.prompt_text().is_some() {
            self.take_held_notices()
        } else {
            None
        };
        let request = match &preface {
            Some(notices) => request.with_prompt_preface(notices),
            None => request,
        };
        let outcome = self.session.take_turn(request).await;
        if let (Err(_), Some(notices)) = (&outcome, preface) {
            // A failed turn does not say whether the agent read its prompt, so the notices it
            // carried wait for the next prompt rather than being dropped.
            self.pending_notices.insert(0, notices);
        }
        // The turn ran whether or not it answered: what it changed is on disk and is committed
        // even when the turn itself failed.
        let committed = self
            .worktree
            .commit_changes(CODING_AGENT_TURN_SUBJECT)
            .await;
        match (outcome, committed) {
            (outcome, Ok(_)) => outcome,
            (Ok(_), Err(e)) => Err(SubagentError::from(format!(
                "the turn ran, but its changes could not be committed to the conversation's \
                 worktree: {e}"
            ))),
            (Err(turn), Err(e)) => {
                log::warn!(
                    target: "tddy_session_agents::clone_placement",
                    "turn changes of conversation on {} not committed: {e}",
                    self.worktree.root().display()
                );
                Err(turn)
            }
        }
    }

    async fn prompt(&mut self, text: &str) -> Result<PromptOutcome, SubagentError> {
        self.take_turn(TurnRequest::prompting(text)).await
    }

    fn model(&self) -> &str {
        self.session.model()
    }

    fn cumulative_usage(&self) -> TokenUsage {
        self.session.cumulative_usage()
    }

    fn context_tokens(&self) -> u64 {
        self.session.context_tokens()
    }

    fn tail(&self, max_messages: usize) -> Vec<String> {
        self.session.tail(max_messages)
    }

    fn permission_answerer(
        &self,
    ) -> Option<std::sync::Arc<dyn tddy_discovery::subagent::PermissionAnswering>> {
        self.session.permission_answerer()
    }
}

#[cfg(test)]
mod sync_notice_tests;
