//! Adapter that serves this daemon's `host.HostService` implementation over native tonic gRPC.
//!
//! The handler is written against the tddy-rpc flavor of the service trait
//! (`tddy_service::proto::host::HostService`, using `tddy_rpc::{Request, Response, Status}`);
//! this adapter implements the tonic flavor
//! (`tddy_service::tonic_host::*_server::HostService`) by delegating each method to it and converting the
//! wrapper types at the boundary. Both flavors reference the identical canonical prost message
//! types via `extern_path`, so nothing is re-encoded — only the transport wrappers and the error
//! `Status`.
//!
//! **Hand-written, and recorded as debt.** `tddy-codegen`'s `generate_tonic_adapter` is a stub, so
//! the two adapters `#unbundle` node 1 needs are spelled out. Each method is a literal `async fn`
//! rather than a macro expansion for the same reason the connection adapter's are:
//! `#[tonic::async_trait]` rewrites method signatures and cannot see through a macro invocation.
//! See the changeset's `## Technical Debt & Production Readiness`.
//!
//! This is plumbing only: constructing the adapter does not start a server.

use std::pin::Pin;
use std::sync::Arc;

use futures_util::{Stream, StreamExt};

use crate::connection_tonic_adapter::to_tonic_status;
use tddy_service::proto::host::HostService as RpcService;
use tddy_service::proto::host::*;

/// Wraps a tddy-rpc `HostService` implementation so it can be served over tonic gRPC.
pub struct HostServiceTonicAdapter<T> {
    inner: Arc<T>,
}

impl<T> HostServiceTonicAdapter<T> {
    pub fn new(inner: Arc<T>) -> Self {
        Self { inner }
    }
}

#[tonic::async_trait]
impl<T> tddy_service::tonic_host::host_service_server::HostService for HostServiceTonicAdapter<T>
where
    T: RpcService + Send + Sync + 'static,
    T::StreamHostPromptsStream: 'static,
    T::StreamHostStatsStream: 'static,
{
    async fn list_eligible_daemons(
        &self,
        request: tonic::Request<ListEligibleDaemonsRequest>,
    ) -> Result<tonic::Response<ListEligibleDaemonsResponse>, tonic::Status> {
        let resp = RpcService::list_eligible_daemons(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    async fn list_known_hosts(
        &self,
        request: tonic::Request<ListKnownHostsRequest>,
    ) -> Result<tonic::Response<ListKnownHostsResponse>, tonic::Status> {
        let resp = RpcService::list_known_hosts(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    async fn get_host_tooling(
        &self,
        request: tonic::Request<GetHostToolingRequest>,
    ) -> Result<tonic::Response<GetHostToolingResponse>, tonic::Status> {
        let resp = RpcService::get_host_tooling(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    /// Server streaming: questions this host is waiting on an operator to answer. Silent by nature.
    type StreamHostPromptsStream =
        Pin<Box<dyn Stream<Item = Result<HostPromptEvent, tonic::Status>> + Send>>;

    #[allow(clippy::result_large_err)]
    async fn stream_host_prompts(
        &self,
        request: tonic::Request<StreamHostPromptsRequest>,
    ) -> Result<tonic::Response<Self::StreamHostPromptsStream>, tonic::Status> {
        let resp = RpcService::stream_host_prompts(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        let outbound = resp.into_inner().map(|item| item.map_err(to_tonic_status));
        Ok(tonic::Response::new(Box::pin(outbound)))
    }

    async fn answer_host_prompt(
        &self,
        request: tonic::Request<AnswerHostPromptRequest>,
    ) -> Result<tonic::Response<AnswerHostPromptResponse>, tonic::Status> {
        let resp = RpcService::answer_host_prompt(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    async fn add_host_key(
        &self,
        request: tonic::Request<AddHostKeyRequest>,
    ) -> Result<tonic::Response<AddHostKeyResponse>, tonic::Status> {
        let resp =
            RpcService::add_host_key(&*self.inner, tddy_rpc::Request::new(request.into_inner()))
                .await
                .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    async fn list_host_key_candidates(
        &self,
        request: tonic::Request<ListHostKeyCandidatesRequest>,
    ) -> Result<tonic::Response<ListHostKeyCandidatesResponse>, tonic::Status> {
        let resp = RpcService::list_host_key_candidates(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    /// Server streaming: host telemetry (immediate emit, then server-owned CPU/disk cadence).
    type StreamHostStatsStream =
        Pin<Box<dyn Stream<Item = Result<HostStatsEvent, tonic::Status>> + Send>>;

    #[allow(clippy::result_large_err)]
    async fn stream_host_stats(
        &self,
        request: tonic::Request<StreamHostStatsRequest>,
    ) -> Result<tonic::Response<Self::StreamHostStatsStream>, tonic::Status> {
        let resp = RpcService::stream_host_stats(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        let outbound = resp.into_inner().map(|item| item.map_err(to_tonic_status));
        Ok(tonic::Response::new(Box::pin(outbound)))
    }
}
