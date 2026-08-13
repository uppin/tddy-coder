//! Client for the privileged surface.
//!
//! Lives in the supervisor crate rather than in `tddy-daemon` so that the supervisor's own
//! acceptance tests and the daemon exercise the same code against the same socket.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use prost::Message;
use tddy_rpc::{RpcClientTransport, RpcMessage, RpcResult, RpcService, Status};
use tddy_stdio::{StdioEndpoint, StdioRpcClient};
use tokio::task::JoinHandle;

use crate::error::SupervisorError;
use crate::proto::supervisor as wire;
use crate::protocol;
use crate::request::{
    CreateScopeRequest, ScopeHandle, SessionStatus, SpawnSandboxRequest, SpawnSessionRequest,
    SpawnedProcess,
};
use crate::server::SERVICE_NAME;
use crate::service::ServiceStatus;

/// A connection to a running supervisor's unix socket.
pub struct SupervisorClient {
    socket_path: PathBuf,
    transport: Arc<StdioRpcClient>,
    /// Drives the framed read/write loop for this connection. Aborted on drop so a short-lived
    /// client does not leave the connection open on the supervisor.
    endpoint: JoinHandle<()>,
}

impl std::fmt::Debug for SupervisorClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SupervisorClient")
            .field("socket_path", &self.socket_path)
            .finish_non_exhaustive()
    }
}

impl Drop for SupervisorClient {
    fn drop(&mut self) {
        self.endpoint.abort();
    }
}

impl SupervisorClient {
    /// Connect to the supervisor listening on `socket_path`.
    ///
    /// Fails with [`SupervisorError::Unavailable`] when the socket is absent or refuses the
    /// connection. There is no retry and no fallback: a caller that cannot reach the supervisor
    /// must surface that, never quietly do the work itself with less isolation.
    pub async fn connect(socket_path: &Path) -> Result<SupervisorClient, SupervisorError> {
        let stream = tokio::net::UnixStream::connect(socket_path)
            .await
            .map_err(|error| SupervisorError::Unavailable {
                message: format!("{}: {error}", socket_path.display()),
            })?;

        let (reader, writer) = stream.into_split();
        // The supervisor never calls back into its callers, so the service hosted for the inbound
        // direction refuses everything rather than pretending to offer a surface.
        let (transport, endpoint) = StdioEndpoint::from_duplex(reader, writer, NoInboundService);
        Ok(SupervisorClient {
            socket_path: socket_path.to_path_buf(),
            transport,
            endpoint: tokio::spawn(endpoint.run()),
        })
    }

    /// Socket this client is connected to.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Every declared service and its current state, in declaration order.
    pub async fn list_services(&self) -> Result<Vec<ServiceStatus>, SupervisorError> {
        let response: wire::ListServicesResponse = self
            .call("ListServices", wire::ListServicesRequest {})
            .await?;
        response
            .services
            .into_iter()
            .map(|status| {
                protocol::service_status_from_wire(status).map_err(protocol::error_from_status)
            })
            .collect()
    }

    /// State of one declared service.
    pub async fn service_status(&self, name: &str) -> Result<ServiceStatus, SupervisorError> {
        self.list_services()
            .await?
            .into_iter()
            .find(|status| status.name == name)
            .ok_or_else(|| SupervisorError::NotFound {
                name: name.to_string(),
            })
    }

    /// Start a declared service that is stopped or has been given up on.
    pub async fn start_service(&self, name: &str) -> Result<ServiceStatus, SupervisorError> {
        let status: wire::ServiceStatus = self
            .call(
                "StartService",
                wire::ServiceRef {
                    name: name.to_string(),
                },
            )
            .await?;
        protocol::service_status_from_wire(status).map_err(protocol::error_from_status)
    }

    /// Stop a declared service and suppress its restart policy.
    pub async fn stop_service(&self, name: &str) -> Result<ServiceStatus, SupervisorError> {
        let status: wire::ServiceStatus = self
            .call(
                "StopService",
                wire::ServiceRef {
                    name: name.to_string(),
                },
            )
            .await?;
        protocol::service_status_from_wire(status).map_err(protocol::error_from_status)
    }

    /// Spawn an allowlisted tool as an allowlisted OS user.
    pub async fn spawn_session(
        &self,
        request: SpawnSessionRequest,
    ) -> Result<SpawnedProcess, SupervisorError> {
        let spawned: wire::SpawnedProcess = self
            .call("SpawnSession", protocol::spawn_session_to_wire(request)?)
            .await?;
        Ok(protocol::spawned_process_from_wire(spawned))
    }

    /// Spawn an allowlisted tool as an allowlisted OS user, jailed.
    pub async fn spawn_sandbox(
        &self,
        request: SpawnSandboxRequest,
    ) -> Result<SpawnedProcess, SupervisorError> {
        let spawned: wire::SpawnedProcess = self
            .call("SpawnSandbox", protocol::spawn_sandbox_to_wire(request)?)
            .await?;
        Ok(protocol::spawned_process_from_wire(spawned))
    }

    /// Liveness and exit code of a session this supervisor spawned.
    ///
    /// A pid the supervisor did not spawn is refused rather than reported on: answering would turn
    /// the privileged surface into a way to probe arbitrary processes on the host.
    pub async fn session_status(&self, pid: u32) -> Result<SessionStatus, SupervisorError> {
        let status: wire::SessionStatus =
            self.call("SessionStatus", wire::SessionRef { pid }).await?;
        protocol::session_status_from_wire(status).map_err(protocol::error_from_status)
    }

    /// Stop a session: `SIGTERM` to its process group, then `SIGKILL` after the grace period.
    ///
    /// Returns the session's state as of the request, so a caller that stops an already-exited
    /// session learns that rather than being told the stop failed. The exit itself is observed by
    /// polling [`Self::session_status`].
    pub async fn stop_session(&self, pid: u32) -> Result<SessionStatus, SupervisorError> {
        let status: wire::SessionStatus =
            self.call("StopSession", wire::SessionRef { pid }).await?;
        protocol::session_status_from_wire(status).map_err(protocol::error_from_status)
    }

    /// Create a cgroup v2 scope with limits clamped to policy ceilings.
    pub async fn create_scope(
        &self,
        request: CreateScopeRequest,
    ) -> Result<ScopeHandle, SupervisorError> {
        let scope: wire::ScopeHandle = self
            .call("CreateScope", protocol::create_scope_to_wire(request))
            .await?;
        Ok(protocol::scope_handle_from_wire(scope))
    }

    /// Move an existing pid into a scope.
    pub async fn attach_pid(&self, scope: &str, pid: u32) -> Result<(), SupervisorError> {
        let _: wire::AttachPidResponse = self
            .call(
                "AttachPid",
                wire::AttachPidRequest {
                    scope: scope.to_string(),
                    pid,
                },
            )
            .await?;
        Ok(())
    }

    /// Remove a scope directory once its session has ended.
    pub async fn destroy_scope(&self, scope: &str) -> Result<(), SupervisorError> {
        let _: wire::DestroyScopeResponse = self
            .call(
                "DestroyScope",
                wire::ScopeRef {
                    name: scope.to_string(),
                },
            )
            .await?;
        Ok(())
    }

    async fn call<Req: Message, Res: Message + Default>(
        &self,
        method: &str,
        request: Req,
    ) -> Result<Res, SupervisorError> {
        let bytes = self
            .transport
            .call_unary(SERVICE_NAME, method, request.encode_to_vec())
            .await
            .map_err(protocol::error_from_status)?;
        Res::decode(&bytes[..]).map_err(|error| SupervisorError::OperationFailed {
            message: format!("could not decode the supervisor's reply to {method}: {error}"),
        })
    }
}

/// The inbound half of a client connection. The supervisor issues no requests to its callers, so
/// anything arriving here is a protocol error rather than something to dispatch.
struct NoInboundService;

#[async_trait]
impl RpcService for NoInboundService {
    async fn handle_rpc(&self, service: &str, method: &str, _message: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(Status::unimplemented(format!(
            "a supervisor client hosts no services (got {service}/{method})"
        ))))
    }
}
