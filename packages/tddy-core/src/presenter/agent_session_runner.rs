//! `AgentSessionRunner` — the Claude-first, **agent-driven** workflow runner.
//!
//! Where [`crate::presenter::workflow_runner::run_workflow`] drives the engine (respawning the agent
//! per goal), this runs the **whole workflow in one backend invocation**. The agent is the
//! orchestrator: it works, then calls `tddy-tools transition --to <goal>` to advance the
//! [`WorkflowController`], reads the returned instructions, and continues in the same chat. No
//! per-goal respawn.
//!
//! Pause gates (clarification via `tddy-tools ask`, approvals) are unchanged — they still relay
//! through the presenter's tool-call loop *concurrently* with this blocking invoke, so nothing
//! extra is needed here for them.

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use crate::backend::{
    CodingBackend, GoalId, InvokeRequest, SessionMode, SharedBackend, WorkflowRecipe,
};
use crate::presenter::{WorkflowCompletePayload, WorkflowEvent};
use crate::workflow::controller::WorkflowController;

/// Backends capable of the agent-driven runner (single persistent session + subagents). Additive:
/// everything else keeps the engine path. Gated further by an explicit opt-in in the selector, so
/// enabling agent-driven mode is never implicit.
pub fn backend_is_agent_driven(backend_name: &str) -> bool {
    matches!(backend_name, "claude" | "claude-acp")
}

/// Whether agent-driven orchestration is opted into for this process (env `TDDY_AGENT_DRIVEN`).
/// Kept explicit so the default remains the engine path and existing flows/tests are unaffected.
pub fn agent_driven_enabled() -> bool {
    std::env::var("TDDY_AGENT_DRIVEN")
        .map(|v| matches!(v.trim(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Inputs for one agent-driven session run.
pub struct AgentSessionConfig {
    pub recipe: Arc<dyn WorkflowRecipe>,
    pub backend: SharedBackend,
    pub event_tx: mpsc::Sender<WorkflowEvent>,
    pub session_dir: Option<PathBuf>,
    pub session_id: Option<String>,
    pub model: Option<String>,
    /// The user's feature/request that seeds the session.
    pub initial_prompt: String,
    pub working_dir: Option<PathBuf>,
    pub socket_path: Option<PathBuf>,
    pub conversation_output_path: Option<PathBuf>,
    pub debug: bool,
    /// Resume target; defaults to `recipe.start_goal()`.
    pub start_goal: Option<GoalId>,
}

/// Run one agent-driven session to completion. Emits `GoalStarted`, then `StateChange`/`GoalStarted`
/// for each committed transition (via the controller), then `WorkflowComplete`.
pub async fn run_agent_session(config: AgentSessionConfig) {
    let AgentSessionConfig {
        recipe,
        backend,
        event_tx,
        session_dir,
        session_id,
        model,
        initial_prompt,
        working_dir,
        socket_path,
        conversation_output_path,
        debug,
        start_goal,
    } = config;

    let graph = Arc::new(recipe.build_graph(backend.as_arc()));
    let start = start_goal.unwrap_or_else(|| recipe.start_goal());

    let controller = Arc::new(WorkflowController::new_at(
        recipe.clone(),
        graph,
        session_dir.clone(),
        Some(event_tx.clone()),
        start.clone(),
    ));

    // Register so `tddy-tools transition` (relayed through the toolcall listener) reaches this
    // controller. Cleared on completion below.
    crate::toolcall::register_transition_handler(
        controller.clone() as Arc<dyn crate::toolcall::TransitionHandler>
    );

    // Announce the starting goal for the UI (mirrors the engine path's GoalStarted).
    let _ = event_tx.send(WorkflowEvent::GoalStarted(start.to_string()));

    let request = InvokeRequest {
        prompt: initial_prompt,
        system_prompt: Some(recipe.orchestration_system_prompt(&start)),
        system_prompt_path: None,
        goal_id: start.clone(),
        submit_key: start.clone(),
        hints: recipe.orchestration_hints(&start),
        model,
        session: session_id.map(SessionMode::Fresh),
        working_dir,
        debug,
        agent_output: true,
        agent_output_sink: crate::workflow::get_agent_sink(),
        progress_sink: crate::workflow::get_progress_sink(),
        conversation_output_path,
        inherit_stdin: false,
        extra_allowed_tools: None,
        socket_path,
        session_dir: session_dir.clone(),
        remote: None,
    };

    let result = backend.invoke(request).await;

    crate::toolcall::clear_transition_handler();

    let payload = match result {
        Ok(resp) => Ok(WorkflowCompletePayload {
            summary: recipe
                .summarize_last_goal_output(&resp.output)
                .unwrap_or_else(|| "Workflow session complete".to_string()),
            session_dir,
        }),
        Err(e) => Err(format!("agent session failed: {e}")),
    };
    let _ = event_tx.send(WorkflowEvent::WorkflowComplete(payload));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockBackend;
    use crate::test_support::LinearTestRecipe;

    /// Serializes tests that register the *process-global* transition handler, which they otherwise
    /// race over under the default parallel test runner.
    static HANDLER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A single agent-driven invoke drives the whole workflow: the (mocked) agent performs
    /// authoritative transitions a→b→c, the controller commits each (StateChange + GoalStarted),
    /// and the run ends with WorkflowComplete. No per-goal respawn (one invoke).
    // Held across `.await` to serialize the process-global handler registry; safe under the
    // single-threaded `#[tokio::test]` runtime (the guard never moves across threads).
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn agent_session_drives_workflow_via_transitions_in_one_invoke() {
        let _guard = HANDLER_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // Given a linear recipe (a->b->c) and an agent scripted to commit a->b then b->c
        let recipe = Arc::new(LinearTestRecipe);
        let mock = MockBackend::new();
        mock.push_ok_driving_transitions(
            "done",
            vec![("b".to_string(), false), ("c".to_string(), false)],
        );
        let backend = SharedBackend::from_arc(Arc::new(mock));

        // When running the whole workflow as one agent session
        let (event_tx, event_rx) = mpsc::channel();
        run_agent_session(AgentSessionConfig {
            recipe: recipe.clone(),
            backend: backend.clone(),
            event_tx,
            session_dir: None,
            session_id: Some("sess-1".to_string()),
            model: None,
            initial_prompt: "build a thing".to_string(),
            working_dir: None,
            socket_path: None,
            conversation_output_path: None,
            debug: false,
            start_goal: None,
        })
        .await;

        // Then goals start a->b->c, states change a->b->c, and the run completes
        let events: Vec<WorkflowEvent> = event_rx.try_iter().collect();
        let goal_starts: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                WorkflowEvent::GoalStarted(g) => Some(g.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(goal_starts, vec!["a", "b", "c"]);

        let state_changes: Vec<(String, String)> = events
            .iter()
            .filter_map(|e| match e {
                WorkflowEvent::StateChange { from, to } => Some((from.clone(), to.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            state_changes,
            vec![
                ("a".to_string(), "b".to_string()),
                ("b".to_string(), "c".to_string())
            ]
        );

        assert!(matches!(
            events.last(),
            Some(WorkflowEvent::WorkflowComplete(Ok(_)))
        ));
    }

    /// A subagent's provisional transition (a→b --provisional) does NOT advance the workflow; the
    /// orchestrator's subsequent authoritative a→b commit does. Verifies the code-enforced go/no-go.
    #[allow(clippy::await_holding_lock)] // see note on the sibling test
    #[tokio::test]
    async fn subagent_provisional_transition_needs_orchestrator_commit() {
        let _guard = HANDLER_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // Given an agent scripted to make a provisional a->b (subagent) then an authoritative a->b
        let recipe = Arc::new(LinearTestRecipe);
        let mock = MockBackend::new();
        mock.push_ok_driving_transitions(
            "done",
            vec![
                ("b".to_string(), true),  // subagent: provisional, must not commit
                ("b".to_string(), false), // orchestrator: verified, commit
            ],
        );
        let backend = SharedBackend::from_arc(Arc::new(mock));

        // When running the agent session
        let (event_tx, event_rx) = mpsc::channel();
        run_agent_session(AgentSessionConfig {
            recipe,
            backend,
            event_tx,
            session_dir: None,
            session_id: Some("sess-2".to_string()),
            model: None,
            initial_prompt: "x".to_string(),
            working_dir: None,
            socket_path: None,
            conversation_output_path: None,
            debug: false,
            start_goal: None,
        })
        .await;

        // Then only ONE committed a->b StateChange results (the provisional one did not advance)
        let events: Vec<WorkflowEvent> = event_rx.try_iter().collect();
        let state_changes: Vec<(String, String)> = events
            .iter()
            .filter_map(|e| match e {
                WorkflowEvent::StateChange { from, to } => Some((from.clone(), to.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(state_changes, vec![("a".to_string(), "b".to_string())]);
    }
}
