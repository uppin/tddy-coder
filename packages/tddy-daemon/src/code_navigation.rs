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

use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::code_navigation::{
    CodeIndexProgress, CodeLocation, CodeNavigationService, CodeNavigationServiceServer,
    DefinitionRequest, DefinitionResponse, HoverRequest, HoverResponse, ReferencesRequest,
    ReferencesResponse, SourcePosition, SourceRange, WatchCodeIndexRequest,
};
use tddy_worktree_service::worktree_files::validate_rel_path_shape;
use tddy_worktree_service::WorktreeServiceImpl;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;

use crate::code_index_warmup::SessionIndexProgress;
use crate::index_daemon::IndexDaemonRegistry;

/// The coordinate the web addresses this service at: `package code_navigation` +
/// `service CodeNavigationService` in `tddy-service/proto/code_navigation.proto`.
pub const CODE_NAVIGATION_SERVICE: &str = "code_navigation.CodeNavigationService";

/// `code_navigation.CodeNavigationService`, over this daemon's worktree authorisation and its
/// managed index daemon.
pub struct CodeNavigationServiceImpl {
    /// Whose `resolve_listed_worktree` gates every request, so this service can never reach a path
    /// the worktree service would refuse to read.
    worktrees: Arc<WorktreeServiceImpl>,
    /// The index daemon this runtime manages; `None` when no `index_daemon:` section asked for one.
    index_daemon: Option<IndexDaemonRegistry>,
    /// Each session's latest code-index warm-up progress, which `WatchCodeIndex` follows.
    #[allow(dead_code)] // TODO(indexing-indicators): read by `watch_code_index`.
    index_progress: SessionIndexProgress,
}

/// An authorised request, ready to be asked of the index: a client on the index daemon's channel
/// and the question in the index's own terms.
struct Forward {
    client: CodeIndexServiceClient<Channel>,
    workspace_root: String,
    file: String,
    position: Option<index::SourcePosition>,
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
            index_progress: SessionIndexProgress::new(),
        }
    }

    /// The one path every method takes: authorise the worktree as `WorktreeService` does, refuse a
    /// `rel_path` that leaves it, require the index daemon, and dial it — which starts it on the
    /// first request. Nothing is started for a request that fails an earlier step.
    async fn authorise_and_connect(
        &self,
        session_token: &str,
        project_id: &str,
        worktree_path: &str,
        rel_path: &str,
        position: Option<SourcePosition>,
    ) -> Result<Forward, Status> {
        let root =
            self.worktrees
                .resolve_listed_worktree(session_token, project_id, worktree_path)?;
        let file = validate_rel_path_shape(rel_path)?;
        let registry = self.index_daemon.as_ref().ok_or_else(|| {
            Status::failed_precondition(
                "code navigation needs the index daemon, and this daemon has no `index_daemon:` \
                 configuration section",
            )
        })?;
        let channel = registry
            .connect()
            .await
            .map_err(|err| Status::unavailable(format!("index daemon: {err}")))?;
        Ok(Forward {
            client: CodeIndexServiceClient::new(channel),
            workspace_root: root.display().to_string(),
            file,
            position: position.map(index_position),
        })
    }

    /// The same service, reporting warm-up progress from `index_progress` — the holder
    /// [`crate::code_index_warmup::warm_for_session`] records into.
    #[must_use]
    pub fn with_index_progress(mut self, index_progress: SessionIndexProgress) -> Self {
        self.index_progress = index_progress;
        self
    }
}

#[async_trait::async_trait]
impl CodeNavigationService for CodeNavigationServiceImpl {
    type WatchCodeIndexStream = ReceiverStream<Result<CodeIndexProgress, Status>>;

    /// Where the symbol at a position is defined.
    async fn definition(
        &self,
        request: Request<DefinitionRequest>,
    ) -> Result<Response<DefinitionResponse>, Status> {
        let r = request.into_inner();
        let mut forward = self
            .authorise_and_connect(
                &r.session_token,
                &r.project_id,
                &r.worktree_path,
                &r.rel_path,
                r.position,
            )
            .await?;
        let answer = forward
            .client
            .definition(index::DefinitionRequest {
                workspace_root: forward.workspace_root,
                file: forward.file,
                position: forward.position,
            })
            .await
            .map_err(tddy_service::to_rpc_status)?
            .into_inner();
        Ok(Response::new(DefinitionResponse {
            locations: answer.locations.into_iter().map(web_location).collect(),
        }))
    }

    /// Every reference to the symbol at a position.
    async fn references(
        &self,
        request: Request<ReferencesRequest>,
    ) -> Result<Response<ReferencesResponse>, Status> {
        let r = request.into_inner();
        let mut forward = self
            .authorise_and_connect(
                &r.session_token,
                &r.project_id,
                &r.worktree_path,
                &r.rel_path,
                r.position,
            )
            .await?;
        let answer = forward
            .client
            .references(index::ReferencesRequest {
                workspace_root: forward.workspace_root,
                file: forward.file,
                position: forward.position,
            })
            .await
            .map_err(tddy_service::to_rpc_status)?
            .into_inner();
        Ok(Response::new(ReferencesResponse {
            locations: answer.locations.into_iter().map(web_location).collect(),
        }))
    }

    /// The hover text of the symbol at a position.
    async fn hover(
        &self,
        request: Request<HoverRequest>,
    ) -> Result<Response<HoverResponse>, Status> {
        let r = request.into_inner();
        let mut forward = self
            .authorise_and_connect(
                &r.session_token,
                &r.project_id,
                &r.worktree_path,
                &r.rel_path,
                r.position,
            )
            .await?;
        let answer = forward
            .client
            .hover(index::HoverRequest {
                workspace_root: forward.workspace_root,
                file: forward.file,
                position: forward.position,
            })
            .await
            .map_err(tddy_service::to_rpc_status)?
            .into_inner();
        Ok(Response::new(HoverResponse {
            markdown: answer.markdown,
        }))
    }

    /// A session's code-index warm-up: its latest progress, then each change, ending after `ready`
    /// or `error` — or at once, for a session nothing warmed.
    async fn watch_code_index(
        &self,
        _request: Request<WatchCodeIndexRequest>,
    ) -> Result<Response<Self::WatchCodeIndexStream>, Status> {
        // TODO(indexing-indicators): authorise the token against the session's owner, follow
        // `index_progress.watch(session_id)` onto the stream, and end it after `ready` or `error`.
        Err(Status::unimplemented(
            "WatchCodeIndex is not served yet — TODO(indexing-indicators)",
        ))
    }
}

fn index_position(position: SourcePosition) -> index::SourcePosition {
    index::SourcePosition {
        line: position.line,
        column: position.column,
    }
}

fn web_position(position: index::SourcePosition) -> SourcePosition {
    SourcePosition {
        line: position.line,
        column: position.column,
    }
}

/// The index's location in the web's vocabulary: `file` becomes `rel_path` and `outside_root`
/// becomes `outside_worktree`; the path is already relative to the root when inside it and
/// absolute otherwise, which is what `rel_path` is documented to hold.
fn web_location(location: index::CodeLocation) -> CodeLocation {
    CodeLocation {
        rel_path: location.file,
        range: location.range.map(|range| SourceRange {
            start: range.start.map(web_position),
            end: range.end.map(web_position),
        }),
        outside_worktree: location.outside_root,
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn a_location_inside_the_root_maps_field_for_field() {
        // Given an index location inside the workspace root
        let inside = index::CodeLocation {
            file: "src/lib.rs".to_string(),
            range: Some(index::SourceRange {
                start: Some(index::SourcePosition { line: 3, column: 1 }),
                end: Some(index::SourcePosition { line: 3, column: 9 }),
            }),
            outside_root: false,
        };

        // When it is mapped for the web
        let mapped = web_location(inside);

        // Then it keeps its path and range and is not marked outside the worktree
        assert_eq!(
            mapped,
            CodeLocation {
                rel_path: "src/lib.rs".to_string(),
                range: Some(SourceRange {
                    start: Some(SourcePosition { line: 3, column: 1 }),
                    end: Some(SourcePosition { line: 3, column: 9 }),
                }),
                outside_worktree: false,
            }
        );
    }

    #[test]
    fn a_location_outside_the_root_is_marked_outside_the_worktree() {
        // Given an index location in a dependency, which the index names by absolute path
        let dependency = index::CodeLocation {
            file: "/home/u/.cargo/registry/src/serde/lib.rs".to_string(),
            range: None,
            outside_root: true,
        };

        // When it is mapped for the web
        let mapped = web_location(dependency);

        // Then the pane is told not to offer opening it, and still gets the absolute path
        assert!(mapped.outside_worktree);
        assert_eq!(mapped.rel_path, "/home/u/.cargo/registry/src/serde/lib.rs");
    }

    #[test]
    fn a_rel_path_that_leaves_the_worktree_is_refused_before_anything_is_forwarded() {
        for escaping in ["../secret.rs", "src/../../secret.rs", "/etc/passwd"] {
            // Given a rel_path that is traversal or absolute
            // When its shape is checked, as every method does before dialling the index
            let refusal = validate_rel_path_shape(escaping)
                .expect_err("a path that leaves the worktree is refused");

            // Then it is an invalid argument
            assert_eq!(
                refusal.code(),
                tddy_rpc::Code::InvalidArgument,
                "{escaping}"
            );
        }
    }

    #[test]
    fn a_plain_relative_path_is_accepted_as_it_is() {
        let accepted = validate_rel_path_shape("src/main.rs").expect("a relative path");
        assert_eq!(Path::new(&accepted), Path::new("src/main.rs"));
    }
}
