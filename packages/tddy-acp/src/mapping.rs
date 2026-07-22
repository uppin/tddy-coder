//! Pure mapping between tddy's internal workflow types and ACP.
//!
//! Two directions, both dependency-free of any transport:
//! - **Agent side** (`tddy-coder --acp`): a running workflow's `PresenterEvent` / `ProgressEvent`
//!   become outbound ACP `SessionUpdate`s; a blocked `ClarificationQuestion` becomes ACP permission
//!   options; a terminal `ExecutionStatus` becomes a `StopReason`.
//! - **Host/bridge side**: an inbound ACP `SessionUpdate` from the agent becomes an internal
//!   `PresenterEvent`, which the existing `TddyRemoteService` already turns into a web
//!   `ServerMessage` — so the browser's `TddyRemote` stream is unchanged.

use agent_client_protocol as acp;
use tddy_core::{
    ActivityEntry, ActivityKind, ClarificationQuestion, ExecutionStatus, PresenterEvent,
    ProgressEvent,
};

/// Agent side: a workflow presenter event → the outbound ACP session update that represents it, or
/// `None` for events with no ACP surface.
pub fn presenter_event_to_session_update(event: &PresenterEvent) -> Option<acp::SessionUpdate> {
    match event {
        PresenterEvent::AgentOutput(text) => Some(acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new(text.clone().into()),
        )),
        _ => None,
    }
}

/// Agent side: a streaming progress event → the outbound ACP session update.
pub fn progress_event_to_session_update(event: &ProgressEvent) -> Option<acp::SessionUpdate> {
    match event {
        ProgressEvent::ToolUse { name, .. } => Some(acp::SessionUpdate::ToolCall(
            acp::ToolCall::new(acp::ToolCallId::new(name.clone()), name.clone()),
        )),
        _ => None,
    }
}

/// Agent side: the options a blocked clarification offers, as ACP permission options — one per
/// choice, plus an extra "Other" affordance when the question allows a custom answer.
pub fn clarification_permission_options(
    question: &ClarificationQuestion,
) -> Vec<acp::PermissionOption> {
    let mut options: Vec<acp::PermissionOption> = question
        .options
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(format!("option-{i}")),
                choice.label.clone(),
                acp::PermissionOptionKind::AllowOnce,
            )
        })
        .collect();

    if question.allow_other {
        options.push(acp::PermissionOption::new(
            acp::PermissionOptionId::new("other"),
            "Other…",
            acp::PermissionOptionKind::AllowOnce,
        ));
    }

    options
}

/// Agent side: a terminal workflow status → the ACP stop reason, or `None` while the turn continues
/// (e.g. the workflow is waiting on a permission answer).
pub fn execution_status_to_stop_reason(status: &ExecutionStatus) -> Option<acp::StopReason> {
    match status {
        ExecutionStatus::Completed => Some(acp::StopReason::EndTurn),
        // The turn is not over: the workflow is blocked awaiting input, paused, needs elicitation,
        // or errored out. None of these are a normal end-of-turn, so no stop reason is emitted.
        ExecutionStatus::WaitingForInput { .. }
        | ExecutionStatus::Paused { .. }
        | ExecutionStatus::ElicitationNeeded { .. }
        | ExecutionStatus::Error(_) => None,
    }
}

/// Host/bridge side: an inbound ACP session update from the agent → the internal presenter event
/// the web stream is built from, or `None` for updates with no web surface.
pub fn session_update_to_presenter_event(update: &acp::SessionUpdate) -> Option<PresenterEvent> {
    match update {
        acp::SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
            acp::ContentBlock::Text(t) => Some(PresenterEvent::AgentOutput(t.text.clone())),
            _ => None,
        },
        acp::SessionUpdate::ToolCall(tc) => Some(PresenterEvent::ActivityLogged(ActivityEntry {
            text: tc.title.clone(),
            kind: ActivityKind::ToolUse,
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::{ActivityKind, QuestionOption};

    fn a_select_question() -> ClarificationQuestion {
        ClarificationQuestion {
            header: "Backend".into(),
            question: "Which coding backend should drive this session?".into(),
            options: vec![
                QuestionOption {
                    label: "Claude".into(),
                    description: String::new(),
                },
                QuestionOption {
                    label: "Cursor".into(),
                    description: String::new(),
                },
            ],
            multi_select: false,
            allow_other: false,
        }
    }

    // --- Agent side: workflow → ACP -----------------------------------------

    #[test]
    fn maps_agent_output_to_an_agent_message_chunk_preserving_the_text() {
        // Given
        let event = PresenterEvent::AgentOutput("Refactoring the parser".into());

        // When
        let update = presenter_event_to_session_update(&event);

        // Then
        match update {
            Some(acp::SessionUpdate::AgentMessageChunk(chunk)) => match chunk.content {
                acp::ContentBlock::Text(t) => assert_eq!(t.text, "Refactoring the parser"),
                other => panic!("expected text content, got {other:?}"),
            },
            other => panic!("expected AgentMessageChunk, got {other:?}"),
        }
    }

    #[test]
    fn maps_a_tool_use_progress_event_to_a_tool_call_titled_with_the_tool_name() {
        // Given
        let event = ProgressEvent::ToolUse {
            name: "Read".into(),
            detail: None,
            input_json: None,
            call_id: None,
        };

        // When
        let update = progress_event_to_session_update(&event);

        // Then
        match update {
            Some(acp::SessionUpdate::ToolCall(tc)) => assert_eq!(tc.title, "Read"),
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn offers_one_permission_option_per_select_choice() {
        // Given
        let question = a_select_question();

        // When
        let options = clarification_permission_options(&question);

        // Then
        assert_eq!(options.len(), 2, "one option per choice, no Other");
        assert_eq!(options[0].name, "Claude");
        assert_eq!(options[1].name, "Cursor");
    }

    #[test]
    fn adds_an_other_permission_affordance_when_the_question_allows_a_custom_answer() {
        // Given — a multi-select question that permits a custom answer
        let question = ClarificationQuestion {
            multi_select: true,
            allow_other: true,
            ..a_select_question()
        };

        // When
        let options = clarification_permission_options(&question);

        // Then — the two choices plus one "Other" affordance
        assert_eq!(options.len(), 3);
        assert!(
            options[2].name.to_lowercase().contains("other"),
            "the extra affordance should be labelled Other, was {:?}",
            options[2].name
        );
    }

    #[test]
    fn maps_a_completed_status_to_end_turn() {
        // Given / When / Then
        assert_eq!(
            execution_status_to_stop_reason(&ExecutionStatus::Completed),
            Some(acp::StopReason::EndTurn),
        );
    }

    #[test]
    fn does_not_produce_a_stop_reason_while_waiting_for_input() {
        // Given — the turn is not over; the workflow is blocked on an answer
        let status = ExecutionStatus::WaitingForInput { message: None };

        // When / Then
        assert_eq!(execution_status_to_stop_reason(&status), None);
    }

    // --- Host/bridge side: ACP → internal -----------------------------------

    #[test]
    fn maps_an_inbound_agent_message_chunk_back_to_agent_output() {
        // Given
        let update = acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
            "Streaming a token".to_string().into(),
        ));

        // When
        let event = session_update_to_presenter_event(&update);

        // Then
        match event {
            Some(PresenterEvent::AgentOutput(text)) => assert_eq!(text, "Streaming a token"),
            other => panic!("expected AgentOutput, got {other:?}"),
        }
    }

    #[test]
    fn maps_an_inbound_tool_call_back_to_a_tool_use_activity() {
        // Given
        let update =
            acp::SessionUpdate::ToolCall(acp::ToolCall::new(acp::ToolCallId::new("tc-1"), "Bash"));

        // When
        let event = session_update_to_presenter_event(&update);

        // Then
        match event {
            Some(PresenterEvent::ActivityLogged(entry)) => {
                assert_eq!(entry.kind, ActivityKind::ToolUse);
                assert_eq!(entry.text, "Bash");
            }
            other => panic!("expected ActivityLogged(ToolUse), got {other:?}"),
        }
    }
}
