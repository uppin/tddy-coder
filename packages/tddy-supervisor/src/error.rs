//! Errors returned across the supervisor's privileged boundary.

use std::fmt;

/// Failure of a call into the supervisor.
///
/// `Denied` deliberately carries no detail. A caller that is not allowed to spawn as `alice`
/// must not be able to learn from the error whether `alice` exists, so every authorization and
/// policy rejection collapses to the same opaque variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorError {
    /// The peer is not authorized, or the request names a user, path or mount root that policy
    /// does not permit.
    Denied,
    /// A declared service or an existing cgroup scope was addressed by a name that is not known.
    NotFound { name: String },
    /// The request was structurally valid but internally inconsistent (for example a cpu limit
    /// whose period does not match the policy ceiling's period).
    Invalid { message: String },
    /// The supervisor could not be reached, or the connection failed mid-request.
    Unavailable { message: String },
    /// The supervisor accepted the request but the privileged operation failed.
    OperationFailed { message: String },
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SupervisorError::Denied => write!(f, "request denied"),
            SupervisorError::NotFound { name } => write!(f, "no such service or scope: {name}"),
            SupervisorError::Invalid { message } => write!(f, "invalid request: {message}"),
            SupervisorError::Unavailable { message } => {
                write!(f, "supervisor unavailable: {message}")
            }
            SupervisorError::OperationFailed { message } => {
                write!(f, "privileged operation failed: {message}")
            }
        }
    }
}

impl std::error::Error for SupervisorError {}

/// Failure to load or validate the root-owned supervisor configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConfigError {}
