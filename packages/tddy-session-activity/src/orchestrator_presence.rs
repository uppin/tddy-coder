//! Whether a child's orchestrator is there to answer it (`#home-inbox` 6/8).
//!
//! A hook waits for an answer only while somebody can give one. "Somebody" is the orchestrating
//! session, and it is active when **this daemon serves its terminal** — not when a pid recorded for
//! it is alive, because a dead home whose pid was reused must never make a child wait
//! (`docs/dev/todo/2026-10-23-project-home-running-heuristic.md`).
//!
//! An orchestrator on another host is not active here: the changeset's
//! `orchestrator_daemon_instance_id` names another daemon, and asking that peer is a peer RPC this
//! node does not build. That check is the caller's, before it asks this port.
//!
//! A port because the terminal store is the daemon's (`tddy-session-lifecycle`).

use async_trait::async_trait;

/// Whether an orchestrating session is active on this daemon.
#[async_trait]
pub trait OrchestratorPresence: Send + Sync {
    /// True when this daemon serves `orchestrator_session_id`'s main terminal.
    async fn is_active(&self, orchestrator_session_id: &str) -> bool;
}

/// No orchestrator is ever active: every hook gets `NO_ORCHESTRATOR` at once, which is how a
/// daemon that wires no terminal store behaves — exactly as before this node.
pub struct NoOrchestratorIsActive;

#[async_trait]
impl OrchestratorPresence for NoOrchestratorIsActive {
    async fn is_active(&self, _orchestrator_session_id: &str) -> bool {
        false
    }
}
