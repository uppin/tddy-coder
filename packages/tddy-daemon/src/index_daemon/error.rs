//! What a caller gets instead of a running index daemon.

/// Why a caller did not get a usable index daemon.
///
/// Four classes rather than one string, because they mean different things to whoever asked: a
/// program that is not there is a deployment problem, a process that died is something to report
/// with its reason, a readiness deadline is a slow host, and a socket that will not answer is a
/// live process that cannot be talked to. None of them is a reason to carry on without an index
/// daemon — a configured one that cannot be started is an error, not a silent cold run.
#[derive(Debug)]
pub enum IndexDaemonError {
    /// The program could not be executed at all.
    NotStarted { detail: String },
    /// The process exited before it bound its socket. `reason` is the decoded exit plus the last
    /// thing it wrote to stderr.
    DiedBeforeReady { reason: String },
    /// The process was still running but had not bound its socket within the readiness budget.
    NotReadyInTime { seconds: u64 },
    /// The socket is there but would not answer.
    NotDialable { detail: String },
    /// This daemon is shutting down, so nothing new is started.
    Stopped,
}

impl std::fmt::Display for IndexDaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStarted { detail } => write!(f, "{detail}"),
            Self::DiedBeforeReady { reason } => {
                write!(f, "the index daemon died before it was ready: {reason}")
            }
            Self::NotReadyInTime { seconds } => write!(
                f,
                "the index daemon had not bound its socket after {seconds}s"
            ),
            Self::NotDialable { detail } => {
                write!(f, "the index daemon could not be dialled: {detail}")
            }
            Self::Stopped => write!(f, "this daemon is shutting down its index daemon"),
        }
    }
}

impl std::error::Error for IndexDaemonError {}
