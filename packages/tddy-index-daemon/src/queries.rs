//! The three questions this service answers without streaming anything.
//!
//! Split from [`crate::operations`] because the absence of a stream is the difference that
//! matters: a unary handler has no back-channel, so it has no way to learn that its caller has
//! gone and no per-request stream for an index wait to report into. Each of these is therefore
//! bounded by how long its one answer takes, and its progress goes to the log.
//!
//! Most still take the root's queue, because each reads the tree or the `.restructure/` state a
//! concurrent apply is writing. Loading and listing plans do not: they read plan files and the
//! store, never the tree.

use std::path::PathBuf;

use tddy_code_restructuring::item_anchor;
use tddy_code_restructuring::plan_store::{OpStaleness, PlanStore};
use tddy_code_restructuring::runner::{self, Command, Options};
use tddy_rpc::Status;
use tokio_util::sync::CancellationToken;

use crate::activity::Activity;
use crate::index::WorkspaceIndex;
use crate::operations::{joined, plan_path};
use crate::proto::code_index::{
    AnchorsRequest, AnchorsResponse, ListPlansRequest, LoadPlansRequest, LoadedPlan,
    PlanStatusRequest, PlanStatusResponse, PlansResponse, SnapshotRequest, SnapshotResponse,
    SourcePosition, SourceRange, StaleOp, UnloadPlansRequest, VerifyRequest, VerifyResponse,
};
use crate::status::status_of;

/// The anchor a plan carries for a named run of items, or for the item enclosing a position.
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
///
/// `anchor_json` is the anchor and `range` is where it lands in the tree as it stands now — the
/// covering range of the named items, or the absolute form of the position's relative range.
async fn anchor_covering(
    index: &WorkspaceIndex,
    root: PathBuf,
    request: AnchorsRequest,
) -> Result<AnchorsResponse, Status> {
    if request.file.trim().is_empty() {
        return Err(Status::invalid_argument("the request names no file"));
    }
    let at = request.at.as_ref().map(position_range).transpose()?;
    if request.items.is_empty() && at.is_none() {
        return Err(Status::invalid_argument(
            "the request names no items for the anchor to cover",
        ));
    }
    if !request.items.is_empty() && at.is_some() {
        return Err(Status::invalid_argument(
            "the request names both items and a position — an anchor covers one or the other",
        ));
    }

    let _queued = index.hold(&root).await;
    let client = index.client_for(&root).await?;
    let progress = logged_progress();
    let options = Options {
        command: Command::Anchors,
        target: Some(PathBuf::from(&request.file)),
        items: request.items,
        at,
        progress: std::sync::Arc::clone(&progress),
        trace: logged_trace,
        wait_heartbeat: index.wait_heartbeat(),
        ..Options::default()
    };

    // A unary handler has no stream to learn its caller left through, but it is dropped when the
    // caller goes: the guard turns that drop into the cancellation the index waits listen to. Without
    // it the wait runs on inside `spawn_blocking` holding `index.hold(root)`, and every later
    // request for this workspace queues behind it.
    let cancel = CancellationToken::new();
    let _stop_when_dropped = cancel.clone().drop_guard();
    let options_heartbeat = options.wait_heartbeat;
    let (anchor, range) = tokio::task::spawn_blocking(move || {
        let anchor = runner::item_anchors(
            &root,
            options,
            Some(std::sync::Arc::clone(&client)),
            cancel.clone(),
        )?;
        let mut registry =
            runner::registry_for_waiting(client, cancel, progress, logged_trace, options_heartbeat);
        let range = item_anchor::span_of(&anchor, &root, &mut registry)?;
        Ok((anchor, range))
    })
    .await
    .map_err(|failure| joined("anchors", &failure))?
    .map_err(|refusal: tddy_code_restructuring::RestructureError| status_of(&refusal))?;

    Ok(AnchorsResponse {
        anchor_json: tddy_code_restructuring::console::item_anchor(&anchor),
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

/// Rewrite a plan's snapshot header to the tree as it stands, and for an item-anchored plan
/// re-resolve its anchors on the root's warm index.
///
/// Split in two for the reason [`serve_anchors`] is: the whole answer, refusal included, passes
/// through [`Activity::recorded`].
pub(crate) async fn serve_snapshot(
    index: &WorkspaceIndex,
    request: SnapshotRequest,
) -> Result<SnapshotResponse, Status> {
    let (activity, root) = Activity::arrived("snapshot", index, &request.workspace_root).await?;
    activity.recorded(snapshotted(index, root, request).await)
}

/// The rewrite itself, with the root already resolved and its arrival already recorded.
///
/// Takes the root's queue: it writes the plan file and reads the tree the hashes are taken from,
/// both of which a concurrent apply is changing. A server is acquired only for a plan that has item
/// anchors, as in-process: a header is hashed without one, and waiting for an index the answer never
/// consults would make a cheap command as slow as a cold one.
async fn snapshotted(
    index: &WorkspaceIndex,
    root: PathBuf,
    request: SnapshotRequest,
) -> Result<SnapshotResponse, Status> {
    let plan = plan_path(&root, &request.plan)?;
    let options = Options {
        command: Command::Snapshot,
        target: Some(plan.clone()),
        progress: logged_progress(),
        trace: logged_trace,
        ..Options::default()
    };

    let _queued = index.hold(&root).await;
    let client = if item_anchor::plan_file_has_item_anchors(&plan) {
        Some(index.client_for(&root).await?)
    } else {
        None
    };

    // Dropped with the handler when the caller goes, which is the cancellation the index wait
    // listens to: see `anchor_covering`.
    let cancel = CancellationToken::new();
    let _stop_when_dropped = cancel.clone().drop_guard();
    let rewrite = tokio::task::spawn_blocking(move || {
        runner::snapshot_resolving(&root, options, client, cancel.clone())
    })
    .await
    .map_err(|failure| joined("snapshot", &failure))?
    .map_err(|refusal| status_of(&refusal))?;

    Ok(SnapshotResponse {
        paths: rewrite.paths as u32,
        rewritten: rewrite.rewritten,
        stale: stale_on_the_wire(&rewrite.stale),
    })
}

/// A one-based range as the wire carries it, back into the library's.
fn position_range(wire: &SourceRange) -> Result<tddy_code_restructuring::Range, Status> {
    let (Some(start), Some(end)) = (&wire.start, &wire.end) else {
        return Err(Status::invalid_argument(
            "the position to anchor needs both a start and an end",
        ));
    };
    let position = |at: &SourcePosition| tddy_code_restructuring::Position {
        line: at.line,
        col: at.column,
    };
    Ok(tddy_code_restructuring::Range {
        start: position(start),
        end: position(end),
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
        target: Some(plan.clone()),
        ..Options::default()
    };

    let _queued = index.hold(&root).await;
    // A loaded plan is counted as the store holds it, which is the plan a run would execute; one
    // that is not loaded is read from its file, since a status does not load what it looks at.
    let store = index.plans_of(&root).await;
    let (progress, stale) = tokio::task::spawn_blocking(move || {
        let held = {
            let store = store.lock().expect("a root's plan store");
            store.key_for(&plan).ok().and_then(|key| {
                store
                    .get(&key)
                    .map(|loaded| (loaded.plan.clone(), store.stale_ops(&key)))
            })
        };
        match held {
            Some((held, stale)) => {
                runner::status_of_plan(&root, &plan, &held).map(|progress| (progress, stale))
            }
            None => runner::status(&root, options).map(|progress| (progress, Vec::new())),
        }
    })
    .await
    .map_err(|failure| joined("plan status", &failure))?
    .map_err(|refusal| status_of(&refusal))?;

    Ok(PlanStatusResponse {
        completed: progress.completed as u32,
        in_flight: progress.in_flight as u32,
        pending: progress.pending as u32,
        failed: progress.failed as u32,
        stale: stale_on_the_wire(&stale),
    })
}

/// Stale operations as the wire carries them.
fn stale_on_the_wire(stale: &[OpStaleness]) -> Vec<StaleOp> {
    stale
        .iter()
        .map(|found| StaleOp {
            op: found.op.to_string(),
            reason: found.reason.to_string(),
        })
        .collect()
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
        repointed: comparison.excused.repointed as u32,
        visibility_normalised: comparison.excused.visibility as u32,
        cfg_test_gates: comparison.excused.cfg_test_gates as u32,
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
///
/// Does not take the root's queue: it reads plan files and touches no tree, so it is answered while
/// a run is going — the one thing that must wait is the store's lock, for as long as the run's step
/// that holds it.
pub(crate) async fn serve_load_plans(
    index: &WorkspaceIndex,
    request: LoadPlansRequest,
) -> Result<PlansResponse, Status> {
    let (activity, root) = Activity::arrived("load plans", index, &request.workspace_root).await?;
    activity.recorded(load_plans(index, root, request).await)
}

async fn load_plans(
    index: &WorkspaceIndex,
    root: PathBuf,
    request: LoadPlansRequest,
) -> Result<PlansResponse, Status> {
    if request.plans.is_empty() {
        return Err(Status::invalid_argument(
            "the request names no plans to load",
        ));
    }
    let plans = request
        .plans
        .iter()
        .map(|plan| plan_path(&root, plan))
        .collect::<Result<Vec<_>, _>>()?;

    let store = index.plans_of(&root).await;
    tokio::task::spawn_blocking(move || {
        let mut store = store.lock().expect("a root's plan store");
        store.load(&plans)?;
        Ok(held_by(&store))
    })
    .await
    .map_err(|failure| joined("load plans", &failure))?
    .map_err(|refusal: tddy_code_restructuring::RestructureError| status_of(&refusal))
}

/// Flush and drop plans from the root's store, answering what it still holds.
///
/// Takes the root's queue, because dropping the plan a run is executing would pull it out from
/// under the run: it waits for the run to end.
pub(crate) async fn serve_unload_plans(
    index: &WorkspaceIndex,
    request: UnloadPlansRequest,
) -> Result<PlansResponse, Status> {
    let (activity, root) =
        Activity::arrived("unload plans", index, &request.workspace_root).await?;
    activity.recorded(unload_plans(index, root, request).await)
}

async fn unload_plans(
    index: &WorkspaceIndex,
    root: PathBuf,
    request: UnloadPlansRequest,
) -> Result<PlansResponse, Status> {
    match (request.all, request.plans.is_empty()) {
        (true, false) => {
            return Err(Status::invalid_argument(
                "the request names plans and asks for all of them — name one or the other",
            ))
        }
        (false, true) => {
            return Err(Status::invalid_argument(
                "the request names no plans to unload",
            ))
        }
        _ => {}
    }

    let _queued = index.hold(&root).await;
    let store = index.plans_of(&root).await;
    tokio::task::spawn_blocking(move || {
        let mut store = store.lock().expect("a root's plan store");
        if request.all {
            store.unload_all()?;
        } else {
            let plans: Vec<PathBuf> = request.plans.iter().map(PathBuf::from).collect();
            store.unload(&plans)?;
        }
        Ok(held_by(&store))
    })
    .await
    .map_err(|failure| joined("unload plans", &failure))?
    .map_err(|refusal: tddy_code_restructuring::RestructureError| status_of(&refusal))
}

/// The plans the root's store holds.
pub(crate) async fn serve_list_plans(
    index: &WorkspaceIndex,
    request: ListPlansRequest,
) -> Result<PlansResponse, Status> {
    let (activity, root) = Activity::arrived("list plans", index, &request.workspace_root).await?;
    let store = index.plans_of(&root).await;
    let listed =
        tokio::task::spawn_blocking(move || held_by(&store.lock().expect("a root's plan store")))
            .await
            .map_err(|failure| joined("list plans", &failure));
    activity.recorded(listed)
}

/// What a store holds, as the wire carries it.
fn held_by(store: &PlanStore) -> PlansResponse {
    PlansResponse {
        plans: store
            .list()
            .into_iter()
            .map(|held| LoadedPlan {
                plan: held.key.to_string(),
                ops: held.ops as u32,
                dirty: held.dirty,
                stale: stale_on_the_wire(&store.stale_ops(&held.key)),
            })
            .collect(),
    }
}
