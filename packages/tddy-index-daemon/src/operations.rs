//! What each RPC actually does, so `service.rs` stays a routing table.
//!
//! Three facts shape every function here.
//!
//! The restructuring library is **synchronous** and reaches its language server through
//! `Handle::current().block_on`, so its entry points run inside [`tokio::task::spawn_blocking`]
//! with the runtime still driving on another thread. Nothing here calls one of them directly from
//! an async context.
//!
//! A long operation runs under a [`CancellationToken`], and the only way this service learns its
//! caller has gone away is a **send failing into that request's dropped receiver** —
//! `tddy_rpc::RpcService` carries no cancellation surface at all (recorded in
//! `docs/dev/todo/2026-09-15-rpcservice-has-no-cancellation-surface.md`). So the progress sink
//! cancels the token on a failed send, and that is the disconnect signal.
//!
//! Operations that touch a root's tree or its `.restructure/` state take that root's queue first
//! (see [`WorkspaceIndex::hold`]), because the journal is keyed by root with no lock file.

use std::path::{Path, PathBuf};

use tddy_code_restructuring::runner::{self, Command, Options};
use tddy_rpc::Status;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::apply::apply_plan;
use crate::index::WorkspaceIndex;
use crate::proto::code_index::{
    restructure_event, ApplyRequest, CheckRequest, Finding, IndexProgress, RestructureEvent,
    WorkspacesResponse,
};
use crate::service::EventStream;
use crate::status::status_of;

/// The sending half of one request's [`EventStream`].
pub(crate) type EventSender<T> = mpsc::Sender<Result<T, Status>>;

/// How many events one request may have in flight before its producer waits for the consumer.
///
/// Deep enough that a burst of per-operation events does not stall an apply, shallow enough that a
/// consumer which has stopped reading is noticed while the run still has work left to cancel.
const EVENT_CHANNEL_CAPACITY: usize = 64;

/// Load a workspace root's crate graph and report progress until it is ready.
///
/// Idempotent, as the schema says: a root this process already holds an index for is answered out
/// of [`tddy_lsp::LspRegistry`] without spawning anything, and the stream ends immediately.
pub(crate) async fn serve_warm(
    index: &WorkspaceIndex,
    workspace_root: &str,
) -> Result<EventStream<IndexProgress>, Status> {
    let root = WorkspaceIndex::workspace_root_of(workspace_root)?;
    let (events, stream) = event_stream();
    let index = index.clone();

    tokio::spawn(async move {
        let loading = IndexProgress {
            line: format!("loading the crate graph at {}", root.display()),
            ..IndexProgress::default()
        };
        if events.send(Ok(loading)).await.is_err() {
            return;
        }
        // TODO(tddy-lsp, tddy-code-restructuring): `ready` is "this root has a live language
        // server holding its index", which is the furthest this crate can observe. The
        // hover-until-answered probe that decides the question properly is
        // `RustBackend::ensure_indexed`, which is private and takes a document URI; and the
        // server's own `$/progress` can only be read through `LspClient::drain_notifications`,
        // which *consumes* the queue an operation on the same root is folding. Forwarding phases
        // and percentages needs one of the two exposed non-destructively.
        let reported = match index.client_for(&root).await {
            Ok(_) => Ok(IndexProgress {
                line: format!("{} is served from a warm index", root.display()),
                ready: true,
                ..IndexProgress::default()
            }),
            Err(status) => Err(status),
        };
        let _ = events.send(reported).await;
    });

    Ok(stream)
}

/// Which workspace roots this process currently holds an index for.
pub(crate) async fn serve_workspaces(index: &WorkspaceIndex) -> WorkspacesResponse {
    WorkspacesResponse {
        workspaces: index.warm_workspaces().await,
    }
}

/// Everything wrong with a plan, without writing anything.
pub(crate) async fn serve_check(
    index: &WorkspaceIndex,
    request: CheckRequest,
) -> Result<EventStream<RestructureEvent>, Status> {
    let root = WorkspaceIndex::workspace_root_of(&request.workspace_root)?;
    let plan = plan_path(&root, &request.plan)?;
    let deep = request.deep;
    let budget = file_budget(request.file_budget);
    let (events, stream) = event_stream();
    let index = index.clone();

    tokio::spawn(async move {
        let _queued = index.hold(&root).await;
        // A shallow check is answered from the text alone, so it neither waits for an index nor
        // needs a server — which is what makes it worth having separately from an apply.
        let client = if deep {
            match index.client_for(&root).await {
                Ok(client) => Some(client),
                Err(status) => {
                    let _ = events.send(Err(status)).await;
                    return;
                }
            }
        } else {
            None
        };

        let cancel = CancellationToken::new();
        let options = Options {
            command: Command::Check,
            target: Some(plan),
            deep,
            budget,
            // A deep check waits for the index, and what it says while it waits belongs to the
            // caller who is waiting with it. The account carries what a check produces that is not
            // a finding — a cross-crate move's blast radius and the file-budget report — which is
            // prose about the run rather than a defect in the plan, and `note` is the field this
            // schema has for exactly that.
            progress: progress_into(events.clone(), cancel.clone()),
            account: note_into(events.clone(), cancel.clone()),
            ..Options::default()
        };

        let checked = {
            let root = root.clone();
            let cancel = cancel.clone();
            tokio::task::spawn_blocking(move || runner::check(&root, options, client, cancel)).await
        };

        // Findings are results, not refusals: `runner::check` hands them back as values and this
        // is where they become the events the schema declares. A plan with findings is a check
        // that worked, so the stream ends with them rather than with an error.
        let Some(findings) = reported(&events, checked, "check").await else {
            return;
        };
        for finding in findings {
            if events.send(Ok(finding_event(&finding))).await.is_err() {
                return;
            }
        }
    });

    Ok(stream)
}

/// Execute a plan against the working tree, one event per operation.
pub(crate) async fn serve_apply(
    index: &WorkspaceIndex,
    request: ApplyRequest,
) -> Result<EventStream<RestructureEvent>, Status> {
    let root = WorkspaceIndex::workspace_root_of(&request.workspace_root)?;
    let plan = plan_path(&root, &request.plan)?;
    let options = Options {
        command: Command::Apply,
        target: Some(plan),
        dry_run: request.dry_run,
        resume: request.resume,
        from: request.from.map(|from| from as usize),
        stop_after: request.stop_after.map(|stop_after| stop_after as usize),
        ..Options::default()
    };
    let (events, stream) = event_stream();
    let index = index.clone();

    tokio::spawn(async move {
        let _queued = index.hold(&root).await;
        let client = match index.client_for(&root).await {
            Ok(client) => client,
            Err(status) => {
                let _ = events.send(Err(status)).await;
                return;
            }
        };

        let cancel = CancellationToken::new();
        let progress = progress_into(events.clone(), cancel.clone());
        let applied = {
            let events = events.clone();
            tokio::task::spawn_blocking(move || {
                apply_plan(&root, &options, client, cancel, progress, &events)
            })
            .await
        };
        reported(&events, applied, "apply").await;
    });

    Ok(stream)
}

/// One request's event channel and the stream its caller reads.
pub(crate) fn event_stream<T>() -> (EventSender<T>, EventStream<T>) {
    let (events, receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
    (events, ReceiverStream::new(receiver))
}

/// What an operation produced, having reported its refusal into its own stream if it had one.
///
/// A refusal is the last item in the stream; a stream that ends without one *is* the success
/// answer, since there is no terminal "it worked" event to invent beyond what the operation
/// returned and streamed.
async fn reported<T>(
    events: &EventSender<RestructureEvent>,
    outcome: Result<tddy_code_restructuring::Result<T>, tokio::task::JoinError>,
    operation: &str,
) -> Option<T> {
    let refusal = match outcome {
        Ok(Ok(produced)) => return Some(produced),
        Ok(Err(refusal)) => status_of(&refusal),
        Err(failure) => joined(operation, &failure),
    };
    log::debug!(target: "tddy_index_daemon::operations", "{operation} refused: {}", refusal.message());
    let _ = events.send(Err(refusal)).await;
    None
}

/// A progress sink that streams each line to the caller who asked for the work, and cancels that
/// work when the caller is no longer there to hear it.
///
/// Sent with `blocking_send` rather than `try_send`: the sink is only ever called from inside
/// [`tokio::task::spawn_blocking`] — every caller of it is the synchronous restructuring engine —
/// so blocking this thread is exactly the backpressure that keeps a fast producer from outrunning
/// a slow consumer, and no line is dropped. A failed send means every receiver is gone, which is
/// the one disconnect signal a handler gets, so the work stops.
pub(crate) fn progress_into(
    events: EventSender<RestructureEvent>,
    cancel: CancellationToken,
) -> tddy_code_restructuring::backends::rust::ProgressSink {
    std::sync::Arc::new(move |line: &str| {
        let event = indexing_event(line);
        if events.blocking_send(Ok(event)).is_err() {
            log::debug!(
                target: "tddy_index_daemon::operations",
                "nobody is listening to this run any more; cancelling it"
            );
            cancel.cancel();
        }
    })
}

/// A sink that streams each line as a note to the caller who asked for the work, cancelling that
/// work when the caller is no longer there to hear it.
///
/// The same send-and-cancel discipline as [`progress_into`], and a different event because the two
/// lines are different claims: one is the language server saying how far it has got, and a note is
/// the run itself saying something about an operation that is not a finding.
pub(crate) fn note_into(
    events: EventSender<RestructureEvent>,
    cancel: CancellationToken,
) -> tddy_code_restructuring::backends::rust::ProgressSink {
    std::sync::Arc::new(move |line: &str| {
        if events.blocking_send(Ok(note_event(line))).is_err() {
            log::debug!(
                target: "tddy_index_daemon::operations",
                "nobody is listening to this run any more; cancelling it"
            );
            cancel.cancel();
        }
    })
}

/// One thing a check found, attributed to the operation that caused it.
pub(crate) fn finding_event(
    finding: &tddy_code_restructuring::runner::Finding,
) -> RestructureEvent {
    RestructureEvent {
        event: Some(restructure_event::Event::Finding(Finding {
            operation: finding.operation as u32,
            detail: finding.detail.clone(),
        })),
    }
}

/// One line the run has to say about itself that is neither progress nor a finding.
pub(crate) fn note_event(line: &str) -> RestructureEvent {
    RestructureEvent {
        event: Some(restructure_event::Event::Note(line.to_string())),
    }
}

/// One line of index progress, as the event a stream carries it in.
pub(crate) fn indexing_event(line: &str) -> RestructureEvent {
    RestructureEvent {
        event: Some(restructure_event::Event::Indexing(IndexProgress {
            line: line.to_string(),
            ..IndexProgress::default()
        })),
    }
}

/// The plan a request names: absolute as given, or relative to the root it named, as the schema
/// states. Never relative to this process's own directory — it serves several trees.
pub(crate) fn plan_path(root: &Path, plan: &str) -> Result<PathBuf, Status> {
    if plan.trim().is_empty() {
        return Err(Status::invalid_argument("the request names no plan"));
    }
    let named = Path::new(plan);
    Ok(if named.is_absolute() {
        named.to_path_buf()
    } else {
        root.join(named)
    })
}

/// The file-length budget a request asked for, if it asked for one.
///
/// Zero is "no budget report": proto3 cannot tell an unset `uint32` from a zero one, and a budget
/// of zero lines would report every file the plan names as over it.
fn file_budget(lines: u32) -> Option<usize> {
    (lines > 0).then_some(lines as usize)
}

/// A blocking operation that did not finish because its thread came apart — a defect in this host,
/// never in the request.
pub(crate) fn joined(operation: &str, failure: &tokio::task::JoinError) -> Status {
    Status::internal(format!("the {operation} task did not complete: {failure}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    /// The production configuration, not an approximation of it: a runtime is driving, and the
    /// sink is called from the blocking thread the synchronous engine runs on.
    #[tokio::test(flavor = "multi_thread")]
    async fn streams_a_progress_line_from_the_blocking_thread_the_engine_runs_on() {
        // Given a sink into one request's stream
        let (events, mut stream) = event_stream();
        let cancel = CancellationToken::new();
        let sink = progress_into(events, cancel.clone());

        // When the engine reports a line from inside its blocking work
        tokio::task::spawn_blocking(move || sink("loading crate graph 12%"))
            .await
            .expect("the blocking work completes");

        // Then the caller who asked for the work hears it, and the work carries on
        assert_eq!(
            stream
                .next()
                .await
                .expect("a streamed event")
                .map_err(|status| status.to_string()),
            Ok(indexing_event("loading crate graph 12%"))
        );
        assert!(
            !cancel.is_cancelled(),
            "a heard line must not stop the work"
        );
    }

    /// The only disconnect signal a handler gets: `tddy_rpc::RpcService` has no cancellation
    /// surface, so a send into a dropped receiver is what says the caller has gone.
    #[tokio::test(flavor = "multi_thread")]
    async fn cancels_the_work_when_nobody_is_left_to_hear_its_progress() {
        // Given a sink whose caller has stopped listening
        let (events, stream) = event_stream::<RestructureEvent>();
        let cancel = CancellationToken::new();
        let sink = progress_into(events, cancel.clone());
        drop(stream);

        // When the engine reports a line from inside its blocking work
        tokio::task::spawn_blocking(move || sink("loading crate graph 12%"))
            .await
            .expect("the blocking work completes");

        // Then the work is cancelled rather than run to completion for nobody
        assert!(
            cancel.is_cancelled(),
            "a run nobody is waiting for must be cancelled"
        );
    }

    #[test]
    fn reads_a_plan_named_relatively_against_the_root_it_came_with() {
        // Given a plan named relative to its workspace root
        let root = Path::new("/trees/one");

        // When it is resolved
        let plan = plan_path(root, "plans/carve.jsonl").expect("a relative plan resolves");

        // Then it is resolved against that root, not against this process's directory
        assert_eq!(plan, Path::new("/trees/one/plans/carve.jsonl"));
    }

    #[test]
    fn keeps_a_plan_named_absolutely_as_it_was_given() {
        // Given a plan named absolutely
        let root = Path::new("/trees/one");

        // When it is resolved
        let plan = plan_path(root, "/elsewhere/carve.jsonl").expect("an absolute plan resolves");

        // Then the root is not prepended to it
        assert_eq!(plan, Path::new("/elsewhere/carve.jsonl"));
    }

    #[test]
    fn reads_a_zero_file_budget_as_no_budget_report_at_all() {
        // Given the budget a request that asked for none carries
        // When it is read
        // Then no report is asked for, rather than one that flags every file
        assert_eq!(file_budget(0), None);
        assert_eq!(file_budget(500), Some(500));
    }
}
