use crate::parked_permissions::{nothing_pending, nothing_pending_to_defer};
use tddy_agent_backend::AcpStopReason;
use tddy_discovery::subagent::{StopReason, SubagentError, TurnRequest};

/// The new prompt a turn that starts a prompt carries, or why it cannot start one.
pub(super) fn new_prompt(request: &TurnRequest) -> Result<String, SubagentError> {
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

pub(super) fn refused_while_pending(request_id: &str) -> SubagentError {
    SubagentError::from(format!(
        "permission request '{request_id}' is pending: answer it before the coding agent takes another turn"
    ))
}

/// Why an answer that was delivered is not continued by this call: the turn holding the request
/// is still running, and it carries on with the answer. The wording is the shared one.
pub(super) fn answer_delivered_while_held(request_id: &str) -> SubagentError {
    SubagentError::from(tddy_discovery::subagent::answer_delivered_while_held(
        request_id,
    ))
}
/// The turn's stop reason as a subagent outcome spells it; a refusal is an error, not an outcome.
pub(super) fn stop_reason_of(reason: AcpStopReason) -> Result<StopReason, SubagentError> {
    match reason {
        AcpStopReason::EndTurn => Ok(StopReason::EndTurn),
        AcpStopReason::MaxTokens => Ok(StopReason::MaxTokens),
        AcpStopReason::MaxTurnRequests => Ok(StopReason::MaxTurnRequests),
        AcpStopReason::Cancelled => Ok(StopReason::Cancelled),
        AcpStopReason::Refusal => Err(SubagentError::from("the coding agent refused the turn")),
    }
}

/// Refuse the request options that act on an OpenAI-loop transcript, naming the first one set.
pub(super) fn refuse_loop_only(request: &TurnRequest) -> Result<(), SubagentError> {
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
