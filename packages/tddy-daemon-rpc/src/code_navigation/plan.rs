//! The restructure-plan half of `code_navigation.CodeNavigationService`: what the plan dialog shows
//! and how a run is relayed.
//!
//! The index daemon's plan calls answer less than the dialog lists — `LoadPlans` carries an operation
//! count and `PlanStatus` journal counts — so the rows (id, kind, anchor, group) come from the plan
//! file itself, read through the same listed-file gate `WorktreeService` applies, and the store's
//! stale reasons are folded in.

use std::path::Path;
use std::time::Duration;

use tddy_code_restructuring::{Anchor, Plan, RefactorOp};
use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;
use tddy_rpc::Status;
use tddy_service::proto::code_navigation::{
    plan_run_event, PlanOperation, PlanOperationApplied, PlanOperationStatus, PlanRunEvent,
    PlanRunFailure, PlanRunOutcome, PlanSnapshot,
};
use tddy_worktree_service::worktree_files::read_worktree_file_utf8;
use tokio::sync::mpsc::Sender;
use tonic::transport::Channel;

/// How often `WatchPlan` asks the index daemon's plan store whether anything changed; the store has
/// no change feed to follow.
const PLAN_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// One operation of a plan file, as the plan spells it.
pub(super) struct PlanRow {
    id: String,
    kind: String,
    item: String,
    file: String,
    group: String,
}

/// The rows of the plan file `rel_path` under `root`, in plan order.
///
/// Ids the file does not carry are the ones the store assigns on load (`op-<n>`), so rows and the
/// store's events and stale list name an operation alike.
pub(super) async fn read_plan_rows(root: &Path, rel_path: &str) -> Result<Vec<PlanRow>, Status> {
    let (root, rel_path) = (root.to_path_buf(), rel_path.to_string());
    let content = tokio::task::spawn_blocking(move || read_worktree_file_utf8(&root, &rel_path))
        .await
        .map_err(|err| Status::internal(format!("reading the plan: {err}")))??;
    if content.truncated {
        return Err(Status::failed_precondition(
            "the plan file is too large to open as a plan",
        ));
    }
    let mut plan = Plan::parse(&content.content_utf8)
        .map_err(|err| Status::failed_precondition(format!("not a restructure plan: {err}")))?;
    plan.assign_missing_op_ids();
    plan.ops.iter().map(plan_row).collect()
}

fn plan_row(op: &RefactorOp) -> Result<PlanRow, Status> {
    let kind = serde_json::to_value(op.op)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| Status::internal("an operation kind has no name"))?;
    Ok(PlanRow {
        id: op.id.as_ref().map(|id| id.0.clone()).unwrap_or_default(),
        kind,
        item: anchor_item(&op.anchor),
        file: op.anchor.file().to_string(),
        group: op.group.clone().unwrap_or_default(),
    })
}

/// The item or symbol path an anchor names; empty for a range.
fn anchor_item(anchor: &Anchor) -> String {
    match anchor {
        Anchor::Symbol { path, .. } => path.clone(),
        Anchor::Item { item, .. } => item.to_string(),
        Anchor::Items { items, .. } => items
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", "),
        Anchor::Range { .. } => String::new(),
    }
}

/// Load the plan into the index's store for `workspace_root`, then describe it: the `rows` read from
/// the file, each with its status and the store's stale reason.
pub(super) async fn open_snapshot(
    client: &mut CodeIndexServiceClient<Channel>,
    workspace_root: &str,
    rel_path: &str,
    rows: &[PlanRow],
) -> Result<PlanSnapshot, Status> {
    client
        .load_plans(index::LoadPlansRequest {
            workspace_root: workspace_root.to_string(),
            plans: vec![rel_path.to_string()],
        })
        .await
        .map_err(tddy_service::to_rpc_status)?;
    current_snapshot(client, workspace_root, rel_path, rows).await
}

/// The plan's rows as the store reports them now.
pub(super) async fn current_snapshot(
    client: &mut CodeIndexServiceClient<Channel>,
    workspace_root: &str,
    rel_path: &str,
    rows: &[PlanRow],
) -> Result<PlanSnapshot, Status> {
    let status = client
        .plan_status(index::PlanStatusRequest {
            workspace_root: workspace_root.to_string(),
            plan: rel_path.to_string(),
        })
        .await
        .map_err(tddy_service::to_rpc_status)?
        .into_inner();
    Ok(snapshot_of(rel_path, rows, &status))
}

fn snapshot_of(
    rel_path: &str,
    rows: &[PlanRow],
    status: &index::PlanStatusResponse,
) -> PlanSnapshot {
    // TODO(plan-dialog): `PlanStatus` reports journal counts, not a status per operation, so the
    // counts are laid over the plan in order — completed, then in flight, then failed. That is
    // exact for a run from the start of a plan and approximate after a `from` or `stop_after` run;
    // exact rows need a per-operation status on the index daemon's `PlanStatus`.
    let completed = status.completed as usize;
    let in_flight_end = completed + status.in_flight as usize;
    let failed_end = in_flight_end + status.failed as usize;
    let operations = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let position = if index < completed {
                PlanOperationStatus::Applied
            } else if index < in_flight_end {
                PlanOperationStatus::InFlight
            } else if index < failed_end {
                PlanOperationStatus::Failed
            } else {
                PlanOperationStatus::Pending
            };
            PlanOperation {
                id: row.id.clone(),
                index: u32::try_from(index).unwrap_or(u32::MAX),
                op: row.kind.clone(),
                item: row.item.clone(),
                file: row.file.clone(),
                group: row.group.clone(),
                status: position as i32,
                stale_reason: status
                    .stale
                    .iter()
                    .find(|stale| stale.op == row.id)
                    .map(|stale| stale.reason.clone())
                    .unwrap_or_default(),
            }
        })
        .collect();
    PlanSnapshot {
        rel_path: rel_path.to_string(),
        operations,
    }
}

/// Send `last`, then a new snapshot whenever the store reports something different, until the
/// reader goes away or the store fails (the failure is sent as the stream's last item).
pub(super) async fn follow(
    mut client: CodeIndexServiceClient<Channel>,
    workspace_root: String,
    rel_path: String,
    rows: Vec<PlanRow>,
    mut last: PlanSnapshot,
    tx: Sender<Result<PlanSnapshot, Status>>,
) {
    if tx.send(Ok(last.clone())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            () = tx.closed() => return,
            () = tokio::time::sleep(PLAN_POLL_INTERVAL) => {}
        }
        let current = match current_snapshot(&mut client, &workspace_root, &rel_path, &rows).await {
            Ok(current) => current,
            Err(status) => {
                let _ = tx.send(Err(status)).await;
                return;
            }
        };
        if current != last {
            if tx.send(Ok(current.clone())).await.is_err() {
                return;
            }
            last = current;
        }
    }
}

/// Relay the index's apply `events` as the dialog's run events until the run's terminal event, the
/// end of the stream, or the reader going away.
pub(super) async fn relay_run(
    mut events: tonic::Streaming<index::RestructureEvent>,
    rows: Vec<PlanRow>,
    tx: Sender<Result<PlanRunEvent, Status>>,
) {
    loop {
        let relayed = match events.message().await {
            Ok(Some(event)) => match run_event(event) {
                Some(event) => event,
                None => continue,
            },
            Ok(None) => return,
            Err(status) => run_failure(&tddy_service::to_rpc_status(status), &rows),
        };
        let terminal = matches!(
            relayed.event,
            Some(plan_run_event::Event::Outcome(_) | plan_run_event::Event::Failure(_))
        );
        if tx.send(Ok(relayed)).await.is_err() || terminal {
            return;
        }
    }
}

/// An index event in the dialog's terms. Progress and findings are not part of a run's account.
pub(super) fn run_event(event: index::RestructureEvent) -> Option<PlanRunEvent> {
    let mapped = match event.event? {
        index::restructure_event::Event::Operation(applied) => {
            plan_run_event::Event::Operation(PlanOperationApplied {
                op_id: applied.op_id,
                index: applied.index,
                done: applied.done,
                total: applied.total,
                files: applied.files,
            })
        }
        index::restructure_event::Event::Note(note) => plan_run_event::Event::Note(note),
        index::restructure_event::Event::Outcome(outcome) => {
            plan_run_event::Event::Outcome(PlanRunOutcome {
                applied: outcome.applied,
                total: outcome.total,
            })
        }
        index::restructure_event::Event::Indexing(_)
        | index::restructure_event::Event::Finding(_) => return None,
    };
    Some(PlanRunEvent {
        event: Some(mapped),
    })
}

/// The terminal event of a run the index stopped with `status`.
///
/// A group that did not compile reaches this service only as `RestructureError::GroupDoesNotCompile`
/// worded into a `FAILED_PRECONDITION` — "group `<name>` does not compile at its end, …" — so the
/// group is read back out of that wording, and every operation of it was restored together.
pub(super) fn run_failure(status: &Status, rows: &[PlanRow]) -> PlanRunEvent {
    let message = status.message().to_string();
    let group = if status.code() == tddy_rpc::Code::FailedPrecondition {
        failed_group(&message).map(str::to_string)
    } else {
        None
    };
    let rolled_back = group
        .as_deref()
        .map(|group| {
            rows.iter()
                .filter(|row| row.group == group)
                .map(|row| row.id.clone())
                .collect()
        })
        .unwrap_or_default();
    PlanRunEvent {
        event: Some(plan_run_event::Event::Failure(PlanRunFailure {
            message,
            group: group.unwrap_or_default(),
            rolled_back,
        })),
    }
}

/// The group named by a `GroupDoesNotCompile` refusal.
fn failed_group(message: &str) -> Option<&str> {
    let named = message.strip_prefix("group `")?;
    let (group, rest) = named.split_once('`')?;
    rest.starts_with(" does not compile").then_some(group)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_that_did_not_compile_is_named_by_the_refusal() {
        // Given the index's refusal for a group that did not compile at its end
        let refusal = "group `geometry` does not compile at its end, so it was rolled back: E0432";

        // When the failed group is read from it
        let group = failed_group(refusal);

        // Then it is the group the refusal names
        assert_eq!(group, Some("geometry"));
    }

    #[test]
    fn any_other_refusal_names_no_group() {
        // Given a refusal that is not about a group
        let refusal = "the plan changed on disk";

        // When the failed group is read from it
        let group = failed_group(refusal);

        // Then there is none
        assert_eq!(group, None);
    }
}
