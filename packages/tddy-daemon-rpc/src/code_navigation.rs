//! `code_navigation.CodeNavigationService` — definition, references and hover for the web's code
//! pane, answered by the warm code-intelligence index.
//!
//! The service owns no index. Each request is authorised exactly as `worktree.WorktreeService`
//! authorises a file read ([`WorktreeServiceImpl::resolve_listed_worktree`]: token → OS user →
//! project main repo on this host → `git worktree list` membership), then forwarded to
//! `code_index.CodeIndexService` with the listed worktree as its workspace root, over the channel
//! [`IndexChannelSource::connect`] dials — which, for the daemon's registry, starts the index daemon
//! on the first request. The port keeps this crate from naming the registry, which stays with the
//! daemon's wiring.
//!
//! Without an `index_daemon:` configuration section there is no source, and every method answers
//! `FAILED_PRECONDITION` naming that section. There is deliberately no fallback to the agent-tool
//! language server: two indexes answering the same pane would disagree.
//!
//! `CodeSession` — the code pane's bidirectional stream — lives in [`crate::code_session`];
//! `WatchCodeIndex` follows `code_index.WatchRoot` for the session's worktree and never warms it.

use std::path::Path;
use std::sync::Arc;

use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::code_navigation::{
    CodeIndexPhase, CodeIndexProgress, CodeIndexState, CodeLocation, CodeNavigationService,
    CodeNavigationServiceServer, CodeSessionEvent, CodeSessionRequest, DefinitionRequest,
    DefinitionResponse, HoverRequest, HoverResponse, OpenPlanRequest, PlanRunEvent, PlanSnapshot,
    ReferencesRequest, ReferencesResponse, RunPlanRequest, SourcePosition, SourceRange,
    WatchCodeIndexRequest, WatchPlanRequest,
};
use tddy_worktree_service::worktree_files::validate_rel_path_shape;
use tddy_worktree_service::WorktreeServiceImpl;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;

/// How many progress messages `WatchCodeIndex` buffers for a reader that has not drained them yet.
const WATCH_BUFFER: usize = 8;

mod plan;

/// The coordinate the web addresses this service at: `package code_navigation` +
/// `service CodeNavigationService` in `tddy-service/proto/code_navigation.proto`.
pub const CODE_NAVIGATION_SERVICE: &str = "code_navigation.CodeNavigationService";

/// The error a source reports when it cannot hand out a channel; only its `Display` is surfaced.
pub type IndexChannelError = Box<dyn std::error::Error + Send + Sync>;

/// Where the index daemon's channel comes from. The daemon implements it for the registry that
/// manages the index daemon process; this crate only dials through it.
#[async_trait::async_trait]
pub trait IndexChannelSource: Send + Sync {
    /// A channel to the index daemon, starting it if nothing has yet.
    async fn connect(&self) -> Result<Channel, IndexChannelError>;
}

/// `code_navigation.CodeNavigationService`, over this daemon's worktree authorisation and its
/// managed index daemon.
pub struct CodeNavigationServiceImpl {
    /// Whose `resolve_listed_worktree` gates every request, so this service can never reach a path
    /// the worktree service would refuse to read.
    worktrees: Arc<WorktreeServiceImpl>,
    /// The index daemon this runtime manages; `None` when no `index_daemon:` section asked for one.
    index_daemon: Option<Arc<dyn IndexChannelSource>>,
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
        index_daemon: Option<Arc<dyn IndexChannelSource>>,
    ) -> Self {
        Self {
            worktrees,
            index_daemon,
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
        Ok(Forward {
            client: connect_index(self.index_daemon.as_ref()).await?,
            workspace_root: root.display().to_string(),
            file,
            position: position.map(index_position),
        })
    }
}

/// A client on the index daemon's channel — dialling it starts the index daemon on first use — or
/// `FAILED_PRECONDITION` when this daemon has no `index_daemon:` section.
pub(crate) async fn connect_index(
    index_daemon: Option<&Arc<dyn IndexChannelSource>>,
) -> Result<CodeIndexServiceClient<Channel>, Status> {
    let source = index_daemon.ok_or_else(|| {
        Status::failed_precondition(
            "code navigation needs the index daemon, and this daemon has no `index_daemon:` \
             configuration section",
        )
    })?;
    let channel = source
        .connect()
        .await
        .map_err(|err| Status::unavailable(format!("index daemon: {err}")))?;
    Ok(CodeIndexServiceClient::new(channel))
}

#[async_trait::async_trait]
impl CodeNavigationService for CodeNavigationServiceImpl {
    type WatchCodeIndexStream = ReceiverStream<Result<CodeIndexProgress, Status>>;
    type WatchPlanStream = ReceiverStream<Result<PlanSnapshot, Status>>;
    type RunPlanStream = ReceiverStream<Result<PlanRunEvent, Status>>;
    type CodeSessionStream = ReceiverStream<Result<CodeSessionEvent, Status>>;

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
            state: web_state(answer.state()) as i32,
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
            state: web_state(answer.state()) as i32,
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
            state: web_state(answer.state()) as i32,
            markdown: answer.markdown,
        }))
    }

    /// The session's worktree's code index: its current state, then every change, whoever started
    /// the load — until the client leaves. Never starts a load. A session whose record names no
    /// Rust worktree yet, or a daemon without an index daemon, ends at once with no message.
    async fn watch_code_index(
        &self,
        request: Request<WatchCodeIndexRequest>,
    ) -> Result<Response<Self::WatchCodeIndexStream>, Status> {
        let r = request.into_inner();
        // The session must be the caller's, as the worktree service decides it, before its record
        // is read or its worktree's progress — an `error` can carry paths — is followed.
        let session_dir = self
            .worktrees
            .resolve_owned_session_dir(&r.session_token, &r.session_id)?;
        let worktree = session_worktree(&session_dir)?;
        let (tx, rx) = tokio::sync::mpsc::channel(WATCH_BUFFER);
        let (Some(worktree), Some(source)) = (
            worktree.filter(|root| root.join("Cargo.toml").is_file()),
            self.index_daemon.as_ref(),
        ) else {
            // Nothing to follow: an empty stream, so no indicator shows.
            return Ok(Response::new(ReceiverStream::new(rx)));
        };
        let progress = connect_index(Some(source))
            .await?
            .watch_root(index::WatchRootRequest {
                workspace_root: worktree.display().to_string(),
            })
            .await
            .map_err(tddy_service::to_rpc_status)?
            .into_inner();
        tokio::spawn(forward_progress(progress, tx));
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    /// The code pane's conversation with its worktree's index: warms it, streams its progress, and
    /// answers queries — holding those asked while the index loads. See [`crate::code_session`].
    async fn code_session(
        &self,
        request: Request<tddy_rpc::Streaming<CodeSessionRequest>>,
    ) -> Result<Response<Self::CodeSessionStream>, Status> {
        Ok(Response::new(crate::code_session::start(
            Arc::clone(&self.worktrees),
            self.index_daemon.clone(),
            request.into_inner(),
        )))
    }

    /// Open a plan file of the session's worktree: load it into the index's plan store and answer
    /// its operations with their status and staleness.
    async fn open_plan(
        &self,
        request: Request<OpenPlanRequest>,
    ) -> Result<Response<PlanSnapshot>, Status> {
        let r = request.into_inner();
        let mut forward = self
            .authorise_and_connect(
                &r.session_token,
                &r.project_id,
                &r.worktree_path,
                &r.rel_path,
                None,
            )
            .await?;
        let rows = plan::read_plan_rows(Path::new(&forward.workspace_root), &forward.file).await?;
        let snapshot = plan::open_snapshot(
            &mut forward.client,
            &forward.workspace_root,
            &forward.file,
            &rows,
        )
        .await?;
        Ok(Response::new(snapshot))
    }

    /// A plan's operations as their status and staleness change: the current snapshot, then a new
    /// one whenever the store reports something different, until the client goes away.
    async fn watch_plan(
        &self,
        request: Request<WatchPlanRequest>,
    ) -> Result<Response<Self::WatchPlanStream>, Status> {
        let r = request.into_inner();
        let mut forward = self
            .authorise_and_connect(
                &r.session_token,
                &r.project_id,
                &r.worktree_path,
                &r.rel_path,
                None,
            )
            .await?;
        let rows = plan::read_plan_rows(Path::new(&forward.workspace_root), &forward.file).await?;
        let last = plan::open_snapshot(
            &mut forward.client,
            &forward.workspace_root,
            &forward.file,
            &rows,
        )
        .await?;
        let (tx, rx) = tokio::sync::mpsc::channel(WATCH_BUFFER);
        tokio::spawn(plan::follow(
            forward.client,
            forward.workspace_root,
            forward.file,
            rows,
            last,
            tx,
        ));
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    /// Apply a plan through the warm index, streaming each operation's outcome. A run the index
    /// stops on an error ends with a `failure` event.
    async fn run_plan(
        &self,
        request: Request<RunPlanRequest>,
    ) -> Result<Response<Self::RunPlanStream>, Status> {
        let r = request.into_inner();
        let mut forward = self
            .authorise_and_connect(
                &r.session_token,
                &r.project_id,
                &r.worktree_path,
                &r.rel_path,
                None,
            )
            .await?;
        let rows = plan::read_plan_rows(Path::new(&forward.workspace_root), &forward.file).await?;
        let events = forward
            .client
            .apply(index::ApplyRequest {
                workspace_root: forward.workspace_root,
                plan: forward.file,
                dry_run: false,
                resume: false,
                from: None,
                stop_after: None,
            })
            .await
            .map_err(tddy_service::to_rpc_status)?
            .into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(WATCH_BUFFER);
        tokio::spawn(plan::relay_run(events, rows, tx));
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

/// The worktree `session_dir`'s `.session.yaml` records, or `None` while it records none — a session
/// whose worktree does not exist yet has no index to follow.
fn session_worktree(session_dir: &Path) -> Result<Option<std::path::PathBuf>, Status> {
    let metadata = tddy_core::read_session_metadata(session_dir).map_err(|err| {
        Status::failed_precondition(format!("the session's .session.yaml cannot be read: {err}"))
    })?;
    Ok(metadata.repo_path.map(std::path::PathBuf::from))
}

/// Relays a root's `WatchRoot` progress to `tx`, in the web's vocabulary, until the index ends the
/// stream or the client leaves. A broken stream ends the client's with the index's status.
async fn forward_progress(
    mut progress: tonic::Streaming<index::IndexProgress>,
    tx: tokio::sync::mpsc::Sender<Result<CodeIndexProgress, Status>>,
) {
    loop {
        let next = tokio::select! {
            next = progress.message() => next,
            () = tx.closed() => return,
        };
        let item = match next {
            Ok(Some(update)) => Ok(web_progress(update)),
            Ok(None) => return,
            Err(status) => Err(tddy_service::to_rpc_status(status)),
        };
        let broken = item.is_err();
        if tx.send(item).await.is_err() || broken {
            return;
        }
    }
}

/// The index's state of a root in the web's vocabulary.
pub(crate) fn web_state(state: index::IndexState) -> CodeIndexState {
    match state {
        index::IndexState::Unspecified => CodeIndexState::Unspecified,
        index::IndexState::Idle => CodeIndexState::Idle,
        index::IndexState::Loading => CodeIndexState::Loading,
        index::IndexState::Ready => CodeIndexState::Ready,
        index::IndexState::Failed => CodeIndexState::Failed,
    }
}

/// A root's progress, as the index reports it, in the web's vocabulary — field for field.
pub(crate) fn web_progress(update: index::IndexProgress) -> CodeIndexProgress {
    CodeIndexProgress {
        state: web_state(update.state()) as i32,
        line: update.line,
        phase: update.phase,
        percentage: update.percentage,
        furthest: update.furthest,
        ready: update.ready,
        error: update.error,
        phases: update
            .phases
            .into_iter()
            .map(|phase| CodeIndexPhase {
                title: phase.title,
                message: phase.message,
                percentage: phase.percentage,
            })
            .collect(),
    }
}

pub(crate) fn index_position(position: SourcePosition) -> index::SourcePosition {
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
pub(crate) fn web_location(location: index::CodeLocation) -> CodeLocation {
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
