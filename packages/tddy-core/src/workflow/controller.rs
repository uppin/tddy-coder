//! `WorkflowController` — the passive state machine for **agent-driven** orchestration.
//!
//! In the engine-driven path (`WorkflowEngine` + `FlowRunner`), an external driver decides "what
//! runs next" and respawns the agent per goal. In the agent-driven path the control is inverted:
//! **one** long-lived agent session is the orchestrator and drives the workflow itself by calling a
//! `transition` tool. This controller is what that tool talks to — it holds the current position in
//! the recipe graph, validates a requested transition against the graph's edges, persists the new
//! state to `changeset.yaml`, and emits the same [`WorkflowEvent`]s the engine path emits (so the
//! TUI/web are unaffected — "API intact").
//!
//! Subagent transitions are **provisional**: the orchestrator agent must verify a subagent's work
//! and make the go/no-go commit itself. The controller records a provisional transition without
//! persisting or emitting; a later authoritative transition (from the orchestrator) commits.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::backend::{GoalId, WorkflowRecipe};
use crate::changeset::{read_changeset, update_state, write_changeset_atomic};
use crate::error::WorkflowError;
use crate::presenter::WorkflowEvent;
use crate::workflow::ids::WorkflowState;
use tddy_graph::graph::Graph;

/// Result of a [`WorkflowController::transition`] request.
#[derive(Debug, Clone)]
pub enum TransitionOutcome {
    /// Authoritative transition committed: state persisted, events emitted, next-goal instructions
    /// returned to the agent.
    Committed {
        from: GoalId,
        to: GoalId,
        instructions: String,
    },
    /// Subagent (provisional) transition recorded but **not** committed. The orchestrator must
    /// verify and commit.
    RecordedProvisional { to: GoalId },
    /// Transition refused (illegal edge, no-op, or persistence failure). `reason` is agent-facing.
    Rejected { reason: String },
}

struct ControllerInner {
    current: GoalId,
    /// Last provisional (subagent) transition target awaiting orchestrator go/no-go.
    provisional: Option<GoalId>,
}

/// Passive state machine driven by the agent's `transition` tool calls.
pub struct WorkflowController {
    recipe: Arc<dyn WorkflowRecipe>,
    graph: Arc<Graph>,
    /// Session artifact dir holding `changeset.yaml`. `None` = in-memory only (tests / no persistence).
    session_dir: Option<PathBuf>,
    event_tx: Option<Sender<WorkflowEvent>>,
    inner: Mutex<ControllerInner>,
}

impl WorkflowController {
    /// Create a controller positioned at the recipe's start goal.
    pub fn new(
        recipe: Arc<dyn WorkflowRecipe>,
        graph: Arc<Graph>,
        session_dir: Option<PathBuf>,
        event_tx: Option<Sender<WorkflowEvent>>,
    ) -> Self {
        let start = recipe.start_goal();
        Self::new_at(recipe, graph, session_dir, event_tx, start)
    }

    /// Create a controller positioned at `start` (use when resuming mid-workflow).
    pub fn new_at(
        recipe: Arc<dyn WorkflowRecipe>,
        graph: Arc<Graph>,
        session_dir: Option<PathBuf>,
        event_tx: Option<Sender<WorkflowEvent>>,
        start: GoalId,
    ) -> Self {
        Self {
            recipe,
            graph,
            session_dir,
            event_tx,
            inner: Mutex::new(ControllerInner {
                current: start,
                provisional: None,
            }),
        }
    }

    /// The goal the workflow is currently at.
    pub fn current_goal(&self) -> GoalId {
        self.inner.lock().expect("controller mutex").current.clone()
    }

    /// The pending provisional (subagent) transition target, if any.
    pub fn pending_provisional(&self) -> Option<GoalId> {
        self.inner
            .lock()
            .expect("controller mutex")
            .provisional
            .clone()
    }

    /// Request a transition to `to`.
    ///
    /// `provisional` is `true` for subagent-originated calls (detected via `parent_tool_use_id` at
    /// the tool boundary): the intent is recorded but not committed. `provisional == false` is an
    /// authoritative orchestrator commit: validate the edge, persist, emit events, and return the
    /// next goal's instructions.
    pub fn transition(&self, to: GoalId, provisional: bool) -> TransitionOutcome {
        let mut inner = self.inner.lock().expect("controller mutex");
        let from = inner.current.clone();

        if to == from {
            return TransitionOutcome::Rejected {
                reason: format!("already at goal '{from}'; no transition performed"),
            };
        }

        let valid = self.graph.successors(from.as_str());
        if !valid.iter().any(|s| s == to.as_str()) {
            return TransitionOutcome::Rejected {
                reason: format!(
                    "'{to}' is not a valid transition from '{from}'. Valid next goals: [{}]",
                    valid.join(", ")
                ),
            };
        }

        if provisional {
            inner.provisional = Some(to.clone());
            return TransitionOutcome::RecordedProvisional { to };
        }

        // Authoritative commit: persist first — only mutate current + emit once state is durable, so
        // a failed write never leaves in-memory position ahead of `changeset.yaml`.
        if let Some(sd) = &self.session_dir {
            if let Err(e) = self.persist_state(sd, &to) {
                return TransitionOutcome::Rejected {
                    reason: format!("failed to persist transition to '{to}': {e}"),
                };
            }
        }

        inner.current = to.clone();
        inner.provisional = None;
        drop(inner);

        self.emit(WorkflowEvent::StateChange {
            from: from.to_string(),
            to: to.to_string(),
        });
        self.emit(WorkflowEvent::GoalStarted(to.to_string()));

        TransitionOutcome::Committed {
            from,
            instructions: self.recipe.goal_instructions(&to),
            to,
        }
    }

    /// Persist the goal as the current workflow state in `changeset.yaml`.
    ///
    /// The agent-driven path uses the goal id as the persisted state string (the engine path's
    /// richer `WorkflowState` vocabulary — `RedTestsReady`, … — is set by engine hooks and is not
    /// used here). `GoalStarted` carries the goal for display; this keeps resume consistent within
    /// the agent-driven path.
    fn persist_state(
        &self,
        session_dir: &std::path::Path,
        to: &GoalId,
    ) -> Result<(), WorkflowError> {
        let mut cs = read_changeset(session_dir)?;
        update_state(&mut cs, WorkflowState::new(to.as_str()));
        write_changeset_atomic(session_dir, &cs)
    }

    fn emit(&self, event: WorkflowEvent) {
        if let Some(tx) = &self.event_tx {
            // A dropped receiver (view detached) is not an error worth failing the transition over.
            let _ = tx.send(event);
        }
    }
}

/// Bridge to the `toolcall` relay so an `Arc<WorkflowController>` can be registered as the process
/// transition handler ([`crate::toolcall::register_transition_handler`]).
impl crate::toolcall::TransitionHandler for WorkflowController {
    fn handle_transition(
        &self,
        to: &str,
        provisional: bool,
    ) -> crate::toolcall::TransitionRelayOutcome {
        use crate::toolcall::TransitionRelayOutcome as Relay;
        match self.transition(GoalId::new(to), provisional) {
            TransitionOutcome::Committed { instructions, .. } => Relay::Committed { instructions },
            TransitionOutcome::RecordedProvisional { to } => Relay::Provisional {
                to: to.into_inner(),
            },
            TransitionOutcome::Rejected { reason } => Relay::Rejected { reason },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::CodingBackend;
    use crate::workflow::graph::GraphBuilder;
    use crate::workflow::hooks::RunnerHooks;
    use crate::workflow::recipe::WorkflowEventSender;
    use std::collections::BTreeMap;
    use std::sync::mpsc;
    use tddy_graph::task::EndTask;

    /// Self-contained recipe with a real graph: a -> b, then b -> (c | d) conditional plus a static
    /// b -> c edge (to exercise successor dedup). `tddy-core` must not depend on
    /// `tddy-workflow-recipes`, so the topology is defined inline.
    #[derive(Debug, Clone, Copy, Default)]
    struct TestRecipe;

    impl WorkflowRecipe for TestRecipe {
        fn name(&self) -> &str {
            "controller_test"
        }
        fn build_graph(&self, _backend: Arc<dyn CodingBackend>) -> Graph {
            GraphBuilder::new("controller_test")
                .add_task(Arc::new(EndTask::new("a")))
                .add_task(Arc::new(EndTask::new("b")))
                .add_task(Arc::new(EndTask::new("c")))
                .add_task(Arc::new(EndTask::new("d")))
                .add_edge("a", "b")
                .add_conditional_edge("b", |_| true, "c", "d")
                .add_edge("b", "c")
                .build()
        }
        fn create_hooks(&self, _tx: Option<WorkflowEventSender>) -> Arc<dyn RunnerHooks> {
            unimplemented!("not used by controller tests")
        }
        fn goal_hints(&self, _goal_id: &GoalId) -> Option<crate::backend::GoalHints> {
            None
        }
        fn goal_ids(&self) -> Vec<GoalId> {
            ["a", "b", "c", "d"].into_iter().map(GoalId::new).collect()
        }
        fn goal_instructions(&self, goal_id: &GoalId) -> String {
            format!("INSTRUCTIONS:{goal_id}")
        }
        fn submit_key(&self, goal_id: &GoalId) -> GoalId {
            goal_id.clone()
        }
        fn next_goal_for_state(&self, _state: &WorkflowState) -> Option<GoalId> {
            None
        }
        fn status_for_state(&self, _state: &WorkflowState) -> &'static str {
            "Active"
        }
        fn initial_state(&self) -> WorkflowState {
            WorkflowState::new("a")
        }
        fn start_goal(&self) -> GoalId {
            GoalId::new("a")
        }
        fn default_models(&self) -> BTreeMap<GoalId, String> {
            BTreeMap::new()
        }
        fn goal_requires_session_dir(&self, _goal_id: &GoalId) -> bool {
            false
        }
        fn plain_goal_cli_output(
            &self,
            _goal_id: &GoalId,
            _output: Option<&str>,
            _session_dir: &std::path::Path,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    fn controller(event_tx: Option<Sender<WorkflowEvent>>) -> WorkflowController {
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TestRecipe);
        let backend: Arc<dyn CodingBackend> = Arc::new(crate::backend::StubBackend::new());
        let graph = Arc::new(recipe.build_graph(backend));
        WorkflowController::new(recipe, graph, None, event_tx)
    }

    #[test]
    fn starts_at_recipe_start_goal() {
        // Given / When a fresh controller — Then it sits at the recipe's start goal (`a`)
        assert_eq!(controller(None).current_goal().as_str(), "a");
    }

    #[test]
    fn successors_includes_both_conditional_branches_deduped() {
        // Given a graph where `b` has a conditional edge (c|d) plus a static edge to c
        let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TestRecipe);
        let backend: Arc<dyn CodingBackend> = Arc::new(crate::backend::StubBackend::new());
        let graph = recipe.build_graph(backend);

        // When listing successors of `b`
        let mut s = graph.successors("b");
        s.sort();

        // Then both conditional branches appear, deduped against the static edge
        assert_eq!(s, vec!["c".to_string(), "d".to_string()]);
    }

    #[test]
    fn rejects_illegal_transition() {
        // Given a controller at `a` (whose only legal edge is a -> b)
        let c = controller(None);

        // When requesting a transition along a non-existent edge (a -> c)
        let out = c.transition(GoalId::new("c"), false);

        // Then it is rejected and the position is unchanged
        match out {
            TransitionOutcome::Rejected { reason } => assert!(reason.contains("not a valid")),
            other => panic!("expected Rejected, got {other:?}"),
        }
        assert_eq!(c.current_goal().as_str(), "a");
    }

    #[test]
    fn rejects_noop_transition_to_current() {
        // Given a controller at `a`
        let c = controller(None);

        // When / Then a transition to the current goal is rejected as a no-op
        assert!(matches!(
            c.transition(GoalId::new("a"), false),
            TransitionOutcome::Rejected { .. }
        ));
    }

    #[test]
    fn authoritative_transition_commits_emits_and_returns_instructions() {
        // Given a controller at `a` with an event receiver
        let (tx, rx) = mpsc::channel();
        let c = controller(Some(tx));

        // When committing an authoritative transition along the legal edge a -> b
        let out = c.transition(GoalId::new("b"), false);

        // Then it commits, moves to `b`, and returns `b`'s instructions
        match out {
            TransitionOutcome::Committed {
                from,
                to,
                instructions,
            } => {
                assert_eq!(from.as_str(), "a");
                assert_eq!(to.as_str(), "b");
                assert_eq!(instructions, "INSTRUCTIONS:b");
            }
            other => panic!("expected Committed, got {other:?}"),
        }
        assert_eq!(c.current_goal().as_str(), "b");

        // Then it emits StateChange then GoalStarted
        match rx.recv().expect("StateChange") {
            WorkflowEvent::StateChange { from, to } => {
                assert_eq!(from, "a");
                assert_eq!(to, "b");
            }
            other => panic!("expected StateChange, got {other:?}"),
        }
        match rx.recv().expect("GoalStarted") {
            WorkflowEvent::GoalStarted(g) => assert_eq!(g, "b"),
            other => panic!("expected GoalStarted, got {other:?}"),
        }
    }

    #[test]
    fn provisional_subagent_transition_does_not_commit_or_emit() {
        // Given a controller at `a` with an event receiver
        let (tx, rx) = mpsc::channel();
        let c = controller(Some(tx));

        // When a subagent requests a provisional transition to `b`
        let out = c.transition(GoalId::new("b"), true);

        // Then it is recorded but not committed, and nothing is emitted
        assert!(matches!(out, TransitionOutcome::RecordedProvisional { .. }));
        assert_eq!(c.current_goal().as_str(), "a");
        assert_eq!(c.pending_provisional().unwrap().as_str(), "b");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn orchestrator_commit_after_provisional_clears_provisional() {
        // Given a controller with a pending provisional transition to `b`
        let c = controller(None);
        let _ = c.transition(GoalId::new("b"), true);
        assert!(c.pending_provisional().is_some());

        // When the orchestrator commits the same transition authoritatively
        let out = c.transition(GoalId::new("b"), false);

        // Then it commits and the provisional marker is cleared
        assert!(matches!(out, TransitionOutcome::Committed { .. }));
        assert!(c.pending_provisional().is_none());
    }
}
