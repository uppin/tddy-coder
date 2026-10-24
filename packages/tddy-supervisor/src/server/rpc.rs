use crate::authz::PeerIdentity;
use crate::clone::CloneEvents;
use crate::proto::supervisor::{self as wire, SupervisorService};
use crate::protocol;
use crate::server::PrivilegedSurface;
use async_trait::async_trait;
use std::sync::Arc;
use tddy_rpc::{Request, Response, Status};

/// One connection's view of the surface: the surface itself, plus the peer that opened it.
///
/// Peer credentials are read once, at accept time, and belong to the connection — not to a request
/// field a caller could set.
pub struct SupervisorServiceImpl {
    surface: Arc<PrivilegedSurface>,
    peer: PeerIdentity,
}

impl SupervisorServiceImpl {
    pub fn new(surface: Arc<PrivilegedSurface>, peer: PeerIdentity) -> SupervisorServiceImpl {
        SupervisorServiceImpl { surface, peer }
    }

    /// The first gate. Runs before any request field is looked at, for every method.
    fn authorize_peer(&self) -> Result<(), Status> {
        self.surface
            .authorizer
            .authorize(&self.peer)
            .map_err(|denied| {
                log::warn!(
                    target: "tddy_supervisor::server",
                    "denied a request from uid {} pid {}",
                    self.peer.uid,
                    self.peer.pid
                );
                protocol::status_from_error(denied)
            })
    }
}

#[async_trait]
impl SupervisorService for SupervisorServiceImpl {
    async fn list_services(
        &self,
        _request: Request<wire::ListServicesRequest>,
    ) -> Result<Response<wire::ListServicesResponse>, Status> {
        self.authorize_peer()?;
        let services = self
            .surface
            .supervisor
            .statuses()
            .into_iter()
            .map(protocol::service_status_to_wire)
            .collect();
        Ok(Response::new(wire::ListServicesResponse { services }))
    }

    async fn start_service(
        &self,
        request: Request<wire::ServiceRef>,
    ) -> Result<Response<wire::ServiceStatus>, Status> {
        self.authorize_peer()?;
        let status = self
            .surface
            .supervisor
            .start_by_name(&request.into_inner().name)
            .await
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::service_status_to_wire(status)))
    }

    async fn stop_service(
        &self,
        request: Request<wire::ServiceRef>,
    ) -> Result<Response<wire::ServiceStatus>, Status> {
        self.authorize_peer()?;
        let status = self
            .surface
            .supervisor
            .stop_by_name(&request.into_inner().name)
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::service_status_to_wire(status)))
    }

    async fn spawn_session(
        &self,
        request: Request<wire::SpawnSessionRequest>,
    ) -> Result<Response<wire::SpawnedProcess>, Status> {
        self.authorize_peer()?;
        let spawned = self
            .surface
            .spawn_session(protocol::spawn_session_from_wire(request.into_inner()))
            .await
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::spawned_process_to_wire(spawned)))
    }

    async fn spawn_relayed(
        &self,
        request: Request<wire::SpawnRelayedRequest>,
    ) -> Result<Response<wire::SpawnedProcess>, Status> {
        self.authorize_peer()?;
        let spawned = self
            .surface
            .spawn_relayed(
                protocol::spawn_relayed_from_wire(request.into_inner()),
                self.peer.uid,
            )
            .await
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::spawned_process_to_wire(spawned)))
    }

    async fn spawn_sandbox(
        &self,
        request: Request<wire::SpawnSandboxRequest>,
    ) -> Result<Response<wire::SpawnedProcess>, Status> {
        self.authorize_peer()?;
        let spawned = self
            .surface
            .spawn_sandbox(protocol::spawn_sandbox_from_wire(request.into_inner()))
            .await
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::spawned_process_to_wire(spawned)))
    }

    async fn session_status(
        &self,
        request: Request<wire::SessionRef>,
    ) -> Result<Response<wire::SessionStatus>, Status> {
        self.authorize_peer()?;
        let status = self
            .surface
            .supervisor
            .session_status(request.into_inner().pid)
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::session_status_to_wire(status)))
    }

    type CloneRepositoryStream = CloneEvents;

    async fn clone_repository(
        &self,
        request: Request<wire::CloneRepositoryRequest>,
    ) -> Result<Response<Self::CloneRepositoryStream>, Status> {
        self.authorize_peer()?;
        let events = self
            .surface
            .clone_repository(request.into_inner())
            .await
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(events))
    }

    async fn stop_session(
        &self,
        request: Request<wire::SessionRef>,
    ) -> Result<Response<wire::SessionStatus>, Status> {
        self.authorize_peer()?;
        let status = self
            .surface
            .stop_session(request.into_inner().pid)
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::session_status_to_wire(status)))
    }

    async fn create_scope(
        &self,
        request: Request<wire::CreateScopeRequest>,
    ) -> Result<Response<wire::ScopeHandle>, Status> {
        self.authorize_peer()?;
        let scope = self
            .surface
            .create_scope(protocol::create_scope_from_wire(request.into_inner()))
            .map_err(protocol::status_from_error)?;
        let scope = protocol::scope_handle_to_wire(scope).map_err(protocol::status_from_error)?;
        Ok(Response::new(scope))
    }

    async fn attach_pid(
        &self,
        request: Request<wire::AttachPidRequest>,
    ) -> Result<Response<wire::AttachPidResponse>, Status> {
        self.authorize_peer()?;
        let request = request.into_inner();
        self.surface
            .attach_pid(&request.scope, request.pid)
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(wire::AttachPidResponse {}))
    }

    async fn destroy_scope(
        &self,
        request: Request<wire::ScopeRef>,
    ) -> Result<Response<wire::DestroyScopeResponse>, Status> {
        self.authorize_peer()?;
        self.surface
            .cgroups
            .destroy_scope(&request.into_inner().name)
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(wire::DestroyScopeResponse {}))
    }
}
