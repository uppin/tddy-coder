//! Conversion between the generated wire messages and the crate's public types.
//!
//! The wire form is protobuf so the surface is declared in one place (`supervisor.proto`) and both
//! ends are generated from it. Callers never see it: they hand in [`crate::request`] types and get
//! them back, so the protocol can change without touching a call site.
//!
//! Error mapping is the sensitive part. [`SupervisorError::Denied`] must survive the round trip as
//! itself and carry nothing else — a denial that arrived with a message would tell the caller
//! *which* gate refused it, which is exactly the existence oracle the boundary exists to close.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tddy_rpc::{Code, Status};

use crate::error::SupervisorError;
use crate::proto::supervisor as wire;
use crate::request::{
    AppliedLimits, CreateScopeRequest, RequestedLimits, SandboxMount, ScopeHandle, SessionState,
    SessionStatus, SpawnSandboxRequest, SpawnSessionRequest, SpawnedProcess,
};
use crate::service::{ServiceState, ServiceStatus};

/// The one message a denial is ever allowed to carry.
const DENIED_MESSAGE: &str = "request denied";

// -------------------------------------------------------------------------------------------
// Errors
// -------------------------------------------------------------------------------------------

/// Render a supervisor error on the wire.
pub fn status_from_error(error: SupervisorError) -> Status {
    match error {
        SupervisorError::Denied => Status::permission_denied(DENIED_MESSAGE),
        SupervisorError::NotFound { name } => Status::not_found(name),
        SupervisorError::Invalid { message } => Status::invalid_argument(message),
        SupervisorError::Unavailable { message } => Status {
            code: Code::Unavailable,
            message,
        },
        SupervisorError::OperationFailed { message } => Status::internal(message),
    }
}

/// Recover a supervisor error from a wire status.
///
/// A `PERMISSION_DENIED` collapses to [`SupervisorError::Denied`] and its message is dropped on
/// the floor, so no future server-side slip can leak detail through this path.
pub fn error_from_status(status: Status) -> SupervisorError {
    match status.code() {
        Code::PermissionDenied => SupervisorError::Denied,
        Code::NotFound => SupervisorError::NotFound {
            name: status.message,
        },
        Code::InvalidArgument => SupervisorError::Invalid {
            message: status.message,
        },
        Code::Unavailable => SupervisorError::Unavailable {
            message: status.message,
        },
        _ => SupervisorError::OperationFailed {
            message: status.message,
        },
    }
}

// -------------------------------------------------------------------------------------------
// Service status
// -------------------------------------------------------------------------------------------

pub fn service_status_to_wire(status: ServiceStatus) -> wire::ServiceStatus {
    wire::ServiceStatus {
        name: status.name,
        pid: status.pid,
        state: service_state_to_wire(status.state) as i32,
        restarts: status.restarts,
    }
}

pub fn service_status_from_wire(status: wire::ServiceStatus) -> Result<ServiceStatus, Status> {
    Ok(ServiceStatus {
        name: status.name,
        pid: status.pid,
        state: service_state_from_wire(status.state)?,
        restarts: status.restarts,
    })
}

fn service_state_to_wire(state: ServiceState) -> wire::ServiceState {
    match state {
        ServiceState::Starting => wire::ServiceState::Starting,
        ServiceState::Running => wire::ServiceState::Running,
        ServiceState::Backoff => wire::ServiceState::Backoff,
        ServiceState::GaveUp => wire::ServiceState::GaveUp,
        ServiceState::Stopped => wire::ServiceState::Stopped,
    }
}

fn service_state_from_wire(state: i32) -> Result<ServiceState, Status> {
    match wire::ServiceState::try_from(state) {
        Ok(wire::ServiceState::Starting) => Ok(ServiceState::Starting),
        Ok(wire::ServiceState::Running) => Ok(ServiceState::Running),
        Ok(wire::ServiceState::Backoff) => Ok(ServiceState::Backoff),
        Ok(wire::ServiceState::GaveUp) => Ok(ServiceState::GaveUp),
        Ok(wire::ServiceState::Stopped) => Ok(ServiceState::Stopped),
        // A peer that reports no state at all is a peer we cannot interpret; there is no sensible
        // default to assume for a lifecycle state.
        Ok(wire::ServiceState::Unspecified) | Err(_) => Err(Status::invalid_argument(format!(
            "unrecognized service state {state}"
        ))),
    }
}

// -------------------------------------------------------------------------------------------
// Session spawning
// -------------------------------------------------------------------------------------------

pub fn spawn_session_to_wire(
    request: SpawnSessionRequest,
) -> Result<wire::SpawnSessionRequest, SupervisorError> {
    Ok(wire::SpawnSessionRequest {
        os_user: request.os_user,
        tool_path: path_to_wire(&request.tool_path)?,
        args: request.args,
        env: request.env.into_iter().collect(),
        working_dir: request
            .working_dir
            .as_deref()
            .map(path_to_wire)
            .transpose()?,
        scope: request.scope,
    })
}

pub fn spawn_session_from_wire(request: wire::SpawnSessionRequest) -> SpawnSessionRequest {
    SpawnSessionRequest {
        os_user: request.os_user,
        tool_path: PathBuf::from(request.tool_path),
        args: request.args,
        env: request.env.into_iter().collect::<BTreeMap<_, _>>(),
        working_dir: request.working_dir.map(PathBuf::from),
        scope: request.scope,
    }
}

pub fn spawn_sandbox_to_wire(
    request: SpawnSandboxRequest,
) -> Result<wire::SpawnSandboxRequest, SupervisorError> {
    Ok(wire::SpawnSandboxRequest {
        os_user: request.os_user,
        tool_path: path_to_wire(&request.tool_path)?,
        args: request.args,
        env: request.env.into_iter().collect(),
        working_dir: request
            .working_dir
            .as_deref()
            .map(path_to_wire)
            .transpose()?,
        scope: request.scope,
        mounts: request
            .mounts
            .into_iter()
            .map(mount_to_wire)
            .collect::<Result<Vec<_>, _>>()?,
        isolate_network: request.isolate_network,
    })
}

pub fn spawn_sandbox_from_wire(request: wire::SpawnSandboxRequest) -> SpawnSandboxRequest {
    SpawnSandboxRequest {
        os_user: request.os_user,
        tool_path: PathBuf::from(request.tool_path),
        args: request.args,
        env: request.env.into_iter().collect::<BTreeMap<_, _>>(),
        working_dir: request.working_dir.map(PathBuf::from),
        scope: request.scope,
        mounts: request.mounts.into_iter().map(mount_from_wire).collect(),
        isolate_network: request.isolate_network,
    }
}

fn mount_to_wire(mount: SandboxMount) -> Result<wire::SandboxMount, SupervisorError> {
    Ok(wire::SandboxMount {
        source: path_to_wire(&mount.source)?,
        target: path_to_wire(&mount.target)?,
        readonly: mount.readonly,
    })
}

fn mount_from_wire(mount: wire::SandboxMount) -> SandboxMount {
    SandboxMount {
        source: PathBuf::from(mount.source),
        target: PathBuf::from(mount.target),
        readonly: mount.readonly,
    }
}

pub fn spawned_process_to_wire(spawned: SpawnedProcess) -> wire::SpawnedProcess {
    wire::SpawnedProcess { pid: spawned.pid }
}

pub fn spawned_process_from_wire(spawned: wire::SpawnedProcess) -> SpawnedProcess {
    SpawnedProcess { pid: spawned.pid }
}

// -------------------------------------------------------------------------------------------
// Session status
// -------------------------------------------------------------------------------------------

pub fn session_status_to_wire(status: SessionStatus) -> wire::SessionStatus {
    wire::SessionStatus {
        pid: status.pid,
        state: session_state_to_wire(status.state) as i32,
        exit_code: status.exit_code,
    }
}

pub fn session_status_from_wire(status: wire::SessionStatus) -> Result<SessionStatus, Status> {
    Ok(SessionStatus {
        pid: status.pid,
        state: session_state_from_wire(status.state)?,
        exit_code: status.exit_code,
    })
}

fn session_state_to_wire(state: SessionState) -> wire::SessionState {
    match state {
        SessionState::Running => wire::SessionState::Running,
        SessionState::Exited => wire::SessionState::Exited,
    }
}

fn session_state_from_wire(state: i32) -> Result<SessionState, Status> {
    match wire::SessionState::try_from(state) {
        Ok(wire::SessionState::Running) => Ok(SessionState::Running),
        Ok(wire::SessionState::Exited) => Ok(SessionState::Exited),
        // "Running" is not a safe thing to assume about a session whose state we could not read: a
        // caller would wait for an exit that has already happened.
        Ok(wire::SessionState::Unspecified) | Err(_) => Err(Status::invalid_argument(format!(
            "unrecognized session state {state}"
        ))),
    }
}

// -------------------------------------------------------------------------------------------
// Cgroup scopes
// -------------------------------------------------------------------------------------------

pub fn create_scope_to_wire(request: CreateScopeRequest) -> wire::CreateScopeRequest {
    wire::CreateScopeRequest {
        name: request.name,
        limits: Some(wire::RequestedLimits {
            memory_max: request.limits.memory_max,
            cpu_max: request.limits.cpu_max,
            pids_max: request.limits.pids_max,
        }),
    }
}

pub fn create_scope_from_wire(request: wire::CreateScopeRequest) -> CreateScopeRequest {
    let limits = request.limits.unwrap_or_default();
    CreateScopeRequest {
        name: request.name,
        limits: RequestedLimits {
            memory_max: limits.memory_max,
            cpu_max: limits.cpu_max,
            pids_max: limits.pids_max,
        },
    }
}

pub fn scope_handle_to_wire(scope: ScopeHandle) -> Result<wire::ScopeHandle, SupervisorError> {
    Ok(wire::ScopeHandle {
        name: scope.name,
        path: path_to_wire(&scope.path)?,
        applied: Some(wire::AppliedLimits {
            memory_max: scope.applied.memory_max,
            cpu_max: scope.applied.cpu_max,
            pids_max: scope.applied.pids_max,
        }),
    })
}

pub fn scope_handle_from_wire(scope: wire::ScopeHandle) -> ScopeHandle {
    let applied = scope.applied.unwrap_or_default();
    ScopeHandle {
        name: scope.name,
        path: PathBuf::from(scope.path),
        applied: AppliedLimits {
            memory_max: applied.memory_max,
            cpu_max: applied.cpu_max,
            pids_max: applied.pids_max,
        },
    }
}

/// Paths cross the boundary as UTF-8. A path that is not UTF-8 is refused rather than lossily
/// transcoded: a `?`-substituted byte would silently change which file is meant.
fn path_to_wire(path: &std::path::Path) -> Result<String, SupervisorError> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| SupervisorError::Invalid {
            message: format!("path {} is not valid utf-8", path.display()),
        })
}
