//! Conversion between tddy-core types and the `AcpService` protobuf mirror of ACP.
//!
//! This is the prost-typed counterpart of `tddy_acp::mapping` (which maps the same internal types
//! to the JSON-RPC `agent_client_protocol` structs). Keeping the two in lockstep is what makes the
//! JSON-RPC edge a mechanical translation.
//!
//! Two halves:
//! - **Stateless** (`presenter_event_to_session_update` + the `*_update` builders): a `PresenterEvent`
//!   with a 1:1 ACP surface → one `SessionUpdate`.
//! - **Stateful** lifecycle (tool-call ids, synthesized `Plan`) lives in
//!   [`crate::service_acp`]'s `OutboundState`, which owns the per-stream counters and calls the
//!   builders here. `PresenterEvent` is all the view-adapter sees, so mappings are bounded by it.
//!
//! **tddy rendering conventions** (both ends are ours; they let the web reconstruct the pr-stack
//! chat's bubble kinds over ACP): the goal rides `agent_thought_chunk`, non-tool activity/system log
//! lines ride a one-shot `tool_call`, and a multi-select clarification is flagged by a
//! `clarification:multi` tool-call id with answers encoded in the reply `option_id`
//! (`option-{i}` / `other[:text]` / `multi:{i,j}[;other=text]`). ACP variants with no internal source
//! (`available_commands_update`, `current_mode_update`) and the client-side `fs/*`/`terminal/*`
//! methods remain name-mirrored stubs (see `acp.proto`).
//! - **Inbound** (client -> agent): an `AcpClientMessage` `PromptRequest` / `RequestPermissionResponse`
//!   becomes a [`UserIntent`] fed into the Presenter — like `client_message_to_intent` for `TddyRemote`.

use tddy_core::{ActivityKind, ClarificationQuestion, PresenterEvent, UserIntent};

use crate::proto::acp::{
    acp_agent_message, content_block, request_permission_outcome, session_update, AcpAgentMessage,
    ContentBlock, ContentChunk, PermissionOption, PermissionOptionId, Plan, PlanEntry,
    PlanEntryStatus, PromptResponse, RequestPermissionRequest, RequestPermissionResponse,
    SessionId, SessionNotification, SessionUpdate, StopReason, TextContent, ToolCall, ToolCallId,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
};

/// Build a text [`ContentBlock`].
fn text_block(text: String) -> ContentBlock {
    ContentBlock {
        block: Some(content_block::Block::Text(TextContent { text })),
    }
}

/// A text `agent_message_chunk` session update.
pub fn agent_message_chunk(text: String) -> SessionUpdate {
    SessionUpdate {
        update: Some(session_update::Update::AgentMessageChunk(ContentChunk {
            content: Some(text_block(text)),
        })),
    }
}

/// A text `user_message_chunk` session update (the echoed operator prompt).
pub fn user_message_chunk(text: String) -> SessionUpdate {
    SessionUpdate {
        update: Some(session_update::Update::UserMessageChunk(ContentChunk {
            content: Some(text_block(text)),
        })),
    }
}

/// A text `agent_thought_chunk` session update. **tddy convention:** this channel (which has no
/// other producer — model "thinking" is discarded upstream) carries the workflow **goal/section
/// header** so the web can render it as a distinct "goal" bubble. External ACP clients render it as
/// agent thinking, which is acceptable.
pub fn agent_thought_chunk(text: String) -> SessionUpdate {
    SessionUpdate {
        update: Some(session_update::Update::AgentThoughtChunk(ContentChunk {
            content: Some(text_block(text)),
        })),
    }
}

/// A one-shot `tool_call` (status `Completed`, fixed id) used to carry a non-tool **activity/system
/// log line** as a distinct "activity" bubble on the web (ACP has no system-message concept).
pub fn activity_tool_call(text: String) -> SessionUpdate {
    SessionUpdate {
        update: Some(session_update::Update::ToolCall(ToolCall {
            tool_call_id: Some(ToolCallId {
                value: "activity".to_string(),
            }),
            title: text,
            status: ToolCallStatus::Completed as i32,
            ..Default::default()
        })),
    }
}

/// Stable tool-call id string for a numeric handle.
fn tool_call_id(id: u64) -> Option<ToolCallId> {
    Some(ToolCallId {
        value: format!("tool-{id}"),
    })
}

/// A `tool_call` opening in the `InProgress` state (stable id + title).
pub fn tool_call_started(id: u64, title: &str) -> SessionUpdate {
    SessionUpdate {
        update: Some(session_update::Update::ToolCall(ToolCall {
            tool_call_id: tool_call_id(id),
            title: title.to_string(),
            status: ToolCallStatus::InProgress as i32,
            ..Default::default()
        })),
    }
}

/// A `tool_call_update` marking a tool call `Completed`.
pub fn tool_call_completed(id: u64) -> SessionUpdate {
    SessionUpdate {
        update: Some(session_update::Update::ToolCallUpdate(ToolCallUpdate {
            tool_call_id: tool_call_id(id),
            fields: Some(ToolCallUpdateFields {
                status: Some(ToolCallStatus::Completed as i32),
                ..Default::default()
            }),
        })),
    }
}

/// A `tool_call_update` keeping a tool call `InProgress` (a progress ping).
pub fn tool_call_progress(id: u64) -> SessionUpdate {
    SessionUpdate {
        update: Some(session_update::Update::ToolCallUpdate(ToolCallUpdate {
            tool_call_id: tool_call_id(id),
            fields: Some(ToolCallUpdateFields {
                status: Some(ToolCallStatus::InProgress as i32),
                ..Default::default()
            }),
        })),
    }
}

/// A `plan` entry with the given content and status.
pub fn plan_entry(content: String, status: PlanEntryStatus) -> PlanEntry {
    PlanEntry {
        content,
        priority: 0, // PLAN_ENTRY_PRIORITY_UNSPECIFIED
        status: status as i32,
    }
}

/// A `plan` session update carrying the current entry list.
pub fn plan_update(entries: &[PlanEntry]) -> SessionUpdate {
    SessionUpdate {
        update: Some(session_update::Update::Plan(Plan {
            entries: entries.to_vec(),
        })),
    }
}

/// Stateless outbound: a Presenter event → the single `SessionUpdate` that represents it, or `None`
/// for events with no 1:1 ACP surface **or** those that need stateful handling (tool-call lifecycle /
/// Plan synthesis — see `OutboundState`). Mirrors the informational side of `tddy_acp::mapping`.
pub fn presenter_event_to_session_update(event: &PresenterEvent) -> Option<SessionUpdate> {
    match event {
        PresenterEvent::AgentOutput(text) => Some(agent_message_chunk(text.clone())),
        // Goal → its own channel (agent_thought_chunk) so the web renders a distinct "goal" bubble.
        PresenterEvent::GoalStarted(goal) => Some(agent_thought_chunk(goal.clone())),
        PresenterEvent::StateChanged { from, to } => {
            Some(activity_tool_call(format!("{from} → {to}")))
        }
        PresenterEvent::ActivityLogged(entry) => match entry.kind {
            // The one real source for user_message_chunk: the operator's echoed prompt.
            ActivityKind::UserPrompt => Some(user_message_chunk(entry.text.clone())),
            ActivityKind::AgentOutput => Some(agent_message_chunk(entry.text.clone())),
            // Non-tool system log lines → an "activity" bubble via a one-shot tool_call.
            ActivityKind::Info | ActivityKind::StateChange => {
                Some(activity_tool_call(entry.text.clone()))
            }
            // Real tool calls / tasks are handled statefully in `OutboundState`.
            ActivityKind::ToolUse | ActivityKind::TaskStarted | ActivityKind::TaskProgress => None,
        },
        _ => None,
    }
}

/// Outbound: wrap a `SessionUpdate` as an `AcpAgentMessage` notification (unsolicited: no `id`).
pub fn session_update_message(session_id: &str, update: SessionUpdate) -> AcpAgentMessage {
    AcpAgentMessage {
        id: 0,
        msg: Some(acp_agent_message::Msg::SessionUpdate(SessionNotification {
            session_id: Some(SessionId {
                value: session_id.to_string(),
            }),
            update: Some(update),
            // Live stream leaves this unset; the persisted replay stamps the real event time.
            timestamp_unix_ms: 0,
        })),
    }
}

/// Outbound: the options a blocked clarification offers, as ACP permission options — one per choice
/// plus an "Other" affordance when a custom answer is allowed. Prost mirror of
/// `tddy_acp::mapping::clarification_permission_options`; option ids (`option-{i}`, `other`) match so
/// the inbound decode below and the JSON-RPC edge agree.
pub fn clarification_permission_options(question: &ClarificationQuestion) -> Vec<PermissionOption> {
    let mut options: Vec<PermissionOption> = question
        .options
        .iter()
        .enumerate()
        .map(|(i, choice)| PermissionOption {
            option_id: Some(PermissionOptionId {
                value: format!("option-{i}"),
            }),
            name: choice.label.clone(),
            kind: crate::proto::acp::PermissionOptionKind::AllowOnce as i32,
        })
        .collect();

    if question.allow_other {
        options.push(PermissionOption {
            option_id: Some(PermissionOptionId {
                value: "other".to_string(),
            }),
            name: "Other…".to_string(),
            kind: crate::proto::acp::PermissionOptionKind::AllowOnce as i32,
        });
    }

    options
}

/// Outbound: a blocked clarification → the agent's `request_permission` request. `id` correlates the
/// eventual client reply. The `tool_call` field carries the question header as a titled update (ACP
/// requires a tool-call handle on permission requests).
pub fn clarification_request_permission(
    id: u64,
    session_id: &str,
    question: &ClarificationQuestion,
) -> AcpAgentMessage {
    AcpAgentMessage {
        id,
        msg: Some(acp_agent_message::Msg::RequestPermission(
            RequestPermissionRequest {
                session_id: Some(SessionId {
                    value: session_id.to_string(),
                }),
                tool_call: Some(ToolCallUpdate {
                    // tddy convention: the `:multi` suffix tells the web to render a multi-select
                    // clarification (ACP request_permission is otherwise single-select).
                    tool_call_id: Some(ToolCallId {
                        value: if question.multi_select {
                            "clarification:multi".to_string()
                        } else {
                            "clarification".to_string()
                        },
                    }),
                    // The question text + header have no ACP field of their own, so they ride the
                    // tool-call fields: `title` = question, `raw_input` = header.
                    fields: Some(ToolCallUpdateFields {
                        title: Some(question.question.clone()),
                        raw_input: Some(question.header.clone()),
                        ..Default::default()
                    }),
                }),
                options: clarification_permission_options(question),
            },
        )),
    }
}

/// Outbound: a terminal prompt response carrying a stop reason.
pub fn prompt_response(id: u64, stop_reason: StopReason) -> AcpAgentMessage {
    AcpAgentMessage {
        id,
        msg: Some(acp_agent_message::Msg::Prompt(PromptResponse {
            stop_reason: stop_reason as i32,
        })),
    }
}

/// Inbound: the text of a prompt's content blocks joined into one string (mirrors
/// `acp_agent::prompt_text`, only `Text` blocks contribute).
pub fn prompt_text(blocks: &[ContentBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if let Some(content_block::Block::Text(t)) = &block.block {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&t.text);
        }
    }
    out
}

/// Inbound: a client's answer to a `request_permission` → the `UserIntent` that advances the blocked
/// workflow. The `option_id` string encodes the answer (tddy conventions, produced by `useAcpSession`):
/// - `option-{i}`            → `AnswerSelect(i)`
/// - `other` / `other:{text}` → `AnswerOther(text)`  (empty when no text)
/// - `multi:{i,j,…}[;other={text}]` → `AnswerMultiSelect([i,j,…], other?)`
/// - `Cancelled`             → `Quit`
pub fn permission_response_to_intent(resp: &RequestPermissionResponse) -> Option<UserIntent> {
    let outcome = resp.outcome.as_ref()?;
    let selected = match outcome.outcome.as_ref()? {
        request_permission_outcome::Outcome::Selected(s) => s,
        request_permission_outcome::Outcome::Cancelled(_) => return Some(UserIntent::Quit),
    };
    let option_id = selected.option_id.as_ref()?.value.as_str();

    if let Some(rest) = option_id.strip_prefix("multi:") {
        // "multi:0,2" or "multi:0,2;other=custom text"
        let (indices_csv, other) = match rest.split_once(";other=") {
            Some((csv, text)) => (csv, Some(text.to_string())),
            None => (rest, None),
        };
        let indices: Vec<usize> = indices_csv
            .split(',')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse::<usize>().ok())
            .collect();
        return Some(UserIntent::AnswerMultiSelect(indices, other));
    }
    if option_id == "other" {
        return Some(UserIntent::AnswerOther(String::new()));
    }
    if let Some(text) = option_id.strip_prefix("other:") {
        return Some(UserIntent::AnswerOther(text.to_string()));
    }
    option_id
        .strip_prefix("option-")
        .and_then(|n| n.parse::<usize>().ok())
        .map(UserIntent::AnswerSelect)
}

/// Inbound: a prompt's text → the `UserIntent` fed to the Presenter. A fresh session's first prompt
/// is the feature input that starts the workflow; later prompts queue onto the running session —
/// mirroring the web `useAgentChat` first-message fallback.
pub fn prompt_to_intent(text: String, has_started: bool) -> UserIntent {
    if has_started {
        UserIntent::QueuePrompt(text)
    } else {
        UserIntent::SubmitFeatureInput(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::{ActivityEntry, QuestionOption};

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

    fn chunk_text(update: Option<SessionUpdate>) -> (String, String) {
        // Returns (variant_label, text) for an AgentMessageChunk / UserMessageChunk.
        match update.and_then(|u| u.update) {
            Some(session_update::Update::AgentMessageChunk(c)) => ("agent".into(), block_text(c)),
            Some(session_update::Update::UserMessageChunk(c)) => ("user".into(), block_text(c)),
            other => panic!("expected a message chunk, got {other:?}"),
        }
    }
    fn block_text(chunk: ContentChunk) -> String {
        match chunk.content.and_then(|c| c.block) {
            Some(content_block::Block::Text(t)) => t.text,
            other => panic!("expected text content, got {other:?}"),
        }
    }

    // --- Stateless outbound: PresenterEvent -> SessionUpdate ------------------------------------

    #[test]
    fn maps_agent_output_to_an_agent_message_chunk_preserving_the_text() {
        let update = presenter_event_to_session_update(&PresenterEvent::AgentOutput(
            "Refactoring the parser".into(),
        ));
        assert_eq!(
            chunk_text(update),
            ("agent".into(), "Refactoring the parser".into())
        );
    }

    #[test]
    fn maps_a_user_prompt_activity_to_a_user_message_chunk() {
        let update =
            presenter_event_to_session_update(&PresenterEvent::ActivityLogged(ActivityEntry {
                text: "add a feature".into(),
                kind: ActivityKind::UserPrompt,
            }));
        assert_eq!(chunk_text(update), ("user".into(), "add a feature".into()));
    }

    #[test]
    fn maps_goal_started_to_a_thought_chunk_carrying_the_goal_name() {
        match presenter_event_to_session_update(&PresenterEvent::GoalStarted(
            "Implement login".into(),
        ))
        .and_then(|u| u.update)
        {
            Some(session_update::Update::AgentThoughtChunk(c)) => {
                assert_eq!(block_text(c), "Implement login")
            }
            other => panic!("expected AgentThoughtChunk (goal channel), got {other:?}"),
        }
    }

    #[test]
    fn maps_state_changed_to_an_activity_tool_call() {
        match presenter_event_to_session_update(&PresenterEvent::StateChanged {
            from: "Red".into(),
            to: "Green".into(),
        })
        .and_then(|u| u.update)
        {
            Some(session_update::Update::ToolCall(tc)) => assert_eq!(tc.title, "Red → Green"),
            other => panic!("expected an activity ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn leaves_tool_and_task_activities_to_the_stateful_path() {
        for kind in [
            ActivityKind::ToolUse,
            ActivityKind::TaskStarted,
            ActivityKind::TaskProgress,
        ] {
            let label = format!("{kind:?}");
            let update =
                presenter_event_to_session_update(&PresenterEvent::ActivityLogged(ActivityEntry {
                    text: "x".into(),
                    kind,
                }));
            assert!(update.is_none(), "{label} must be handled statefully");
        }
    }

    // --- Tool-call + plan builders -------------------------------------------------------------

    #[test]
    fn tool_call_lifecycle_builders_carry_stable_ids_and_statuses() {
        match tool_call_started(3, "Read").update {
            Some(session_update::Update::ToolCall(tc)) => {
                assert_eq!(tc.tool_call_id.unwrap().value, "tool-3");
                assert_eq!(tc.title, "Read");
                assert_eq!(tc.status, ToolCallStatus::InProgress as i32);
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
        match tool_call_completed(3).update {
            Some(session_update::Update::ToolCallUpdate(u)) => {
                assert_eq!(u.tool_call_id.unwrap().value, "tool-3");
                assert_eq!(
                    u.fields.unwrap().status,
                    Some(ToolCallStatus::Completed as i32)
                );
            }
            other => panic!("expected ToolCallUpdate, got {other:?}"),
        }
    }

    #[test]
    fn plan_update_carries_entries_with_statuses() {
        let entries = vec![
            plan_entry("first".into(), PlanEntryStatus::Completed),
            plan_entry("second".into(), PlanEntryStatus::InProgress),
        ];
        match plan_update(&entries).update {
            Some(session_update::Update::Plan(p)) => {
                assert_eq!(p.entries.len(), 2);
                assert_eq!(p.entries[0].content, "first");
                assert_eq!(p.entries[0].status, PlanEntryStatus::Completed as i32);
                assert_eq!(p.entries[1].status, PlanEntryStatus::InProgress as i32);
            }
            other => panic!("expected Plan, got {other:?}"),
        }
    }

    // --- Permission options + inbound decode ---------------------------------------------------

    #[test]
    fn offers_one_permission_option_per_select_choice() {
        let options = clarification_permission_options(&a_select_question());
        assert_eq!(options.len(), 2, "one option per choice, no Other");
        assert_eq!(options[0].name, "Claude");
        assert_eq!(options[0].option_id.as_ref().unwrap().value, "option-0");
        assert_eq!(options[1].name, "Cursor");
    }

    #[test]
    fn adds_an_other_permission_affordance_when_the_question_allows_a_custom_answer() {
        let question = ClarificationQuestion {
            multi_select: true,
            allow_other: true,
            ..a_select_question()
        };
        let options = clarification_permission_options(&question);
        assert_eq!(options.len(), 3);
        assert_eq!(options[2].option_id.as_ref().unwrap().value, "other");
    }

    #[test]
    fn joins_only_text_prompt_blocks() {
        let blocks = vec![
            ContentBlock {
                block: Some(content_block::Block::Text(TextContent {
                    text: "line one".into(),
                })),
            },
            ContentBlock {
                block: Some(content_block::Block::Text(TextContent {
                    text: "line two".into(),
                })),
            },
        ];
        assert_eq!(prompt_text(&blocks), "line one\nline two");
    }

    #[test]
    fn decodes_a_selected_option_back_to_the_answer_index() {
        let resp = RequestPermissionResponse {
            outcome: Some(crate::proto::acp::RequestPermissionOutcome {
                outcome: Some(request_permission_outcome::Outcome::Selected(
                    crate::proto::acp::SelectedPermissionOutcome {
                        option_id: Some(PermissionOptionId {
                            value: "option-1".into(),
                        }),
                    },
                )),
            }),
        };
        assert_eq!(
            permission_response_to_intent(&resp),
            Some(UserIntent::AnswerSelect(1))
        );
    }

    #[test]
    fn decodes_a_cancelled_permission_to_quit() {
        let resp = RequestPermissionResponse {
            outcome: Some(crate::proto::acp::RequestPermissionOutcome {
                outcome: Some(request_permission_outcome::Outcome::Cancelled(
                    crate::proto::acp::Cancelled {},
                )),
            }),
        };
        assert_eq!(permission_response_to_intent(&resp), Some(UserIntent::Quit));
    }

    /// Build a `Selected` permission response with the given encoded option id.
    fn selected(option_id: &str) -> RequestPermissionResponse {
        RequestPermissionResponse {
            outcome: Some(crate::proto::acp::RequestPermissionOutcome {
                outcome: Some(request_permission_outcome::Outcome::Selected(
                    crate::proto::acp::SelectedPermissionOutcome {
                        option_id: Some(PermissionOptionId {
                            value: option_id.into(),
                        }),
                    },
                )),
            }),
        }
    }

    #[test]
    fn decodes_the_other_affordance_with_and_without_custom_text() {
        assert_eq!(
            permission_response_to_intent(&selected("other")),
            Some(UserIntent::AnswerOther(String::new()))
        );
        assert_eq!(
            permission_response_to_intent(&selected("other:my custom answer")),
            Some(UserIntent::AnswerOther("my custom answer".into()))
        );
    }

    #[test]
    fn decodes_a_multi_select_answer_with_optional_custom_text() {
        assert_eq!(
            permission_response_to_intent(&selected("multi:0,2")),
            Some(UserIntent::AnswerMultiSelect(vec![0, 2], None))
        );
        assert_eq!(
            permission_response_to_intent(&selected("multi:1;other=extra")),
            Some(UserIntent::AnswerMultiSelect(vec![1], Some("extra".into())))
        );
    }

    #[test]
    fn multi_select_clarification_marks_the_permission_request_with_the_multi_suffix() {
        let question = ClarificationQuestion {
            multi_select: true,
            ..a_select_question()
        };
        let msg = clarification_request_permission(7, "s", &question);
        match msg.msg {
            Some(acp_agent_message::Msg::RequestPermission(req)) => {
                assert_eq!(
                    req.tool_call.unwrap().tool_call_id.unwrap().value,
                    "clarification:multi"
                );
            }
            other => panic!("expected RequestPermission, got {other:?}"),
        }
    }

    #[test]
    fn first_prompt_starts_the_workflow_then_later_prompts_queue() {
        assert_eq!(
            prompt_to_intent("build it".into(), false),
            UserIntent::SubmitFeatureInput("build it".into())
        );
        assert_eq!(
            prompt_to_intent("and now this".into(), true),
            UserIntent::QueuePrompt("and now this".into())
        );
    }
}
