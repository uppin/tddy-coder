//! What a caller asks a conversation to do next: ask a new question, carry on where it stopped, or
//! go back to a named message and try again — each optionally within a turn budget of the caller's
//! choosing.

use super::transcript::MessageId;

/// The most turns one call may spend, whatever it asks for.
///
/// A budget the caller chooses is a caller choosing how much local-model time to spend, and an
/// unbounded one is unbounded spend. Fifty is far above any search that has ever been useful and
/// far below a runaway loop. A request above it is clamped rather than refused — an over-eager
/// caller keeps working — and the clamp is *reported*, because a budget silently smaller than the
/// one requested is how a caller reads an early stop as a finished search.
pub const SUBAGENT_MAX_TURNS_CEILING: u32 = 50;

/// The fewest turns a caller may buy. A budget below this buys no search at all and, because the
/// turn loop would then exit before any tool call could fail, lands straight in the synthesis turn
/// — a summary of nothing. Clamped rather than refused, and reported, exactly like the ceiling.
pub const SUBAGENT_MIN_TURNS: u32 = 1;

/// One request to take a turn on an open conversation.
///
/// Built from one of two starting points, so the two intents cannot be confused:
/// [`TurnRequest::prompting`] asks something new, and [`TurnRequest::resuming`] continues what is
/// already there without asking anything. The builders then narrow it: [`Self::from_message`]
/// rewinds first, [`Self::with_correction`] says what was wrong, [`Self::within_turns`] sets the
/// budget for this call alone.
#[derive(Debug, Clone, Default)]
pub struct TurnRequest {
    prompt: Option<String>,
    from_message: Option<MessageId>,
    correction: Option<String>,
    max_turns: Option<u32>,
    /// The conditions on a tool call that stop this turn at that call and hand control back to
    /// the caller ([`crate::subagent::yield_condition`]). For **this** turn only, never the
    /// conversation.
    yield_conditions: Vec<super::yield_condition::YieldCondition>,
    /// A caller-provided tool call and its result, appended after the original (yielded) call —
    /// the keep-original + append replacement
    /// ([`crate::subagent::replacement::Replacement`]). Never dispatched; recorded as history.
    replacement: Option<super::replacement::Replacement>,
    /// The caller asked that a rewind leave the conversation's worktree as it is
    /// (`resetWorktree: false`).
    keep_worktree: bool,
    /// The caller asked that this turn run on the conversation worktree as it stands, without taking
    /// in the caller's current files (`syncWorktree: false`).
    skip_sync: bool,
}

impl TurnRequest {
    /// Ask the conversation something new.
    pub fn prompting(text: impl Into<String>) -> Self {
        Self {
            prompt: Some(text.into()),
            ..Self::default()
        }
    }

    /// Carry on from what the conversation already holds, sending no new user message.
    ///
    /// The plain case is a chain cut short by its budget: re-asking would make the agent re-read
    /// everything it has already read, which is the cost the resume exists to avoid.
    pub fn resuming() -> Self {
        Self::default()
    }

    /// Go back to `id` first, discarding everything the conversation appended after it.
    pub fn from_message(mut self, id: MessageId) -> Self {
        self.from_message = Some(id);
        self
    }

    /// Append one corrective instruction after the rewind point.
    ///
    /// What makes a rewind able to change anything at all: requests go out at `temperature: 0.0`,
    /// so re-running identical context reproduces the same turn. Without a correction a rewind is
    /// a feature that silently does nothing.
    pub fn with_correction(mut self, text: impl Into<String>) -> Self {
        self.correction = Some(text.into());
        self
    }

    /// Rewind the transcript only: the conversation's worktree keeps every file the dropped
    /// messages wrote, and later commits build on them.
    #[must_use]
    pub fn keeping_worktree(mut self) -> Self {
        self.keep_worktree = true;
        self
    }

    /// Run this turn on the conversation worktree as it stands: the caller's current files are not
    /// merged in first.
    #[must_use]
    pub fn without_sync(mut self) -> Self {
        self.skip_sync = true;
        self
    }

    /// Whether this turn first takes in the caller's current files — the default.
    pub fn syncs_worktree(&self) -> bool {
        // TODO(caller-sync): implement
        let _ = self.skip_sync;
        todo!("syncs_worktree")
    }

    /// Whether a rewind by this request takes the conversation's worktree back too — the default.
    pub fn resets_worktree(&self) -> bool {
        !self.keep_worktree
    }

    /// Spend at most `max_turns` model turns on this call, in place of the agent definition's own
    /// budget — for this call only, and never above [`SUBAGENT_MAX_TURNS_CEILING`].
    pub fn within_turns(mut self, max_turns: u32) -> Self {
        self.max_turns = Some(max_turns);
        self
    }

    /// Yield this turn back to the caller the moment one of `conditions` fires on a tool call —
    /// for this call only, never the conversation.
    pub fn with_yield_conditions(
        mut self,
        conditions: Vec<super::yield_condition::YieldCondition>,
    ) -> Self {
        self.yield_conditions = conditions;
        self
    }

    /// The conditions this turn watches tool calls for; empty when the caller set none.
    pub fn yield_conditions(&self) -> &[super::yield_condition::YieldCondition] {
        &self.yield_conditions
    }

    /// Append `replacement`'s call and result after the conversation's last message — resume-only
    /// (a fresh prompt has nothing to replace), and after any rewind and correction, in that
    /// order.
    pub fn with_replacement(mut self, replacement: super::replacement::Replacement) -> Self {
        self.replacement = Some(replacement);
        self
    }

    /// The replacement this resume appends, when it carries one.
    pub fn replacement(&self) -> Option<&super::replacement::Replacement> {
        self.replacement.as_ref()
    }

    /// The new question this request asks, if it asks one.
    pub fn prompt_text(&self) -> Option<&str> {
        self.prompt.as_deref()
    }

    /// The message this request rewinds to, if it rewinds.
    pub fn rewind_point(&self) -> Option<&MessageId> {
        self.from_message.as_ref()
    }

    /// The corrective instruction this request appends, if it appends one.
    pub fn correction(&self) -> Option<&str> {
        self.correction.as_deref()
    }

    /// The turn budget the caller asked for, before the ceiling is applied.
    pub fn requested_max_turns(&self) -> Option<u32> {
        self.max_turns
    }

    /// The budget this call actually runs under, given the agent definition's own.
    ///
    /// The caller's choice wins where it made one, because it is choosing how long *this* search
    /// may take rather than what the agent runs against — the endpoint, model and credential stay
    /// the operator's alone. Only the caller's figure meets the bounds: the definition's budget is
    /// the operator's own configuration, and clamping it here would quietly override a deliberate
    /// setting nobody in this call asked to change.
    ///
    /// **The floor matters as much as the ceiling, and for a sharper reason.** The turn loop runs
    /// `0..turns`, so a budget of zero runs no turns: nothing is read, no tool call fails, and the
    /// total-outage guard — which fires on a failure having *happened* — has nothing to fire on.
    /// Control then reaches the synthesis turn, which asks a model to cite file:line locations
    /// against a history holding only the prompt. That is the 2026-09-26 fabrication reached
    /// through a caller-controlled field with no tool outage anywhere, so zero is clamped up to
    /// one rather than passed through.
    pub(crate) fn budget_within(&self, definitions_budget: u32) -> TurnBudget {
        match self.max_turns {
            None => TurnBudget {
                turns: definitions_budget,
                clamped_to: None,
            },
            Some(asked) if asked > SUBAGENT_MAX_TURNS_CEILING => TurnBudget {
                turns: SUBAGENT_MAX_TURNS_CEILING,
                clamped_to: Some(SUBAGENT_MAX_TURNS_CEILING),
            },
            Some(asked) if asked < SUBAGENT_MIN_TURNS => TurnBudget {
                turns: SUBAGENT_MIN_TURNS,
                clamped_to: Some(SUBAGENT_MIN_TURNS),
            },
            Some(asked) => TurnBudget {
                turns: asked,
                clamped_to: None,
            },
        }
    }
}

/// How many turns one call may spend, and whether the ceiling cut the caller's request down.
pub(crate) struct TurnBudget {
    pub(crate) turns: u32,
    /// The budget actually applied, when it is smaller than the one asked for; `None` when the
    /// caller got what it asked for, so the field means something whenever it is set.
    pub(crate) clamped_to: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const THE_DEFINITIONS_BUDGET: u32 = 10;

    #[test]
    fn a_request_with_no_budget_of_its_own_runs_on_the_definitions() {
        // Given / When
        let budget = TurnRequest::prompting("find it").budget_within(THE_DEFINITIONS_BUDGET);

        // Then
        assert_eq!(budget.turns, THE_DEFINITIONS_BUDGET);
        assert_eq!(budget.clamped_to, None);
    }

    #[test]
    fn a_callers_budget_replaces_the_definitions() {
        // Given / When
        let budget = TurnRequest::prompting("find it")
            .within_turns(7)
            .budget_within(THE_DEFINITIONS_BUDGET);

        // Then
        assert_eq!(budget.turns, 7);
        assert_eq!(budget.clamped_to, None);
    }

    #[test]
    fn a_budget_above_the_ceiling_is_cut_to_it_and_says_so() {
        // Given / When
        let budget = TurnRequest::prompting("find it")
            .within_turns(SUBAGENT_MAX_TURNS_CEILING * 10)
            .budget_within(THE_DEFINITIONS_BUDGET);

        // Then
        assert_eq!(budget.turns, SUBAGENT_MAX_TURNS_CEILING);
        assert_eq!(budget.clamped_to, Some(SUBAGENT_MAX_TURNS_CEILING));
    }

    #[test]
    fn a_definitions_budget_above_the_ceiling_is_the_operators_choice_and_stands() {
        // Given a definition an operator wrote a large budget into, and a caller that asks for
        // nothing
        // When
        let budget = TurnRequest::resuming().budget_within(SUBAGENT_MAX_TURNS_CEILING * 2);

        // Then
        assert_eq!(budget.turns, SUBAGENT_MAX_TURNS_CEILING * 2);
        assert_eq!(budget.clamped_to, None);
    }

    #[test]
    fn a_rewind_takes_the_worktree_back_by_default() {
        assert!(TurnRequest::resuming()
            .from_message(MessageId::from("m3".to_string()))
            .resets_worktree());
    }

    #[test]
    fn keeping_the_worktree_opts_a_rewind_out_of_the_reset() {
        assert!(!TurnRequest::resuming()
            .from_message(MessageId::from("m3".to_string()))
            .keeping_worktree()
            .resets_worktree());
    }

    #[test]
    fn a_turn_takes_in_the_callers_files_by_default() {
        assert!(TurnRequest::prompting("go on").syncs_worktree());
    }

    #[test]
    fn without_sync_runs_the_turn_on_the_worktree_as_it_stands() {
        assert!(!TurnRequest::resuming().without_sync().syncs_worktree());
    }
}
