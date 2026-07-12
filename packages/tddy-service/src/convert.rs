//! Conversion between tddy-core types and proto messages.

use tddy_core::{
    ActivityKind, AppMode, ModeChangedDetails, PresenterEvent, PresenterState, UserIntent,
    WorkflowEvent,
};

use crate::gen::{
    app_mode_proto, client_message, server_message, ActivityLogged, AgentOutput, AnswerMultiSelect,
    AnswerOther, AnswerSelect, AnswerText, AppModeDocumentReview, AppModeDone, AppModeFeatureInput,
    AppModeMarkdownViewer, AppModeMultiSelect, AppModeProto, AppModeRunning, AppModeSelect,
    AppModeTextInput, ApproveSessionDocument, BackendSelected, ClarificationQuestionProto,
    ClientMessage, ConversationRecord as ProtoConversationRecord, DeleteInboxItem, DismissViewer,
    EditInboxItem, GoalStarted, InboxChanged, IntentReceived, ModeChanged, QuestionOptionProto,
    QueuePrompt, Quit, RefineSessionDocument, RejectSessionDocument, Scroll, ServerMessage,
    StateChanged, SubmitFeatureInput, TokenUsageUpdated, ViewSessionDocument, WorkflowComplete,
};

/// Convert ClientMessage to UserIntent. Returns None if the message has no intent.
pub fn client_message_to_intent(msg: ClientMessage) -> Option<UserIntent> {
    use client_message::Intent;
    match msg.intent? {
        Intent::SubmitFeatureInput(SubmitFeatureInput { text }) => {
            Some(UserIntent::SubmitFeatureInput(text))
        }
        Intent::AnswerSelect(AnswerSelect { index }) => {
            Some(UserIntent::AnswerSelect(index as usize))
        }
        Intent::AnswerOther(AnswerOther { text }) => Some(UserIntent::AnswerOther(text)),
        Intent::AnswerMultiSelect(AnswerMultiSelect { indices, other }) => {
            let indices: Vec<usize> = indices.into_iter().map(|i| i as usize).collect();
            Some(UserIntent::AnswerMultiSelect(
                indices,
                if other.is_empty() { None } else { Some(other) },
            ))
        }
        Intent::AnswerText(AnswerText { text }) => Some(UserIntent::AnswerText(text)),
        Intent::QueuePrompt(QueuePrompt { text }) => Some(UserIntent::QueuePrompt(text)),
        Intent::EditInboxItem(EditInboxItem { index, text }) => Some(UserIntent::EditInboxItem {
            index: index as usize,
            text,
        }),
        Intent::DeleteInboxItem(DeleteInboxItem { index }) => {
            Some(UserIntent::DeleteInboxItem(index as usize))
        }
        Intent::Scroll(Scroll { delta }) => Some(UserIntent::Scroll(delta)),
        Intent::Quit(Quit {}) => Some(UserIntent::Quit),
        Intent::ApproveSessionDocument(ApproveSessionDocument {}) => {
            Some(UserIntent::ApproveSessionDocument)
        }
        Intent::ViewSessionDocument(ViewSessionDocument {}) => {
            Some(UserIntent::ViewSessionDocument)
        }
        Intent::RefineSessionDocument(RefineSessionDocument {}) => {
            Some(UserIntent::RefineSessionDocument)
        }
        Intent::DismissViewer(DismissViewer {}) => Some(UserIntent::DismissViewer),
        Intent::RejectSessionDocument(RejectSessionDocument {}) => {
            Some(UserIntent::RejectSessionDocument)
        }
        Intent::StartSession(_) | Intent::ConfirmWorktree(_) => None, // Daemon-only, not UserIntent
    }
}

/// Convert WorkflowEvent to ServerMessage. Used by daemon when running WorkflowEngine directly.
pub fn workflow_event_to_server_message(event: WorkflowEvent) -> Option<ServerMessage> {
    use server_message::Event;
    let event = match event {
        WorkflowEvent::GoalStarted(goal) => Event::GoalStarted(GoalStarted { goal }),
        WorkflowEvent::StateChange { from, to } => Event::StateChanged(StateChanged { from, to }),
        WorkflowEvent::AgentOutput(text) => Event::AgentOutput(AgentOutput { text }),
        WorkflowEvent::WorkflowComplete(result) => {
            let (ok, message) = match &result {
                Ok(payload) => (true, payload.summary.clone()),
                Err(e) => (false, e.clone()),
            };
            Event::WorkflowComplete(WorkflowComplete { ok, message })
        }
        WorkflowEvent::SessionDocumentApprovalNeeded { content } => {
            Event::ModeChanged(ModeChanged {
                mode: Some(AppModeProto {
                    variant: Some(app_mode_proto::Variant::DocumentReview(
                        AppModeDocumentReview { content },
                    )),
                }),
            })
        }
        WorkflowEvent::Progress(_)
        | WorkflowEvent::ClarificationNeeded { .. }
        | WorkflowEvent::WorktreeSwitched { .. }
        | WorkflowEvent::AwaitingFeatureInput => return None,
    };
    Some(ServerMessage { event: Some(event) })
}

/// Build ServerMessage for session-document approval elicitation (ModeChanged with DocumentReview).
pub fn session_document_approval_to_server_message(content: String) -> ServerMessage {
    use server_message::Event;
    ServerMessage {
        event: Some(Event::ModeChanged(ModeChanged {
            mode: Some(AppModeProto {
                variant: Some(app_mode_proto::Variant::DocumentReview(
                    AppModeDocumentReview { content },
                )),
            }),
        })),
    }
}

/// Convert PresenterEvent to ServerMessage.
pub fn event_to_server_message(event: PresenterEvent) -> ServerMessage {
    use server_message::Event;
    match event {
        PresenterEvent::ModeChanged(details) => ServerMessage {
            event: Some(Event::ModeChanged(ModeChanged {
                mode: Some(app_mode_to_proto(&details.mode)),
            })),
        },
        PresenterEvent::ActivityLogged(entry) => ServerMessage {
            event: Some(Event::ActivityLogged(ActivityLogged {
                text: entry.text,
                kind: activity_kind_to_str(&entry.kind),
            })),
        },
        PresenterEvent::GoalStarted(goal) => ServerMessage {
            event: Some(Event::GoalStarted(GoalStarted { goal })),
        },
        PresenterEvent::StateChanged { from, to } => ServerMessage {
            event: Some(Event::StateChanged(StateChanged { from, to })),
        },
        PresenterEvent::WorkflowComplete(result) => {
            let (ok, message) = match &result {
                Ok(payload) => (true, payload.summary.clone()),
                Err(e) => (false, e.clone()),
            };
            ServerMessage {
                event: Some(Event::WorkflowComplete(WorkflowComplete { ok, message })),
            }
        }
        PresenterEvent::AgentOutput(text) => ServerMessage {
            event: Some(Event::AgentOutput(AgentOutput { text })),
        },
        PresenterEvent::InboxChanged(items) => ServerMessage {
            event: Some(Event::InboxChanged(InboxChanged { items })),
        },
        PresenterEvent::IntentReceived(intent) => {
            if let Some(msg) = intent_to_client_message(&intent) {
                ServerMessage {
                    event: Some(Event::IntentReceived(IntentReceived { intent: Some(msg) })),
                }
            } else {
                ServerMessage { event: None }
            }
        }
        PresenterEvent::BackendSelected { agent, model } => ServerMessage {
            event: Some(Event::BackendSelected(BackendSelected { agent, model })),
        },
        PresenterEvent::ShouldQuit => {
            if let Some(msg) = intent_to_client_message(&UserIntent::Quit) {
                ServerMessage {
                    event: Some(Event::IntentReceived(IntentReceived { intent: Some(msg) })),
                }
            } else {
                ServerMessage { event: None }
            }
        }
        PresenterEvent::TokenUsageUpdated(records) => ServerMessage {
            event: Some(Event::TokenUsageUpdated(TokenUsageUpdated {
                conversations: records
                    .into_iter()
                    .map(|r| ProtoConversationRecord {
                        agent: r.agent,
                        id: r.id,
                        model: r.model,
                        input_tokens: r.input_tokens,
                        output_tokens: r.output_tokens,
                        total_tokens: r.total_tokens,
                        turns: r.turns,
                    })
                    .collect(),
            })),
        },
    }
}

/// Build the ordered `ServerMessage`s a freshly-connected remote View must receive to reconstruct
/// the Presenter's current state — the event-stream equivalent of the `state_snapshot` the TUI gets
/// from [`tddy_core::Presenter::connect_view`]. `TddyRemote::stream` replays these on stream open
/// (before forwarding live events), so a View that connects *after* agent output was produced still
/// sees the prior goal, mode, and agent output instead of an empty transcript.
pub fn snapshot_replay_messages(state: &PresenterState) -> Vec<ServerMessage> {
    let mut events: Vec<PresenterEvent> = Vec::new();
    if let Some(goal) = &state.current_goal {
        events.push(PresenterEvent::GoalStarted(goal.clone()));
    }
    events.push(PresenterEvent::ModeChanged(ModeChangedDetails {
        mode: state.mode.clone(),
        plan_refinement_pending: state.plan_refinement_pending,
        skills_project_root: state.skills_project_root.clone(),
    }));
    for entry in &state.activity_log {
        events.push(match entry.kind {
            ActivityKind::AgentOutput => PresenterEvent::AgentOutput(entry.text.clone()),
            _ => PresenterEvent::ActivityLogged(entry.clone()),
        });
    }
    events.into_iter().map(event_to_server_message).collect()
}

fn app_mode_to_proto(mode: &AppMode) -> AppModeProto {
    let variant = match mode {
        AppMode::FeatureInput => app_mode_proto::Variant::FeatureInput(AppModeFeatureInput {}),
        AppMode::Running => app_mode_proto::Variant::Running(AppModeRunning {}),
        AppMode::Select {
            question,
            question_index,
            total_questions,
            initial_selected,
        } => app_mode_proto::Variant::Select(AppModeSelect {
            question: Some(clarification_to_proto(question)),
            question_index: *question_index as u32,
            total_questions: *total_questions as u32,
            initial_selected: *initial_selected as u32,
        }),
        AppMode::MultiSelect {
            question,
            question_index,
            total_questions,
        } => app_mode_proto::Variant::MultiSelect(AppModeMultiSelect {
            question: Some(clarification_to_proto(question)),
            question_index: *question_index as u32,
            total_questions: *total_questions as u32,
        }),
        AppMode::TextInput { prompt } => app_mode_proto::Variant::TextInput(AppModeTextInput {
            prompt: prompt.clone(),
        }),
        AppMode::Done => app_mode_proto::Variant::Done(AppModeDone {}),
        AppMode::DocumentReview { content } => {
            app_mode_proto::Variant::DocumentReview(AppModeDocumentReview {
                content: content.clone(),
            })
        }
        AppMode::MarkdownViewer { content } => {
            app_mode_proto::Variant::MarkdownViewer(AppModeMarkdownViewer {
                content: content.clone(),
            })
        }
        // skeleton: ErrorRecovery has no proto representation yet
        AppMode::ErrorRecovery { .. } => app_mode_proto::Variant::Done(AppModeDone {}),
    };
    AppModeProto {
        variant: Some(variant),
    }
}

fn clarification_to_proto(q: &tddy_core::ClarificationQuestion) -> ClarificationQuestionProto {
    ClarificationQuestionProto {
        header: q.header.clone(),
        question: q.question.clone(),
        options: q
            .options
            .iter()
            .map(|o| QuestionOptionProto {
                label: o.label.clone(),
                description: o.description.clone(),
            })
            .collect(),
        multi_select: q.multi_select,
        allow_other: q.allow_other,
        recommended_other: String::new(),
    }
}

fn activity_kind_to_str(k: &ActivityKind) -> String {
    match k {
        ActivityKind::ToolUse => "ToolUse".to_string(),
        ActivityKind::TaskStarted => "TaskStarted".to_string(),
        ActivityKind::TaskProgress => "TaskProgress".to_string(),
        ActivityKind::StateChange => "StateChange".to_string(),
        ActivityKind::Info => "Info".to_string(),
        ActivityKind::UserPrompt => "UserPrompt".to_string(),
        ActivityKind::AgentOutput => "AgentOutput".to_string(),
    }
}

fn intent_to_client_message(intent: &UserIntent) -> Option<ClientMessage> {
    use client_message::Intent;
    let intent = match intent {
        UserIntent::SubmitFeatureInput(text) => {
            Intent::SubmitFeatureInput(SubmitFeatureInput { text: text.clone() })
        }
        UserIntent::AnswerSelect(idx) => Intent::AnswerSelect(AnswerSelect { index: *idx as u32 }),
        UserIntent::AnswerOther(text) => Intent::AnswerOther(AnswerOther { text: text.clone() }),
        UserIntent::AnswerMultiSelect(indices, other) => {
            Intent::AnswerMultiSelect(AnswerMultiSelect {
                indices: indices.iter().map(|&i| i as u32).collect(),
                other: other.clone().unwrap_or_default(),
            })
        }
        UserIntent::AnswerText(text) => Intent::AnswerText(AnswerText { text: text.clone() }),
        UserIntent::QueuePrompt(text) => Intent::QueuePrompt(QueuePrompt { text: text.clone() }),
        UserIntent::EditInboxItem { index, text } => Intent::EditInboxItem(EditInboxItem {
            index: *index as u32,
            text: text.clone(),
        }),
        UserIntent::DeleteInboxItem(index) => Intent::DeleteInboxItem(DeleteInboxItem {
            index: *index as u32,
        }),
        UserIntent::Scroll(delta) => Intent::Scroll(Scroll { delta: *delta }),
        UserIntent::Quit => Intent::Quit(Quit {}),
        UserIntent::ApproveSessionDocument => {
            Intent::ApproveSessionDocument(ApproveSessionDocument {})
        }
        UserIntent::ViewSessionDocument => Intent::ViewSessionDocument(ViewSessionDocument {}),
        UserIntent::RefineSessionDocument => {
            Intent::RefineSessionDocument(RefineSessionDocument {})
        }
        UserIntent::DismissViewer => Intent::DismissViewer(DismissViewer {}),
        UserIntent::RejectSessionDocument => {
            Intent::RejectSessionDocument(RejectSessionDocument {})
        }
        // skeleton: ResumeFromError has no proto message yet
        UserIntent::ResumeFromError => return None,
        // skeleton: ContinueWithAgent has no proto message yet
        UserIntent::ContinueWithAgent => return None,
        // Local-only (VirtualTui / TUI): sync Select highlight for reconnect snapshots
        UserIntent::SelectHighlightChanged(_) => return None,
        // Local-only: feature-prompt slash menu `/recipe` (no wire proto yet)
        UserIntent::FeatureSlashBuiltinRecipe => return None,
        // Local-only: TUI Stop pane — handled before presenter (`ctrl_c_interrupt_session`)
        UserIntent::Interrupt => return None,
    };
    Some(ClientMessage {
        intent: Some(intent),
    })
}

#[cfg(test)]
mod acceptance_plan_approval_rpc {
    use super::*;
    use prost::Message;
    use tddy_core::{AppMode, ModeChangedDetails, PresenterEvent};

    /// When the workflow asks for session document approval, RPC `ModeChanged` must match
    /// [`AppMode::DocumentReview`] from the presenter.
    #[test]
    fn service_mode_changed_still_serializes_plan_approval() {
        // Given
        let prd = "# Shared PRD body\n".to_string();

        // When
        let from_workflow =
            workflow_event_to_server_message(WorkflowEvent::SessionDocumentApprovalNeeded {
                content: prd.clone(),
            })
            .expect("workflow event");
        let from_document_review =
            event_to_server_message(PresenterEvent::ModeChanged(ModeChangedDetails {
                mode: AppMode::DocumentReview {
                    content: prd.clone(),
                },
                plan_refinement_pending: false,
                skills_project_root: None,
            }));

        // Then
        assert_eq!(
            from_workflow.encode_to_vec(),
            from_document_review.encode_to_vec(),
            "DocumentReview gate must serialize like SessionDocumentApprovalNeeded"
        );
    }
}

#[cfg(test)]
mod token_usage_updated_rpc {
    use super::*;
    use crate::gen::{
        server_message, ConversationRecord as ProtoConversationRecord, TokenUsageUpdated,
    };
    use tddy_core::token_accounting::ConversationRecord;
    use tddy_core::PresenterEvent;

    #[test]
    fn maps_each_conversation_record_field_for_field_onto_the_proto_snapshot() {
        // Given a two-conversation usage snapshot (main agent + one subagent)
        let records = vec![
            ConversationRecord {
                agent: "claude".to_string(),
                id: "claude-main".to_string(),
                model: "claude-opus-4-8".to_string(),
                input_tokens: 12_340,
                output_tokens: 3_210,
                total_tokens: 15_550,
                turns: 7,
            },
            ConversationRecord {
                agent: "Explore".to_string(),
                id: "agent-01".to_string(),
                model: "claude-haiku-4-5".to_string(),
                input_tokens: 4_100,
                output_tokens: 820,
                total_tokens: 4_920,
                turns: 2,
            },
        ];

        // When it is converted for the wire
        let msg = event_to_server_message(PresenterEvent::TokenUsageUpdated(records));

        // Then it is a TokenUsageUpdated event mirroring every record field-for-field
        let Some(server_message::Event::TokenUsageUpdated(TokenUsageUpdated { conversations })) =
            msg.event
        else {
            panic!("expected a TokenUsageUpdated event, got {:?}", msg.event);
        };
        assert_eq!(
            conversations,
            vec![
                ProtoConversationRecord {
                    agent: "claude".to_string(),
                    id: "claude-main".to_string(),
                    model: "claude-opus-4-8".to_string(),
                    input_tokens: 12_340,
                    output_tokens: 3_210,
                    total_tokens: 15_550,
                    turns: 7,
                },
                ProtoConversationRecord {
                    agent: "Explore".to_string(),
                    id: "agent-01".to_string(),
                    model: "claude-haiku-4-5".to_string(),
                    input_tokens: 4_100,
                    output_tokens: 820,
                    total_tokens: 4_920,
                    turns: 2,
                },
            ]
        );
    }
}
