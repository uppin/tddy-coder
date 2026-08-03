//! Observable state of the services the supervisor manages.

use serde::{Deserialize, Serialize};

/// Lifecycle state of a managed service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceState {
    /// Forked and exec'd, not yet observed alive past the stability threshold.
    Starting,
    /// Running and stable; its restart backoff has been reset.
    Running,
    /// Exited and waiting out a backoff delay before the next restart attempt.
    Backoff,
    /// Exhausted `max_retries`; the supervisor will not restart it again.
    GaveUp,
    /// Stopped on request, and deliberately not restarted.
    Stopped,
}

/// A point-in-time snapshot of one managed service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    /// Absent while the service is in `Backoff`, `GaveUp` or `Stopped`.
    pub pid: Option<u32>,
    pub state: ServiceState,
    /// Restarts since the service last stayed up past its stability threshold.
    pub restarts: u32,
}
