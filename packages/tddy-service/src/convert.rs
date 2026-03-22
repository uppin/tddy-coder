//! Conversion between tddy-core types and proto messages.

use tddy_core::{ActivityKind, AppMode, PresenterEvent, UserIntent, WorkflowEvent};

use crate::gen::{
    app_mode_proto, client_message, server_message, ActivityLogged, AgentOutput, AnswerMultiSelect,
    AnswerOther, AnswerSelect, AnswerText, AppModeDone, AppModeFeatureInput, AppModeMarkdownViewer,
    AppModeMultiSelect, AppModePlanReview, AppModeProto, AppModeRunning, AppModeSelect,
    AppModeTextInput, ApprovePlan, ClarificationQuestionProto, ClientMessage, DeleteInboxItem,
    DismissViewer, EditInboxItem, GoalStarted, InboxChanged, IntentReceived, ModeChanged,
    QuestionOptionProto, QueuePrompt, Quit, RefinePlan, Scroll, ServerMessage, StateChanged,
    SubmitFeatureInput, ViewPlan, WorkflowComplete,
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
        Intent::ApprovePlan(ApprovePlan {}) => Some(UserIntent::ApprovePlan),
        Intent::ViewPlan(ViewPlan {}) => Some(UserIntent::ViewPlan),
        Intent::RefinePlan(RefinePlan {}) => Some(UserIntent::RefinePlan),
        Intent::DismissViewer(DismissViewer {}) => Some(UserIntent::DismissViewer),
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
        WorkflowEvent::PlanApprovalNeeded { prd_content } => Event::ModeChanged(ModeChanged {
            mode: Some(AppModeProto {
                variant: Some(app_mode_proto::Variant::PlanReview(AppModePlanReview {
                    prd_content,
                })),
            }),
        }),
        WorkflowEvent::Progress(_)
        | WorkflowEvent::ClarificationNeeded { .. }
        | WorkflowEvent::WorktreeSwitched { .. }
        | WorkflowEvent::AwaitingFeatureInput => return None,
    };
    Some(ServerMessage { event: Some(event) })
}

/// Build ServerMessage for plan approval elicitation (ModeChanged with PlanReview).
pub fn plan_approval_to_server_message(prd_content: String) -> ServerMessage {
    use server_message::Event;
    ServerMessage {
        event: Some(Event::ModeChanged(ModeChanged {
            mode: Some(AppModeProto {
                variant: Some(app_mode_proto::Variant::PlanReview(AppModePlanReview {
                    prd_content,
                })),
            }),
        })),
    }
}

/// Convert PresenterEvent to ServerMessage.
pub fn event_to_server_message(event: PresenterEvent) -> ServerMessage {
    use server_message::Event;
    match event {
        PresenterEvent::ModeChanged(mode) => ServerMessage {
            event: Some(Event::ModeChanged(ModeChanged {
                mode: Some(app_mode_to_proto(&mode)),
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
        PresenterEvent::BackendSelected { .. } => ServerMessage { event: None },
        PresenterEvent::ShouldQuit => {
            if let Some(msg) = intent_to_client_message(&UserIntent::Quit) {
                ServerMessage {
                    event: Some(Event::IntentReceived(IntentReceived { intent: Some(msg) })),
                }
            } else {
                ServerMessage { event: None }
            }
        }
    }
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
        AppMode::PlanReview { prd_content } => {
            app_mode_proto::Variant::PlanReview(AppModePlanReview {
                prd_content: prd_content.clone(),
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
    }
}

fn activity_kind_to_str(k: &ActivityKind) -> String {
    match k {
        ActivityKind::ToolUse => "ToolUse".to_string(),
        ActivityKind::TaskStarted => "TaskStarted".to_string(),
        ActivityKind::TaskProgress => "TaskProgress".to_string(),
        ActivityKind::StateChange => "StateChange".to_string(),
        ActivityKind::Info => "Info".to_string(),
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
        UserIntent::ApprovePlan => Intent::ApprovePlan(ApprovePlan {}),
        UserIntent::ViewPlan => Intent::ViewPlan(ViewPlan {}),
        UserIntent::RefinePlan => Intent::RefinePlan(RefinePlan {}),
        UserIntent::DismissViewer => Intent::DismissViewer(DismissViewer {}),
        // skeleton: ResumeFromError has no proto message yet
        UserIntent::ResumeFromError => return None,
        // skeleton: ContinueWithAgent has no proto message yet
        UserIntent::ContinueWithAgent => return None,
        // Local-only (VirtualTui / TUI): sync Select highlight for reconnect snapshots
        UserIntent::SelectHighlightChanged(_) => return None,
    };
    Some(ClientMessage {
        intent: Some(intent),
    })
}
