//! The three questions this service answers without streaming anything.
//!
//! Split from [`crate::operations`] because the absence of a stream is the difference that
//! matters: a unary handler has no back-channel, so it has no way to learn that its caller has
//! gone and no per-request stream for an index wait to report into. Each of these is therefore
//! bounded by how long its one answer takes, and its progress goes to the log.
//!
//! They still take the root's queue, because each reads the tree or the `.restructure/` state a
//! concurrent apply is writing.

use std::path::{Path, PathBuf};

use tddy_code_restructuring::registry::Workspace;
use tddy_code_restructuring::runner::{self, Command, Options};
use tddy_code_restructuring::{Overlay, RefactorKind};
use tddy_rpc::Status;
use tokio_util::sync::CancellationToken;

use crate::activity::Activity;
use crate::index::WorkspaceIndex;
use crate::operations::{joined, plan_path};
use crate::proto::code_index::{
    AnchorsRequest, AnchorsResponse, ListPlansRequest, LoadPlansRequest, PlanStatusRequest,
    PlanStatusResponse, PlansResponse, SourcePosition, SourceRange, UnloadPlansRequest,
    VerifyRequest, VerifyResponse,
};
use crate::status::status_of;

/// The range anchor covering a named run of items, trivia included.
///
/// Split in two so the whole answer — refusal included — passes through
/// [`Activity::recorded`]: the resolving happens in the inner function, and this one is the record
/// that a request arrived and what it came to.
pub(crate) async fn serve_anchors(
    index: &WorkspaceIndex,
    request: AnchorsRequest,
) -> Result<AnchorsResponse, Status> {
    let (activity, root) = Activity::arrived("anchors", index, &request.workspace_root).await?;
    activity.recorded(anchor_covering(index, root, request).await)
}

/// The anchor itself, with the root already resolved and its arrival already recorded.
async fn anchor_covering(
    index: &WorkspaceIndex,
    root: PathBuf,
    request: AnchorsRequest,
) -> Result<AnchorsResponse, Status> {
    if request.file.trim().is_empty() {
        return Err(Status::invalid_argument("the request names no file"));
    }
    if request.items.is_empty() {
        return Err(Status::invalid_argument(
            "the request names no items for the anchor to cover",
        ));
    }

    let _queued = index.hold(&root).await;
    let client = index.client_for(&root).await?;
    let file = request.file;
    let items = request.items;

    let range = tokio::task::spawn_blocking(move || {
        // `ExtractModule` is not the operation being performed — there is none. It is how a file
        // selects its backend, which is the same route `runner::anchors` takes.
        let mut registry = runner::registry_for(
            client,
            CancellationToken::new(),
            logged_progress(),
            logged_trace,
        );
        let overlay = Overlay::new();
        registry
            .backend_for(Path::new(&file), RefactorKind::ExtractModule)?
            .anchor_for(
                &file,
                &items,
                &Workspace {
                    root: &root,
                    overlay: &overlay,
                },
            )
    })
    .await
    .map_err(|failure| joined("anchors", &failure))?
    .map_err(|refusal| status_of(&refusal))?;

    Ok(AnchorsResponse {
        // TODO(item-anchors): implement — the `items` anchor for named items, the `item` anchor for
        // `at` (which this function also has to route rather than refuse for empty `items`).
        anchor_json: String::new(),
        range: Some(SourceRange {
            start: Some(SourcePosition {
                line: range.start.line,
                column: range.start.col,
            }),
            end: Some(SourcePosition {
                line: range.end.line,
                column: range.end.col,
            }),
        }),
    })
}

/// How far a plan's journal got, counted by [`runner::status`] rather than here.
///
/// This used to re-derive the four numbers from `Plan::parse` plus `Journal::load`, because the
/// library's own `status` printed them and returned nothing. It returns them now, so there is one
/// place the arithmetic lives — `in_flight` discounting the operations that went on to complete is
/// the kind of detail two copies drift on.
pub(crate) async fn serve_plan_status(
    index: &WorkspaceIndex,
    request: PlanStatusRequest,
) -> Result<PlanStatusResponse, Status> {
    let (activity, root) = Activity::arrived("plan status", index, &request.workspace_root).await?;
    activity.recorded(plan_progress(index, root, request).await)
}

/// The four counts themselves, with the root already resolved and its arrival already recorded.
async fn plan_progress(
    index: &WorkspaceIndex,
    root: PathBuf,
    request: PlanStatusRequest,
) -> Result<PlanStatusResponse, Status> {
    let plan = plan_path(&root, &request.plan)?;

    let options = Options {
        command: Command::Status,
        target: Some(plan),
        ..Options::default()
    };

    let _queued = index.hold(&root).await;
    let progress = tokio::task::spawn_blocking(move || runner::status(&root, options))
        .await
        .map_err(|failure| joined("plan status", &failure))?
        .map_err(|refusal| status_of(&refusal))?;

    Ok(PlanStatusResponse {
        completed: progress.completed as u32,
        in_flight: progress.in_flight as u32,
        pending: progress.pending as u32,
        failed: progress.failed as u32,
        // TODO(live-plans): implement — the held plan's stale operations.
        stale: Vec::new(),
    })
}

/// Hold the working tree's statements against a git ref's, as multisets.
pub(crate) async fn serve_verify(
    index: &WorkspaceIndex,
    request: VerifyRequest,
) -> Result<VerifyResponse, Status> {
    let (activity, root) = Activity::arrived("verify", index, &request.workspace_root).await?;
    activity.recorded(comparison_against(index, root, request).await)
}

/// The comparison itself, with the root already resolved and its arrival already recorded.
async fn comparison_against(
    index: &WorkspaceIndex,
    root: PathBuf,
    request: VerifyRequest,
) -> Result<VerifyResponse, Status> {
    if request.against.trim().is_empty() {
        return Err(Status::invalid_argument(
            "the request names no git ref to verify against",
        ));
    }
    let options = Options {
        command: Command::Verify,
        against: Some(request.against),
        ..Options::default()
    };

    let _queued = index.hold(&root).await;
    let comparison = tokio::task::spawn_blocking(move || runner::verify(&root, options))
        .await
        .map_err(|failure| joined("verify", &failure))?
        .map_err(|refusal| status_of(&refusal))?;

    // A comparison that does not hold is this answer with `holds` false, not an error: the
    // comparison *is* what was asked for, and a caller that receives the statements it lost and
    // gained decides for itself what they mean. Only a tree that could not be compared at all —
    // one that is not a git worktree, or a ref git will not read — refuses the request.
    Ok(VerifyResponse {
        holds: comparison.holds(),
        before: comparison.before as u32,
        after: comparison.after as u32,
        missing: comparison.missing,
        added: comparison.added,
    })
}

/// A progress sink for the unary operations, which have no stream to report into.
///
/// The log rather than stdout: this process speaks a protocol there.
fn logged_progress() -> tddy_code_restructuring::backends::rust::ProgressSink {
    std::sync::Arc::new(|line: &str| {
        log::debug!(target: "tddy_index_daemon::operations", "indexing: {line}");
    })
}

/// Where a seam's diagnostic trace goes when `RESTRUCTURE_TRACE` asks for one. The log, for the
/// same reason: a line this process writes to stdout is a line in somebody's RPC frame.
fn logged_trace(line: &str) {
    log::debug!(target: "tddy_index_daemon::operations", "trace: {line}");
}

/// Load plans into the root's store, answering every plan the store then holds.
pub(crate) async fn serve_load_plans(
    index: &WorkspaceIndex,
    request: LoadPlansRequest,
) -> Result<PlansResponse, Status> {
    // TODO(plan-store): implement
    let _ = (index, request);
    Err(Status::unimplemented("LoadPlans: TODO(plan-store)"))
}

/// Flush and drop plans from the root's store, answering what it still holds.
pub(crate) async fn serve_unload_plans(
    index: &WorkspaceIndex,
    request: UnloadPlansRequest,
) -> Result<PlansResponse, Status> {
    // TODO(plan-store): implement
    let _ = (index, request);
    Err(Status::unimplemented("UnloadPlans: TODO(plan-store)"))
}

/// The plans the root's store holds.
pub(crate) async fn serve_list_plans(
    index: &WorkspaceIndex,
    request: ListPlansRequest,
) -> Result<PlansResponse, Status> {
    // TODO(plan-store): implement
    let _ = (index, request);
    Err(Status::unimplemented("ListPlans: TODO(plan-store)"))
}
