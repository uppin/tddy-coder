//! The daemon's sandbox orchestration: starting a jailed session, provisioning the workspace tool
//! sandbox, and the plans that describe both.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 3. It has the **best test locality in that
//! crate** — 62 lines of inline `#[cfg(test)]` against 5,494 lines of dedicated integration suites
//! — which is the strongest possible position from which to move code, because the tests that prove
//! the behaviour move with it unrewritten.
//!
//! # Why this is a crate and `tddy-spawn` is another
//!
//! Discovery refuted the assumption that sandbox and spawn are one cluster. They share **no**
//! `crate::` edge — the sandbox modules never touch `spawner` — and their dependency sets are
//! disjoint: six `tddy-sandbox*` crates here against `tddy-supervisor` there. Confinement and
//! privileged fork are different concerns, and one crate would have forced every sandbox consumer to
//! link the supervisor.
//!
//! # What this move proves
//!
//! `tddy-sandbox-app` consumes `tddy_daemon::{sandbox_session, claude_cli_session, tool_engine}`
//! today. After this node it depends on **this** crate and not on `tddy-daemon` for the sandbox
//! path — a dependency **reversal**, and the node's most checkable outcome. It is asserted, not
//! described.
//!
//! The two symbols this subsystem reached the 23,099-line god module for —
//! [`tddy_daemon_kernel::AgentActivityHub`] and [`tddy_daemon_kernel::now_unix_ms`] — are node 1's,
//! and they are the whole reason this can leave.

use std::sync::Arc;

use tddy_daemon_kernel::AgentActivityHub;

/// A provisioned workspace sandbox a session's tool calls execute inside.
pub trait WorkspaceSandbox: Send + Sync {
    /// The path the sandbox confines execution to.
    fn root(&self) -> &std::path::Path;
}

/// Provisions a [`WorkspaceSandbox`] for a session, or explains why it cannot.
pub trait WorkspaceSandboxProvisioner: Send + Sync {
    /// Provision a sandbox for `session_id`.
    fn provision(&self, session_id: &str) -> Result<Arc<dyn WorkspaceSandbox>, SandboxError>;
}

/// Why a sandbox could not be provisioned or started.
///
/// `Unavailable` is distinct from `Refused` on purpose: a backend that is absent on this host is a
/// configuration fact an operator can act on, and one that refused a specific plan is a defect in
/// the plan. Collapsing them is how "sandboxing is off" gets misread as "your request was wrong".
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("no sandbox backend is available on this host")]
    Unavailable,
    #[error("the sandbox backend refused the plan: {reason}")]
    Refused { reason: String },
}

/// Jailed session lifecycle — start, bridge tool IPC, and reap.
pub struct SandboxSessionManager {
    // TODO(sandbox-spawn-services): implement
    _activity: Arc<AgentActivityHub>,
}

impl SandboxSessionManager {
    /// Build a manager that publishes activity into the shared hub.
    pub fn new(_activity: Arc<AgentActivityHub>) -> Self {
        // TODO(sandbox-spawn-services): implement
        unimplemented!("SandboxSessionManager::new")
    }

    /// Start a jailed session, returning once its tool-IPC bridge is serving.
    pub async fn start(&self, _session_id: &str) -> Result<(), SandboxError> {
        // TODO(sandbox-spawn-services): implement
        unimplemented!("SandboxSessionManager::start")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_an_absent_backend_apart_from_a_refused_plan() {
        // Given the two failures a caller must distinguish
        let absent = SandboxError::Unavailable;
        let refused = SandboxError::Refused {
            reason: "the plan named a path outside the workspace".to_string(),
        };

        // Then — an operator reading these must be able to tell whose problem it is
        assert!(absent.to_string().contains("no sandbox backend"));
        assert!(refused.to_string().contains("refused the plan"));
        assert_ne!(absent.to_string(), refused.to_string());
    }

    #[tokio::test]
    async fn publishes_a_started_session_into_the_shared_activity_hub() {
        // Given
        let hub = Arc::new(AgentActivityHub::default());
        let manager = SandboxSessionManager::new(Arc::clone(&hub));
        let mut listener = hub.subscribe("session-a");

        // When
        manager
            .start("session-a")
            .await
            .expect("the session starts");

        // Then
        assert!(
            listener.try_recv().is_ok(),
            "a started jailed session is observable on the hub the daemon shares"
        );
    }
}
