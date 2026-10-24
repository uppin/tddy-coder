//! A streamed start, run as a task of the daemon's registry.
//!
//! The task names itself to the caller first (`StartSessionEvent.started`), runs the start with its
//! cancellation token, and ends the stream with the start's result — or, when the task was
//! cancelled, with `CANCELLED`.

use async_trait::async_trait;
use tddy_rpc::Status;
use tddy_service::proto::session::start_session_event::Event as StartSessionEventKind;
use tddy_service::proto::session::{StartSessionEvent, StartSessionRequest, StartTaskStarted};
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_task::{TaskBody, TaskContext, TaskStatus};
use tokio::sync::mpsc::UnboundedSender;

use crate::launch_ports::LaunchSessions;
use crate::start_keepalive::{
    with_activity_keepalive, STARTING_ACTIVITY, START_KEEPALIVE_INTERVAL,
};

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
        let ended = run_start(&service, req, &tx, &ctx.cancel_token()).await;
        let status = match &ended {
            Ok(()) => TaskStatus::Completed { exit_code: Some(0) },
            Err(status) if status.code == tddy_rpc::Code::Cancelled => TaskStatus::Cancelled,
            Err(status) => TaskStatus::Failed {
                message: status.message.clone(),
            },
        };
        // The terminal event goes out last: every progress event ahead of it was forwarded first.
        if let Err(status) = ended {
            let _ = tx.send(Err(status));
        }
        status
    }
}

/// Run the start, reporting into `tx`; on success its result event is sent here, on failure the
/// status is returned for the caller to end the stream with.
async fn run_start(
    service: &LaunchSessions,
    req: StartSessionRequest,
    tx: &StartEvents,
    cancel: &tddy_task::CancellationToken,
) -> Result<(), Status> {
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
    let response = started?.into_inner();
    let _ = tx.send(Ok(StartSessionEvent {
        event: Some(StartSessionEventKind::Result(response)),
    }));
    Ok(())
}
