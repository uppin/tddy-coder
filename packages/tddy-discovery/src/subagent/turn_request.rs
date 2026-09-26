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

    /// Spend at most `max_turns` model turns on this call, in place of the agent definition's own
    /// budget — for this call only, and never above [`SUBAGENT_MAX_TURNS_CEILING`].
    pub fn within_turns(mut self, max_turns: u32) -> Self {
        self.max_turns = Some(max_turns);
        self
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
    /// the operator's alone. Only the caller's figure meets the ceiling: the definition's budget is
    /// the operator's own configuration, and clamping it here would quietly override a deliberate
    /// setting nobody in this call asked to change.
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
}
