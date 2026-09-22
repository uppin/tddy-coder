//! The surfaces outside the presenter: the views it broadcasts to, the intents they send back,
//! and the shared critical state they read. [`Presenter::handle_intent`] dispatches each intent to
//! the partition that owns the state it changes.

use super::Presenter;
use crate::presenter::activity_prompt_log;
use crate::presenter::intent::UserIntent;
use crate::presenter::presenter_events::{PresenterEvent, ViewConnection};
use crate::presenter::state::{ActivityEntry, ActivityKind, AppMode};

impl Presenter {
    /// Create a new view connection: state snapshot + event subscription + intent sender.
    /// Returns None if broadcast or intent_tx is not configured.
    pub fn connect_view(&self) -> Option<ViewConnection> {
        let broadcast_tx = self.views.broadcast_tx.as_ref()?;
        let intent_tx = self.views.intent_tx.clone()?;
        Some(ViewConnection {
            state_snapshot: self.state.clone(),
            event_rx: broadcast_tx.subscribe(),
            intent_tx,
            critical_state: self.views.critical_state.clone(),
        })
    }

    /// Handle a user intent. Updates state and may send answers to workflow.
    pub fn handle_intent(&mut self, intent: UserIntent) {
        if let UserIntent::SelectHighlightChanged(idx) = &intent {
            if self.select_highlight_matches(*idx) {
                return;
            }
        }
        self.broadcast(PresenterEvent::IntentReceived(intent.clone()));
        match intent {
            UserIntent::SubmitFeatureInput(text) => self.submit_feature_input(text),
            UserIntent::FeatureSlashBuiltinRecipe => {
                self.apply_feature_slash_builtin_recipe();
            }
            UserIntent::ApproveSessionDocument => self.approve_session_document(),
            UserIntent::ViewSessionDocument => self.view_session_document(),
            UserIntent::RejectSessionDocument => self.reject_session_document(),
            UserIntent::RefineSessionDocument => self.refine_session_document(),
            UserIntent::DismissViewer => self.dismiss_viewer(),
            UserIntent::AnswerSelect(idx) => self.answer_select(idx),
            UserIntent::AnswerOther(text) => self.answer_other(text),
            UserIntent::AnswerMultiSelect(indices, other) => {
                self.answer_multi_select(indices, other)
            }
            UserIntent::AnswerText(text) => self.answer_text(text),
            UserIntent::QueuePrompt(text) => self.queue_prompt(text),
            UserIntent::EditInboxItem { index, text } => self.edit_inbox_item(index, text),
            UserIntent::DeleteInboxItem(index) => self.delete_inbox_item(index),
            UserIntent::Scroll(_) => {
                // View-local; no-op in Presenter
            }
            UserIntent::SelectHighlightChanged(idx) => {
                self.sync_select_highlight(idx);
            }
            UserIntent::Quit => {
                self.state.should_quit = true;
            }
            UserIntent::Interrupt => {
                // TUI / VirtualTui call `ctrl_c_interrupt_session` without sending this intent.
            }
            UserIntent::ContinueWithAgent => self.continue_with_agent(),
            UserIntent::ResumeFromError => self.resume_from_error(),
        }
    }

    fn select_highlight_matches(&self, idx: usize) -> bool {
        matches!(
            &self.state.mode,
            AppMode::Select {
                initial_selected,
                ..
            } if *initial_selected == idx
        )
    }

    /// Sync presenter Select highlight (for reconnect snapshots). No-op if not in Select mode.
    fn sync_select_highlight(&mut self, idx: usize) {
        let (question, question_index, total_questions) = match &self.state.mode {
            AppMode::Select {
                question,
                question_index,
                total_questions,
                ..
            } => (question.clone(), *question_index, *total_questions),
            _ => return,
        };
        let max = question.options.len() + if question.allow_other { 1 } else { 0 };
        if max == 0 || idx >= max {
            return;
        }
        self.state.mode = AppMode::Select {
            question,
            question_index,
            total_questions,
            initial_selected: idx,
        };
        self.broadcast_mode_changed();
    }

    /// Queue a prompt for after the running workflow, or answer an open question with it.
    fn queue_prompt(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        if self.questions.awaiting_open_answer {
            if let Some(ref tx) = self.workflow.answer_tx {
                log::debug!(
                    "QueuePrompt → answer_tx (awaiting_open_answer, len={})",
                    text.len()
                );
                let _ = tx.send(text);
                return;
            }
        }
        let queued_line = activity_prompt_log::format_queued_prompt_line(&text);
        if !queued_line.is_empty() {
            self.log_activity(queued_line, ActivityKind::UserPrompt);
        }
        self.state.inbox.push(text);
        self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
    }

    /// Replace a queued prompt.
    fn edit_inbox_item(&mut self, index: usize, text: String) {
        if index < self.state.inbox.len() {
            self.state.inbox[index] = text;
            self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
        }
    }

    /// Drop a queued prompt.
    fn delete_inbox_item(&mut self, index: usize) {
        if index < self.state.inbox.len() {
            self.state.inbox.remove(index);
            self.broadcast(PresenterEvent::InboxChanged(self.state.inbox.clone()));
        }
    }

    /// Record a state transition and mirror it into the views' critical state.
    pub(super) fn on_state_change(&mut self, from: String, to: String) {
        self.state.current_state = Some(to.clone());
        if let Ok(mut cs) = self.views.critical_state.lock() {
            cs.current_state = Some(to.clone());
        }
        let entry = ActivityEntry {
            text: format!("State: {} → {}", from, to),
            kind: ActivityKind::StateChange,
        };
        self.state.activity_log.push(entry.clone());
        self.broadcast(PresenterEvent::ActivityLogged(entry));
        self.broadcast(PresenterEvent::StateChanged {
            from: from.clone(),
            to: to.clone(),
        });
    }

    /// Record the goal the workflow started and mirror it into the views' critical state.
    pub(super) fn on_goal_started(&mut self, goal: String) {
        self.questions.awaiting_open_answer = false;
        self.state.current_goal = Some(goal.clone());
        if let Ok(mut cs) = self.views.critical_state.lock() {
            cs.current_goal = Some(goal.clone());
        }
        self.state.goal_start_time = std::time::Instant::now();
        if matches!(self.state.mode, AppMode::FeatureInput) {
            self.state.mode = AppMode::Running;
            self.broadcast_mode_changed();
        }
        self.broadcast(PresenterEvent::GoalStarted(goal.clone()));
    }
}
