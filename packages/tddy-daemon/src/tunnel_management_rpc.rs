//! Connect `/rpc` surface for managed TCP tunnel advertisements and control.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, ServiceEntry, Status};
use tddy_service::proto::tunnel_management::{
    ListTunnelAdvertisementsRequest, ListTunnelAdvertisementsResponse, OpenBrowserForTunnelRequest,
    OpenBrowserForTunnelResponse, StartTunnelRequest, StartTunnelResponse, StopTunnelRequest,
    StopTunnelResponse, TunnelAdvertisement, TunnelManagementService,
};
use tddy_service::TunnelManagementServiceServer;

use crate::tunnel_supervisor::TunnelSupervisor;

const LOG: &str = "tddy_daemon::tunnel_management_rpc";

/// Connect handlers backed by [`TunnelSupervisor`].
pub struct TunnelManagementServiceImpl {
    supervisor: Arc<TunnelSupervisor>,
}

impl TunnelManagementServiceImpl {
    pub fn new(supervisor: Arc<TunnelSupervisor>) -> Self {
        Self { supervisor }
    }

    fn snapshot_to_proto(
        s: &crate::tunnel_supervisor::TunnelAdvertisementSnapshot,
    ) -> TunnelAdvertisement {
        TunnelAdvertisement {
            operator_loopback_port: s.operator_loopback_port,
            session_correlation_id: s.session_correlation_id.clone(),
            kind: s.kind,
            state: s.state,
            authorize_url: s.authorize_url.clone(),
        }
    }
}

#[async_trait]
impl TunnelManagementService for TunnelManagementServiceImpl {
    async fn list_tunnel_advertisements(
        &self,
        _request: Request<ListTunnelAdvertisementsRequest>,
    ) -> Result<Response<ListTunnelAdvertisementsResponse>, Status> {
        log::debug!(target: LOG, "list_tunnel_advertisements");
        let ads = self.supervisor.snapshot_advertisements();
        log::info!(
            target: LOG,
            "list_tunnel_advertisements returning count={}",
            ads.len()
        );
        let advertisements = ads.iter().map(Self::snapshot_to_proto).collect::<Vec<_>>();
        Ok(Response::new(ListTunnelAdvertisementsResponse {
            advertisements,
        }))
    }

    async fn start_tunnel(
        &self,
        request: Request<StartTunnelRequest>,
    ) -> Result<Response<StartTunnelResponse>, Status> {
        let r = request.into_inner();
        log::debug!(
            target: LOG,
            "start_tunnel session_correlation_id_len={} port={}",
            r.session_correlation_id.len(),
            r.operator_bind_port
        );
        self.supervisor
            .validate_operator_bind_port(r.operator_bind_port)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.supervisor.upsert_active_binding(
            &r.session_correlation_id,
            r.operator_bind_port,
            r.kind,
        );
        let row = self
            .supervisor
            .snapshot_advertisements()
            .into_iter()
            .find(|x| x.session_correlation_id == r.session_correlation_id)
            .ok_or_else(|| Status::internal("start_tunnel: row missing after upsert"))?;
        log::info!(
            target: LOG,
            "start_tunnel ok session_correlation_id_len={}",
            r.session_correlation_id.len()
        );
        Ok(Response::new(StartTunnelResponse {
            advertisement: Some(Self::snapshot_to_proto(&row)),
        }))
    }

    async fn stop_tunnel(
        &self,
        request: Request<StopTunnelRequest>,
    ) -> Result<Response<StopTunnelResponse>, Status> {
        let r = request.into_inner();
        log::debug!(
            target: LOG,
            "stop_tunnel session_correlation_id_len={}",
            r.session_correlation_id.len()
        );
        let state = self.supervisor.stop_binding(&r.session_correlation_id);
        log::info!(
            target: LOG,
            "stop_tunnel done session_correlation_id_len={} state={}",
            r.session_correlation_id.len(),
            state
        );
        Ok(Response::new(StopTunnelResponse { state }))
    }

    async fn open_browser_for_tunnel(
        &self,
        request: Request<OpenBrowserForTunnelRequest>,
    ) -> Result<Response<OpenBrowserForTunnelResponse>, Status> {
        let r = request.into_inner();
        log::info!(
            target: LOG,
            "open_browser_for_tunnel session_correlation_id_len={} url_len={}",
            r.session_correlation_id.len(),
            r.url.len()
        );
        log::debug!(target: LOG, "open_browser_for_tunnel acknowledged (client/delegate opens browser)");
        Ok(Response::new(OpenBrowserForTunnelResponse {}))
    }
}

/// [`ServiceEntry`] for tests or embedding with a specific supervisor instance.
pub fn tunnel_management_rpc_entry_with_supervisor(
    supervisor: Arc<TunnelSupervisor>,
) -> ServiceEntry {
    let server = TunnelManagementServiceServer::new(TunnelManagementServiceImpl::new(supervisor));
    ServiceEntry {
        name: TunnelManagementServiceServer::<TunnelManagementServiceImpl>::NAME,
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}

/// Isolated entry with an empty in-memory supervisor (each call gets a fresh supervisor).
pub fn tunnel_management_rpc_entry() -> ServiceEntry {
    tunnel_management_rpc_entry_with_supervisor(Arc::new(TunnelSupervisor::new()))
}
