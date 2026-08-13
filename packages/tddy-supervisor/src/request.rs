//! The wire requests and replies of the privileged surface.
//!
//! Every request names *what* the caller wants, never *how* to get it. There is no field for a
//! uid, a cgroup path, or a mount flag — those are resolved by the supervisor against its
//! root-owned policy, so a compromised daemon cannot widen its own grant by lying.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Spawn a tool as another OS user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnSessionRequest {
    /// Must appear in `SpawnPolicy::allowed_session_users`.
    pub os_user: String,
    /// Must appear in `SpawnPolicy::allowed_tool_paths`.
    pub tool_path: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    /// Name of an existing scope to place the child in before `exec`.
    #[serde(default)]
    pub scope: Option<String>,
}

/// A process the supervisor spawned on the caller's behalf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnedProcess {
    pub pid: u32,
}

/// Create a cgroup v2 scope inside the subtree the supervisor owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateScopeRequest {
    pub name: String,
    #[serde(default)]
    pub limits: RequestedLimits,
}

/// Limits a caller asks for. Each is clamped down to the matching `CgroupPolicy` ceiling; a
/// request can never raise a ceiling, only stay under it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestedLimits {
    #[serde(default)]
    pub memory_max: Option<u64>,
    /// `"<quota_us> <period_us>"`.
    #[serde(default)]
    pub cpu_max: Option<String>,
    #[serde(default)]
    pub pids_max: Option<u64>,
}

/// A scope that exists on the host, with the limits that were actually written to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeHandle {
    pub name: String,
    /// Absolute path of the scope directory.
    pub path: PathBuf,
    pub applied: AppliedLimits,
}

/// Limits as resolved against policy — what the kernel was told, not what was asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedLimits {
    pub memory_max: Option<u64>,
    pub cpu_max: Option<String>,
    pub pids_max: Option<u64>,
}

/// Spawn a jailed sandbox session as another OS user.
///
/// Distinct from [`SpawnSessionRequest`] in exactly two ways: the child is jailed, and it is placed
/// in a scope. Everything else — the allowlisted user, the allowlisted tool — is the same contract.
///
/// The session's control channel is **not** here. The caller passes the runner a `--grpc-uds` path
/// in `args` and dials it once the child is up, following the precedent set by
/// `SpawnRequest::host_session_socket`: a path crosses a process boundary, a file descriptor does
/// not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnSandboxRequest {
    /// Must appear in `SpawnPolicy::allowed_session_users`.
    pub os_user: String,
    /// Must appear in `SpawnPolicy::allowed_tool_paths`.
    pub tool_path: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    /// Existing scope the child joins while it is still privileged.
    #[serde(default)]
    pub scope: Option<String>,
    /// Bind mounts to establish inside the jail. Every source must fall under a policy-declared
    /// `allowed_mount_roots` entry.
    #[serde(default)]
    pub mounts: Vec<SandboxMount>,
    /// Give the jail its own network namespace with only loopback up.
    #[serde(default)]
    pub isolate_network: bool,
}

/// One bind mount a caller asks for inside a jail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxMount {
    pub source: PathBuf,
    pub target: PathBuf,
    #[serde(default)]
    pub readonly: bool,
}

/// Liveness of a session the supervisor spawned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Running,
    Exited,
}

/// What became of a session.
///
/// The supervisor reaps its own children, so it is the only process that can answer this — the
/// daemon cannot `waitpid` a process it did not fork. It retains the status of an exited session
/// until asked, because the caller's poll always arrives after the reap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStatus {
    pub pid: u32,
    pub state: SessionState,
    /// Set once the session has exited. `None` while it runs, and also when it was killed by a
    /// signal rather than exiting with a code.
    #[serde(default)]
    pub exit_code: Option<i32>,
}
