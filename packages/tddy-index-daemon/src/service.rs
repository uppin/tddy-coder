//! The service implementation, and the entry a host registers it as.
//!
//! Every method is a routing table entry: it hands the decoded request to [`crate::operations`]
//! (the streaming restructure half), [`crate::queries`] (the unary restructure half) or
//! [`crate::analyze`] (everything about coverage and complexity) and does nothing else, so this
//! file stays readable as the list of what the coordinate answers to.

use std::sync::Arc;

use tddy_lsp::LspRegistry;
use tokio_stream::wrappers::ReceiverStream;

use crate::analyze;
use crate::index::WorkspaceIndex;
use crate::operations;
use crate::proto::code_index::{
    AnalyzeEvent, AnchorsRequest, AnchorsResponse, ApplyRequest, CheckRequest, CodeIndexService,
    CodeIndexServiceServer, ComplexityRequest, ComplexityResponse, CoverageRequest,
    DuplicateTestsRequest, IndexProgress, PlanStatusRequest, PlanStatusResponse, ReportRequest,
    ReportResponse, RestructureEvent, VerifyRequest, VerifyResponse, WarmRequest,
    WorkspacesRequest, WorkspacesResponse,
};
use crate::queries;
use crate::CODE_INDEX_SERVICE;

/// A stream of events a running operation reports as it goes.
///
/// A channel rather than a materialised list, because the point of streaming these operations is
/// that the caller hears about a seven-minute index while it is happening. It is also the
/// back-channel that tells the service its caller has gone: a send into a dropped receiver fails,
/// and that failure is the only disconnect signal a handler gets.
pub type EventStream<T> = ReceiverStream<Result<T, tddy_rpc::Status>>;

/// What the service needs from its host, as one struct because every field is wiring rather than
/// behaviour — the shape `tddy_terminal_rpc::TerminalSessionPorts` established.
pub struct CodeIndexPorts {
    /// Where the warm language servers live, keyed by `(workspace root, language)`. The host owns
    /// it so the same registry can be reaped on the host's own schedule and shared with anything
    /// else the process serves.
    pub servers: LspRegistry,
}

/// `code_index.CodeIndexService`, over a registry of warm language servers.
pub struct CodeIndexServiceImpl {
    index: WorkspaceIndex,
}

impl CodeIndexServiceImpl {
    #[must_use]
    pub fn new(ports: CodeIndexPorts) -> Self {
        Self {
            index: WorkspaceIndex::new(ports.servers),
        }
    }
}

#[async_trait::async_trait]
impl CodeIndexService for CodeIndexServiceImpl {
    type WarmStream = EventStream<IndexProgress>;
    type CheckStream = EventStream<RestructureEvent>;
    type ApplyStream = EventStream<RestructureEvent>;
    type CoverageStream = EventStream<AnalyzeEvent>;
    type DuplicateTestsStream = EventStream<AnalyzeEvent>;

    /// Load a workspace root's crate graph and report progress until it is ready.
    async fn warm(
        &self,
        request: tddy_rpc::Request<WarmRequest>,
    ) -> Result<tddy_rpc::Response<Self::WarmStream>, tddy_rpc::Status> {
        let request = request.into_inner();
        crate::warm::serve_warm(&self.index, &request.workspace_root)
            .await
            .map(tddy_rpc::Response::new)
    }

    /// Report every finding in a plan without writing anything.
    async fn check(
        &self,
        request: tddy_rpc::Request<CheckRequest>,
    ) -> Result<tddy_rpc::Response<Self::CheckStream>, tddy_rpc::Status> {
        operations::serve_check(&self.index, request.into_inner())
            .await
            .map(tddy_rpc::Response::new)
    }

    /// Execute a plan against the working tree.
    async fn apply(
        &self,
        request: tddy_rpc::Request<ApplyRequest>,
    ) -> Result<tddy_rpc::Response<Self::ApplyStream>, tddy_rpc::Status> {
        operations::serve_apply(&self.index, request.into_inner())
            .await
            .map(tddy_rpc::Response::new)
    }

    /// Emit the range anchor covering a named run of items.
    async fn anchors(
        &self,
        request: tddy_rpc::Request<AnchorsRequest>,
    ) -> Result<tddy_rpc::Response<AnchorsResponse>, tddy_rpc::Status> {
        queries::serve_anchors(&self.index, request.into_inner())
            .await
            .map(tddy_rpc::Response::new)
    }

    /// Report how far a plan's journal got.
    async fn plan_status(
        &self,
        request: tddy_rpc::Request<PlanStatusRequest>,
    ) -> Result<tddy_rpc::Response<PlanStatusResponse>, tddy_rpc::Status> {
        queries::serve_plan_status(&self.index, request.into_inner())
            .await
            .map(tddy_rpc::Response::new)
    }

    /// Compare the tree's statement multiset against a git ref.
    async fn verify(
        &self,
        request: tddy_rpc::Request<VerifyRequest>,
    ) -> Result<tddy_rpc::Response<VerifyResponse>, tddy_rpc::Status> {
        queries::serve_verify(&self.index, request.into_inner())
            .await
            .map(tddy_rpc::Response::new)
    }

    /// Build the instrumented tests and capture a coverage profile per test.
    async fn coverage(
        &self,
        request: tddy_rpc::Request<CoverageRequest>,
    ) -> Result<tddy_rpc::Response<Self::CoverageStream>, tddy_rpc::Status> {
        analyze::serve_coverage(&self.index, request.into_inner())
            .await
            .map(tddy_rpc::Response::new)
    }

    /// Join a capture against the complexity of the tree it measured.
    async fn report(
        &self,
        request: tddy_rpc::Request<ReportRequest>,
    ) -> Result<tddy_rpc::Response<ReportResponse>, tddy_rpc::Status> {
        analyze::serve_report(&self.index, request.into_inner())
            .await
            .map(tddy_rpc::Response::new)
    }

    /// Report tests whose coverage signatures are identical or contained in another's.
    async fn duplicate_tests(
        &self,
        request: tddy_rpc::Request<DuplicateTestsRequest>,
    ) -> Result<tddy_rpc::Response<Self::DuplicateTestsStream>, tddy_rpc::Status> {
        analyze::serve_duplicate_tests(&self.index, request.into_inner())
            .await
            .map(tddy_rpc::Response::new)
    }

    /// Score every function in one source file for cyclomatic complexity.
    async fn complexity(
        &self,
        request: tddy_rpc::Request<ComplexityRequest>,
    ) -> Result<tddy_rpc::Response<ComplexityResponse>, tddy_rpc::Status> {
        analyze::serve_complexity(&self.index, request.into_inner())
            .await
            .map(tddy_rpc::Response::new)
    }

    /// Report which workspace roots this process holds an index for.
    async fn workspaces(
        &self,
        _request: tddy_rpc::Request<WorkspacesRequest>,
    ) -> Result<tddy_rpc::Response<WorkspacesResponse>, tddy_rpc::Status> {
        Ok(tddy_rpc::Response::new(
            operations::serve_workspaces(&self.index).await,
        ))
    }
}

/// The `code_index.CodeIndexService` entry a host's wiring layer registers.
///
/// Takes the whole [`CodeIndexPorts`] because every field is wiring. Built through `from_arc` so
/// the same implementation instance can be served over more than one transport at once — one warm
/// index behind both a gRPC listener and this process's stdio.
#[must_use]
pub fn build_code_index_entry(ports: CodeIndexPorts) -> tddy_rpc::ServiceEntry {
    let server = CodeIndexServiceServer::from_arc(Arc::new(CodeIndexServiceImpl::new(ports)));
    tddy_rpc::ServiceEntry {
        name: CODE_INDEX_SERVICE,
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}
