//! Adapter that serves this daemon's `worktree.WorktreeService` implementation over native tonic gRPC.
//!
//! The handler is written against the tddy-rpc flavor of the service trait
//! (`tddy_service::proto::worktree::WorktreeService`, using `tddy_rpc::{Request, Response, Status}`);
//! this adapter implements the tonic flavor
//! (`tddy_service::tonic_worktree::*_server::WorktreeService`) by delegating each method to it and converting the
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
use tddy_service::proto::worktree::WorktreeService as RpcService;
use tddy_service::proto::worktree::*;

/// Wraps a tddy-rpc `WorktreeService` implementation so it can be served over tonic gRPC.
pub struct WorktreeServiceTonicAdapter<T> {
    inner: Arc<T>,
}

impl<T> WorktreeServiceTonicAdapter<T> {
    pub fn new(inner: Arc<T>) -> Self {
        Self { inner }
    }
}

#[tonic::async_trait]
impl<T> tddy_service::tonic_worktree::worktree_service_server::WorktreeService
    for WorktreeServiceTonicAdapter<T>
where
    T: RpcService + Send + Sync + 'static,
    T::StreamWorktreeStatsStream: 'static,
    T::StreamReadWorktreeFileStream: 'static,
{
    async fn list_worktree_directory(
        &self,
        request: tonic::Request<ListWorktreeDirectoryRequest>,
    ) -> Result<tonic::Response<ListWorktreeDirectoryResponse>, tonic::Status> {
        let resp = RpcService::list_worktree_directory(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    async fn read_worktree_file(
        &self,
        request: tonic::Request<ReadWorktreeFileRequest>,
    ) -> Result<tonic::Response<ReadWorktreeFileResponse>, tonic::Status> {
        let resp = RpcService::read_worktree_file(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    async fn list_worktrees_for_project(
        &self,
        request: tonic::Request<ListWorktreesForProjectRequest>,
    ) -> Result<tonic::Response<ListWorktreesForProjectResponse>, tonic::Status> {
        let resp = RpcService::list_worktrees_for_project(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    async fn remove_worktree(
        &self,
        request: tonic::Request<RemoveWorktreeRequest>,
    ) -> Result<tonic::Response<RemoveWorktreeResponse>, tonic::Status> {
        let resp =
            RpcService::remove_worktree(&*self.inner, tddy_rpc::Request::new(request.into_inner()))
                .await
                .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    async fn clean_worktree(
        &self,
        request: tonic::Request<CleanWorktreeRequest>,
    ) -> Result<tonic::Response<CleanWorktreeResponse>, tonic::Status> {
        let resp =
            RpcService::clean_worktree(&*self.inner, tddy_rpc::Request::new(request.into_inner()))
                .await
                .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    async fn restore_session_worktree(
        &self,
        request: tonic::Request<RestoreSessionWorktreeRequest>,
    ) -> Result<tonic::Response<RestoreSessionWorktreeResponse>, tonic::Status> {
        let resp = RpcService::restore_session_worktree(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }

    /// Server streaming: byte-exact worktree file read (docs/ft/daemon/session-worktree-sync.md).
    type StreamReadWorktreeFileStream =
        Pin<Box<dyn Stream<Item = Result<WorktreeFileChunk, tonic::Status>> + Send>>;

    #[allow(clippy::result_large_err)]
    async fn stream_read_worktree_file(
        &self,
        request: tonic::Request<ReadWorktreeFileRequest>,
    ) -> Result<tonic::Response<Self::StreamReadWorktreeFileStream>, tonic::Status> {
        let resp = RpcService::stream_read_worktree_file(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        let outbound = resp.into_inner().map(|item| item.map_err(to_tonic_status));
        Ok(tonic::Response::new(Box::pin(outbound)))
    }

    /// Server streaming: per-worktree disk-size status (snapshot frame, then one row per change).
    type StreamWorktreeStatsStream =
        Pin<Box<dyn Stream<Item = Result<WorktreeStatsEvent, tonic::Status>> + Send>>;

    #[allow(clippy::result_large_err)]
    async fn stream_worktree_stats(
        &self,
        request: tonic::Request<StreamWorktreeStatsRequest>,
    ) -> Result<tonic::Response<Self::StreamWorktreeStatsStream>, tonic::Status> {
        let resp = RpcService::stream_worktree_stats(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        let outbound = resp.into_inner().map(|item| item.map_err(to_tonic_status));
        Ok(tonic::Response::new(Box::pin(outbound)))
    }

    async fn calculate_worktree_size(
        &self,
        request: tonic::Request<CalculateWorktreeSizeRequest>,
    ) -> Result<tonic::Response<CalculateWorktreeSizeResponse>, tonic::Status> {
        let resp = RpcService::calculate_worktree_size(
            &*self.inner,
            tddy_rpc::Request::new(request.into_inner()),
        )
        .await
        .map_err(to_tonic_status)?;
        Ok(tonic::Response::new(resp.into_inner()))
    }
}
