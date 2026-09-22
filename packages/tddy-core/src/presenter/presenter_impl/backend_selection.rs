//! Which backend and recipe this session runs, including the deferred-start path.

use super::{DeferredBackendFactory, PendingWorkflowStart, Presenter};
use crate::presenter::presenter_events::PresenterEvent;
use crate::presenter::state::AppMode;
use crate::{ClarificationQuestion, SharedBackend};

impl Presenter {
    /// Show interactive backend selection (synthetic single-select question).
    pub fn show_backend_selection(
        &mut self,
        question: ClarificationQuestion,
        initial_selected: usize,
    ) {
        self.backend.selection_pending = true;
        self.questions.questions = vec![question.clone()];
        self.questions.current_index = 0;
        self.questions.collected_answers.clear();
        self.state.mode = AppMode::Select {
            question,
            question_index: 0,
            total_questions: 1,
            initial_selected,
        };
        self.broadcast_mode_changed();
    }

    /// Configure backend creation + first workflow start after interactive backend selection (tddy-coder TUI).
    pub fn configure_deferred_workflow_start(
        &mut self,
        factory: DeferredBackendFactory,
        pending: PendingWorkflowStart,
        cli_model_override: Option<String>,
    ) {
        self.state.skills_project_root = Some(pending.output_dir.clone());
        self.backend.deferred_factory = Some(factory);
        self.backend.pending_start = Some(pending);
        self.backend.deferred_cli_model = cli_model_override;
    }

    /// True while waiting for user to pick a coding backend at session start.
    #[must_use]
    pub fn is_backend_selection_pending(&self) -> bool {
        self.backend.selection_pending
    }

    fn broadcast_error_recovery(&mut self, error_message: String) {
        self.state.mode = AppMode::ErrorRecovery { error_message };
        self.broadcast_mode_changed();
    }

    fn start_workflow_from_pending_if_any(&mut self, backend: SharedBackend) {
        let Some(pending) = self.backend.pending_start.take() else {
            return;
        };
        self.backend.deferred_cli_model = None;
        self.start_workflow(
            backend,
            pending.output_dir,
            pending.session_dir,
            pending.initial_prompt,
            pending.conversation_output_path,
            pending.debug_output_path,
            pending.debug,
            pending.session_id,
            pending.socket_path,
            pending.tool_call_rx,
        );
    }

    fn apply_deferred_backend_factory(&mut self, factory: DeferredBackendFactory, agent_str: &str) {
        match factory(agent_str) {
            Ok(backend) => self.start_workflow_from_pending_if_any(backend),
            Err(msg) => self.broadcast_error_recovery(msg),
        }
    }

    /// Resolves interactive backend selection (`show_backend_selection`). No-op if the index is invalid.
    fn handle_backend_selection_answer(&mut self, idx: usize) {
        let Some(q) = self.questions.questions.first() else {
            return;
        };
        if idx >= q.options.len() {
            return;
        }
        let label = q.options[idx].label.clone();
        let (agent, model) = crate::backend::backend_from_label(&label);
        let agent_str = agent.to_string();
        self.state.agent = agent_str.clone();
        self.state.model = model.to_string();
        if let Some(ref m) = self.backend.deferred_cli_model {
            self.state.model = m.clone();
        }
        self.backend.selection_pending = false;
        self.questions.questions.clear();
        self.questions.current_index = 0;
        self.questions.collected_answers.clear();
        self.state.mode = AppMode::FeatureInput;
        self.broadcast_mode_changed();
        self.broadcast(PresenterEvent::BackendSelected {
            agent: agent_str.clone(),
            model: self.state.model.clone(),
        });
        let Some(factory) = self.backend.deferred_factory.take() else {
            return;
        };
        self.apply_deferred_backend_factory(factory, agent_str.as_str());
    }

    fn handle_recipe_slash_selection_answer(&mut self, idx: usize) {
        let Some(q) = self.questions.questions.first() else {
            self.backend.recipe_slash_selection_pending = false;
            return;
        };
        if idx >= q.options.len() {
            return;
        }
        let label = q.options[idx].label.clone();
        self.backend.recipe_slash_selection_pending = false;
        self.questions.questions.clear();
        self.questions.current_index = 0;
        self.questions.collected_answers.clear();

        let Some(cli_name) = crate::backend::recipe_cli_name_from_selection_label(&label) else {
            log::warn!("recipe slash: unknown option label {:?}", label);
            self.state.mode = AppMode::FeatureInput;
            self.broadcast_mode_changed();
            return;
        };
        if let Some(ref resolve) = self.backend.recipe_resolver {
            match resolve(cli_name) {
                Ok(new_recipe) => {
                    log::info!("recipe slash: active workflow recipe set to `{cli_name}`");
                    self.backend.recipe = new_recipe;
                }
                Err(e) => {
                    log::warn!("recipe slash: could not resolve `{cli_name}`: {e}");
                }
            }
        } else {
            log::debug!("recipe slash: no recipe_resolver; recipe unchanged after UI pick");
        }
        self.state.mode = AppMode::FeatureInput;
        self.broadcast_mode_changed();
    }

    /// User accepted the `/recipe` built-in from the feature slash menu (PRD).
    pub fn apply_feature_slash_builtin_recipe(&mut self) {
        if !matches!(self.state.mode, AppMode::FeatureInput) {
            log::debug!(
                "apply_feature_slash_builtin_recipe: no-op (mode={:?})",
                self.state.mode
            );
            return;
        }
        log::info!("apply_feature_slash_builtin_recipe: showing workflow recipe selection");
        self.backend.recipe_slash_selection_pending = true;
        self.questions.questions = vec![crate::backend::workflow_recipe_selection_question()];
        self.questions.current_index = 0;
        self.questions.collected_answers.clear();
        self.advance_to_next_question();
    }

    /// Whether the presenter is in recipe selection after `/recipe` from slash menu.
    pub fn recipe_slash_selection_active(&self) -> bool {
        let active = self.backend.recipe_slash_selection_pending
            && matches!(self.state.mode, AppMode::Select { .. });
        log::debug!("recipe_slash_selection_active: {active}");
        active
    }

    /// Route a selected option: to the pending backend selection, else to the pending `/recipe`
    /// slash selection, else to the current question via `answer_selected_option`.
    pub(super) fn route_select_answer(&mut self, idx: usize) {
        if self.backend.selection_pending {
            self.handle_backend_selection_answer(idx);
            return;
        }
        if self.backend.recipe_slash_selection_pending {
            self.handle_recipe_slash_selection_answer(idx);
            return;
        }
        self.answer_selected_option(idx);
    }
}
