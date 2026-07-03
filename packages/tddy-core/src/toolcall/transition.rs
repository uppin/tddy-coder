//! `transition` relay: a [`TransitionHandler`] trait + process-global registry.
//!
//! The agent-driven orchestration model exposes a `transition` tool (see `tddy-tools`) that the
//! agent calls to advance the workflow state machine. The relay handles it **listener-local**
//! (like `list-actions` / `build`, not routed through the presenter loop): the listener looks up
//! the registered handler and calls it synchronously, returning the next-goal instructions (or a
//! rejection) to the agent on the wire.
//!
//! The handler is registered per session run by the runner that owns the
//! [`crate::workflow::controller::WorkflowController`]. Unlike [`super::build`]'s `OnceLock`
//! registry, this uses an `RwLock` so a new session can replace the previous handler.

use std::sync::{Arc, RwLock};

/// Outcome of a relayed `transition`, as seen by the agent.
#[derive(Debug, Clone)]
pub enum TransitionRelayOutcome {
    /// Authoritative (orchestrator) transition committed; `instructions` are the next goal's brief.
    Committed { instructions: String },
    /// Provisional (subagent) transition recorded; the orchestrator must verify and commit.
    Provisional { to: String },
    /// Transition refused (illegal edge, no-op, or persistence failure). `reason` is agent-facing.
    Rejected { reason: String },
}

/// Handles a `transition` relay request. Implemented by the workflow controller (via an adapter in
/// the runner) so `toolcall` stays decoupled from `workflow::controller` types.
pub trait TransitionHandler: Send + Sync {
    /// `provisional` is derived at the tool boundary from `parent_tool_use_id` (subagent calls).
    fn handle_transition(&self, to: &str, provisional: bool) -> TransitionRelayOutcome;
}

static REGISTERED: RwLock<Option<Arc<dyn TransitionHandler>>> = RwLock::new(None);

/// Register the process-wide transition handler for the active agent-driven session. Replaces any
/// previous handler (a new session supersedes the old one).
pub fn register_transition_handler(handler: Arc<dyn TransitionHandler>) {
    if let Ok(mut guard) = REGISTERED.write() {
        *guard = Some(handler);
    }
}

/// Clear the registered handler (session teardown / test isolation).
pub fn clear_transition_handler() {
    if let Ok(mut guard) = REGISTERED.write() {
        *guard = None;
    }
}

/// The registered handler, if any.
pub fn transition_handler() -> Option<Arc<dyn TransitionHandler>> {
    REGISTERED.read().ok().and_then(|g| g.clone())
}
