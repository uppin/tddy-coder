//! What a turn does before its first model call: forget what the caller's input invalidates, rewind
//! (taking the worktree back first), take in the caller's current files, then append the turn's
//! messages.
//!
//! The order is the contract (PRD-2026-10-03-agent-worktree-caller-sync): a reset that fails refuses
//! the resume with the history whole; the sync runs on the worktree the turn will run on, and a
//! conflict refuses the turn before anything is appended or sent; the sync's notice is appended last.

use crate::openai::ChatMessage;

use super::worktree_sync::{self, sync_notice, RewindApplied};
use super::{SpecializedSubagentSession, SubagentError, TurnRequest, WorktreeReset, WorktreeSync};

/// What a turn did to the conversation's worktree before it ran — reported on its outcome.
pub(super) struct WorktreeAtTurnStart {
    pub(super) reset: Option<WorktreeReset>,
    pub(super) sync: Option<WorktreeSync>,
}

impl SpecializedSubagentSession {
    /// Everything a turn does to the conversation before it appends a message: clear the repeat
    /// ledger when the caller brings something new, rewind, and sync.
    pub(super) async fn prepare_turn(
        &mut self,
        request: &TurnRequest,
    ) -> Result<WorktreeAtTurnStart, SubagentError> {
        // Anything that changes the conversation from outside the loop — a new question, a
        // correction, a rewind that discards answers it was holding, a replacement appended
        // after the yield — invalidates the premise the repeat ledger rests on (see
        // [`super::RepeatedCalls::forget_earlier_calls`]).
        if request.prompt_text().is_some()
            || request.correction().is_some()
            || request.rewind_point().is_some()
            || request.replacement().is_some()
        {
            self.repeated_calls.forget_earlier_calls();
        }
        let (reset, rewind) = self.rewind(request).await?;
        // After the reset, so the caller's files merge into the worktree the turn will run on; before
        // anything is appended, so a conflict refuses the turn before a single model call.
        let sync =
            worktree_sync::take_in_callers_files(self.worktree_sync.as_deref(), request, rewind)
                .await?;
        if sync.is_some() {
            // A read the ledger remembers can now return different content.
            self.repeated_calls.forget_earlier_calls();
        }
        Ok(WorktreeAtTurnStart { reset, sync })
    }

    /// Rewind to the request's rewind point, if it names one — the worktree first: a reset that fails
    /// refuses the resume with the history whole, which a reset after the cut could not promise.
    async fn rewind(
        &mut self,
        request: &TurnRequest,
    ) -> Result<(Option<WorktreeReset>, Option<RewindApplied>), SubagentError> {
        let Some(rewind_point) = request.rewind_point() else {
            return Ok((None, None));
        };
        let mut reset = None;
        if let (Some(port), true) = (&self.worktree_reset, request.resets_worktree()) {
            let target = self
                .transcript
                .commit_kept_by(rewind_point)
                .map_err(|e| SubagentError(e.to_string()))?;
            reset = port.reset(target).await?;
        }
        self.transcript
            .rewind_to(rewind_point)
            .map_err(|e| SubagentError(e.to_string()))?;
        let applied = match reset {
            Some(_) => RewindApplied::HistoryAndWorktree,
            None => RewindApplied::History,
        };
        Ok((reset, Some(applied)))
    }

    /// Append the turn's prompt, correction, replacement and sync notice, in that order.
    pub(super) fn append_turn_messages(
        &mut self,
        request: &TurnRequest,
        sync: Option<&WorktreeSync>,
    ) {
        if let Some(text) = request.prompt_text() {
            self.transcript.push(ChatMessage::user(text.to_string()));
        }
        if let Some(correction) = request.correction() {
            self.transcript
                .push(ChatMessage::user(correction.to_string()));
        }
        // The caller's replacement call and its result, appended after any rewind and correction —
        // in that order, so all three can be given. Resume-only: a fresh prompt never carries one
        // (a prompt has nothing to replace), enforced where the RPC shapes are built. Validated
        // before the rewind, so a malformed one never reshapes the history it was refused from.
        // The append never dispatches — the result is the caller's text, verbatim.
        if let Some(replacement) = request.replacement() {
            self.transcript.append_replacement(replacement);
        }
        // Last, so it follows a replacement's completed tool-call group and is what the model reads
        // right before it acts.
        if let Some(sync) = sync {
            self.transcript.push(ChatMessage::user(sync_notice(sync)));
        }
    }
}
