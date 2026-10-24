//! A streamed start, run as a task of the daemon's registry.
//!
//! The task names itself to the caller first (`StartSessionEvent.started`), runs the start with its
//! cancellation token, and ends the stream with the start's result — or, when the task was
//! cancelled, with `CANCELLED`.
//!
//! A cancel that lands while a step past the clone is already running (a worktree being cut, an
//! agent being spawned) does not interrupt that step: the start finishes it, and the session it
//! created is then deleted — its agent stopped, its worktree and its directory removed — exactly as
//! a Delete would, before the stream ends `CANCELLED`.

use async_trait::async_trait;
use tddy_rpc::{Request, Status};
use tddy_service::proto::session::start_session_event::Event as StartSessionEventKind;
use tddy_service::proto::session::{
    DeleteSessionRequest, StartSessionEvent, StartSessionRequest, StartSessionResponse,
    StartTaskStarted,
};
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_task::{TaskBody, TaskContext, TaskStatus};
use tokio::sync::mpsc::UnboundedSender;

use crate::launch_ports::LaunchSessions;
use crate::start_keepalive::{
    with_activity_keepalive, STARTING_ACTIVITY, START_KEEPALIVE_INTERVAL,
};
use crate::svc_start_session_core::START_CANCELLED;

/// The `kind` a start task is registered under.
pub(crate) const START_TASK_KIND: &str = "start_session";

type StartEvents = UnboundedSender<Result<StartSessionEvent, Status>>;

/// One streamed start: what runs, and where its events go.
pub(crate) struct StartTask {
    pub(crate) service: LaunchSessions,
    pub(crate) req: StartSessionRequest,
    pub(crate) tx: StartEvents,
    pub(crate) daemon_instance_id: String,
}

#[async_trait]
impl TaskBody for StartTask {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let StartTask {
            service,
            req,
            tx,
            daemon_instance_id,
        } = *self;
        // First, before any progress: the caller learns what to cancel before anything can be slow.
        let _ = tx.send(Ok(StartSessionEvent {
            event: Some(StartSessionEventKind::Started(StartTaskStarted {
                task_id: ctx.task_id().0,
                daemon_instance_id,
            })),
        }));
        let cancel = ctx.cancel_token();
        let session_token = req.session_token.clone();
        let started = run_start(&service, req, &tx, &cancel).await;
        let ended = match started {
            Ok(response) if cancel.is_cancelled() => {
                Err(undo_a_cancelled_start(&service, &response, session_token).await)
            }
            Ok(response) => {
                let _ = tx.send(Ok(StartSessionEvent {
                    event: Some(StartSessionEventKind::Result(response)),
                }));
                Ok(())
            }
            Err(status) => Err(status),
        };
        let status = task_status_of(&ended);
        // The terminal event goes out last: every progress event ahead of it was forwarded first.
        if let Err(status) = ended {
            let _ = tx.send(Err(status));
        }
        status
    }
}

/// What the registry records for a start that ended this way.
fn task_status_of(ended: &Result<(), Status>) -> TaskStatus {
    match ended {
        Ok(()) => TaskStatus::Completed { exit_code: Some(0) },
        Err(status) if status.code == tddy_rpc::Code::Cancelled => TaskStatus::Cancelled,
        Err(status) => TaskStatus::Failed {
            message: status.message.clone(),
        },
    }
}

/// The start finished a step the cancel could not interrupt: delete the session it created, so a
/// cancelled start leaves no session directory, worktree or agent behind. Returns the status the
/// stream ends with — `CANCELLED`, unless the session could not be removed.
async fn undo_a_cancelled_start(
    service: &LaunchSessions,
    created: &StartSessionResponse,
    session_token: String,
) -> Status {
    if created.session_id.is_empty() {
        return Status::cancelled(START_CANCELLED);
    }
    let deleted = service
        .delete_session_at_session_coordinate(Request::direct(DeleteSessionRequest {
            session_id: created.session_id.clone(),
            session_token,
        }))
        .await;
    match deleted {
        Ok(_) => Status::cancelled(START_CANCELLED),
        Err(status) => {
            log::warn!(
                "StreamStartSession: the cancelled start's session {} could not be removed: {status}",
                created.session_id
            );
            Status::internal(format!(
                "start cancelled, but its session {} could not be removed: {}",
                created.session_id, status.message
            ))
        }
    }
}

/// Run the start, forwarding its progress into `tx`, and return what it answered — the caller
/// decides whether that is sent as the result or undone.
async fn run_start(
    service: &LaunchSessions,
    req: StartSessionRequest,
    tx: &StartEvents,
    cancel: &tddy_task::CancellationToken,
) -> Result<StartSessionResponse, Status> {
    let (conn_progress_tx, mut conn_progress_rx) =
        tokio::sync::mpsc::unbounded_channel::<Result<StartSessionEvent, Status>>();
    let session_tx = tx.clone();
    // Progress travels through its own task, so progress reaches the client while the host is still
    // materializing, rather than all at once after the start completes.
    let forwarding = tokio::spawn(async move {
        while let Some(item) = conn_progress_rx.recv().await {
            if session_tx.send(item).is_err() {
                break;
            }
        }
    });
    let sink = AttachmentProgressSink::streaming(conn_progress_tx);
    // Kept alive for the whole start, not only while a project is provisioned: a worktree cut or an
    // index build can be quiet for longer than the relay tolerates.
    let started = with_activity_keepalive(
        &sink,
        START_KEEPALIVE_INTERVAL,
        STARTING_ACTIVITY,
        service.start_session_core_cancellable(req, &sink, cancel),
    )
    .await;
    // Dropping the sink closes the channel the forwarding task drains, so the terminal event cannot
    // overtake a phase's end.
    drop(sink);
    if let Err(err) = forwarding.await {
        log::warn!(
            "StreamStartSession: the start's progress forwarding task ended abnormally: {err}"
        );
    }
    Ok(started?.into_inner())
}
