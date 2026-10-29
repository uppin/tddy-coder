//! Whether a child's orchestrator is there to answer it (`#home-inbox` 6/8, 10/11).
//!
//! A hook waits for an answer only while somebody can give one. "Somebody" is the orchestrating
//! session, and it is active when **the daemon it runs on serves its terminal** — not when a pid
//! recorded for it is alive, because a dead home whose pid was reused must never make a child wait
//! (`docs/dev/todo/2026-10-23-project-home-running-heuristic.md`).
//!
//! The orchestrator may run on another host: the child's changeset names it in
//! `orchestrator_daemon_instance_id`. The port is told which host to ask, and the daemon that wires
//! it answers a peer by asking that peer (`ActivityService.IsSessionTerminalServed`), bounded by
//! [`PEER_PRESENCE_TIMEOUT`] (`#home-inbox` 10/11).
//!
//! A port because the terminal store and the peer transport are the daemon's
//! (`tddy-session-lifecycle`).

use std::time::Duration;

use async_trait::async_trait;

/// How long a daemon waits for a peer to say whether it serves an orchestrator's terminal.
///
/// The hook is held on the child's host while it asks, so this is the longest the question can delay
/// Claude's own prompt. A peer that does not answer within it, or answers with an error, is absent.
/// Deliberately shorter than the peer-forward deadline (`CommonRoom::forward_timeout`), which is
/// sized for calls the peer spends real work on.
pub const PEER_PRESENCE_TIMEOUT: Duration = Duration::from_secs(3);

/// Whether an orchestrating session is active on the daemon that runs it.
#[async_trait]
pub trait OrchestratorPresence: Send + Sync {
    /// True when the daemon running `orchestrator_session_id` serves its main terminal.
    ///
    /// `orchestrator_daemon_instance_id` names that daemon when it is **not this one**; `None` is
    /// this daemon, which is answered from its own terminals. The caller has already folded "the
    /// changeset names this host" into `None`, so an implementation never compares ids to its own.
    async fn is_active(
        &self,
        orchestrator_session_id: &str,
        orchestrator_daemon_instance_id: Option<&str>,
    ) -> bool;
}

/// No orchestrator is ever active: every hook gets `NO_ORCHESTRATOR` at once, which is how a
/// daemon that wires no terminal store behaves — exactly as before this node.
pub struct NoOrchestratorIsActive;

#[async_trait]
impl OrchestratorPresence for NoOrchestratorIsActive {
    async fn is_active(
        &self,
        _orchestrator_session_id: &str,
        _orchestrator_daemon_instance_id: Option<&str>,
    ) -> bool {
        false
    }
}
