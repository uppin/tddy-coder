//! Construction and the builder-style wiring each caller does before the presenter runs.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use super::Presenter;
use crate::presenter::intent::UserIntent;
use crate::presenter::presenter_events::PresenterEvent;
use crate::presenter::state::{AppMode, PresenterState};
use crate::presenter::state_groups::{
    ActivityRecorder, BackendSelection, PendingQuestions, RecipeResolverFn, ViewChannels,
    WorkflowRun,
};
use crate::WorkflowRecipe;

impl Presenter {
    /// Create a new Presenter in FeatureInput mode.
    pub fn new(
        agent: impl Into<String>,
        model: impl Into<String>,
        workflow_recipe: Arc<dyn WorkflowRecipe>,
        tddy_data_dir: PathBuf,
    ) -> Self {
        let state = PresenterState {
            agent: agent.into(),
            model: model.into(),
            mode: AppMode::FeatureInput,
            current_goal: None,
            current_state: None,
            workflow_session_id: None,
            goal_start_time: std::time::Instant::now(),
            activity_log: Vec::new(),
            inbox: Vec::new(),
            should_quit: false,
            exit_action: None,
            plan_refinement_pending: false,
            skills_project_root: None,
            active_worktree_display: None,
        };
        Presenter {
            state,
            workflow: WorkflowRun::default(),
            questions: PendingQuestions::default(),
            activity: ActivityRecorder::default(),
            views: ViewChannels::default(),
            backend: BackendSelection::new(workflow_recipe),
            tddy_data_dir,
        }
    }

    /// Set where the agent's own tool calls are persisted (`agent-activity.jsonl` under
    /// `dir`), which `worktree` those calls run in, and the `source` recorded on each row. Wired by
    /// `tddy-coder` for tool / cursor-cli sessions, where the coder process — not the daemon —
    /// executes the tools.
    ///
    /// `worktree` is asked for rather than derived because `dir` is the session folder and the
    /// checkout is somewhere else entirely; only the caller that started the session knows which
    /// one. It is an `Option` so a caller that does not know must say so, rather than the presenter
    /// quietly stamping records against whatever directory it happened to hold — see
    /// [`ActivityRecorder::worktree`].
    pub fn set_agent_activity_context(
        &mut self,
        dir: PathBuf,
        worktree: Option<PathBuf>,
        source: impl Into<String>,
    ) {
        self.activity.dir = Some(dir);
        self.activity.worktree = worktree;
        self.activity.source = source.into();
    }

    /// Enable broadcast of PresenterEvents (for gRPC subscribers).
    pub fn with_broadcast(mut self, tx: tokio::sync::broadcast::Sender<PresenterEvent>) -> Self {
        self.views.broadcast_tx = Some(tx);
        self
    }

    /// Enable connect_view() by providing an intent sender for external views.
    pub fn with_intent_sender(mut self, tx: mpsc::Sender<UserIntent>) -> Self {
        self.views.intent_tx = Some(tx);
        self
    }

    /// Resolve workflow recipe CLI names when the user picks `/recipe` → TDD or Bugfix.
    pub fn with_recipe_resolver(mut self, resolver: Arc<RecipeResolverFn>) -> Self {
        self.backend.recipe_resolver = Some(resolver);
        self
    }

    /// Pre-set worktree dir so the workflow skips git fetch / worktree creation.
    pub fn with_worktree_dir(mut self, dir: PathBuf) -> Self {
        self.workflow.worktree_dir = Some(dir);
        self
    }
}
