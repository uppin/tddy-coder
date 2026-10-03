//! `code_navigation.CodeNavigationService` — definition, references and hover for the web's code
//! pane, answered by the warm code-intelligence index.
//!
//! The service owns no index. Each request is authorised exactly as `worktree.WorktreeService`
//! authorises a file read ([`WorktreeServiceImpl::resolve_listed_worktree`]: token → OS user →
//! project main repo on this host → `git worktree list` membership), then forwarded to
//! `code_index.CodeIndexService` with the listed worktree as its workspace root, over the channel
//! [`IndexDaemonRegistry::connect`] dials — which starts the index daemon on the first request.
//!
//! Without an `index_daemon:` configuration section there is no registry, and every method answers
//! `FAILED_PRECONDITION` naming that section. There is deliberately no fallback to the agent-tool
//! language server: two indexes answering the same pane would disagree.

use std::sync::Arc;

use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::code_navigation::{
    CodeNavigationService, CodeNavigationServiceServer, DefinitionRequest, DefinitionResponse,
    HoverRequest, HoverResponse, ReferencesRequest, ReferencesResponse,
};
use tddy_worktree_service::WorktreeServiceImpl;

use crate::index_daemon::IndexDaemonRegistry;

/// The coordinate the web addresses this service at: `package code_navigation` +
/// `service CodeNavigationService` in `tddy-service/proto/code_navigation.proto`.
pub const CODE_NAVIGATION_SERVICE: &str = "code_navigation.CodeNavigationService";

/// `code_navigation.CodeNavigationService`, over this daemon's worktree authorisation and its
/// managed index daemon.
pub struct CodeNavigationServiceImpl {
    /// Whose `resolve_listed_worktree` gates every request, so this service can never reach a path
    /// the worktree service would refuse to read.
    #[allow(dead_code)]
    // TODO(code-navigation): read by the authorisation every method starts with.
    worktrees: Arc<WorktreeServiceImpl>,
    /// The index daemon this runtime manages; `None` when no `index_daemon:` section asked for one.
    #[allow(dead_code)] // TODO(code-navigation): dialled through `connect` by every forward.
    index_daemon: Option<IndexDaemonRegistry>,
}

impl CodeNavigationServiceImpl {
    /// A service authorising through `worktrees` and forwarding to `index_daemon`, when there is one.
    #[must_use]
    pub fn new(
        worktrees: Arc<WorktreeServiceImpl>,
        index_daemon: Option<IndexDaemonRegistry>,
    ) -> Self {
        Self {
            worktrees,
            index_daemon,
        }
    }
}

#[async_trait::async_trait]
impl CodeNavigationService for CodeNavigationServiceImpl {
    /// Where the symbol at a position is defined.
    async fn definition(
        &self,
        _request: Request<DefinitionRequest>,
    ) -> Result<Response<DefinitionResponse>, Status> {
        // TODO(code-navigation): authorise, require the registry, `connect`, forward
        // `code_index.Definition` with the listed worktree as `workspace_root`, map the answer.
        Err(Status::unimplemented(
            "Definition is not served yet — TODO(code-navigation)",
        ))
    }

    /// Every reference to the symbol at a position.
    async fn references(
        &self,
        _request: Request<ReferencesRequest>,
    ) -> Result<Response<ReferencesResponse>, Status> {
        // TODO(code-navigation): as `definition`, forwarding `code_index.References`.
        Err(Status::unimplemented(
            "References is not served yet — TODO(code-navigation)",
        ))
    }

    /// The hover text of the symbol at a position.
    async fn hover(
        &self,
        _request: Request<HoverRequest>,
    ) -> Result<Response<HoverResponse>, Status> {
        // TODO(code-navigation): as `definition`, forwarding `code_index.Hover`.
        Err(Status::unimplemented(
            "Hover is not served yet — TODO(code-navigation)",
        ))
    }
}

/// The `code_navigation.CodeNavigationService` entry the runtime registers.
#[must_use]
pub fn build_code_navigation_entry(service: CodeNavigationServiceImpl) -> tddy_rpc::ServiceEntry {
    let server = CodeNavigationServiceServer::from_arc(Arc::new(service));
    tddy_rpc::ServiceEntry {
        name: CODE_NAVIGATION_SERVICE,
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}
