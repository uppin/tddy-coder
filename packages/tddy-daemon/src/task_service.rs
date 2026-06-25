//! TaskServiceImpl — maps the `tasks.TaskService` RPC trait to the shared `TaskRegistry`.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::tasks::{
    task_list_event, CancelTaskRequest, CancelTaskResponse, GetTaskRequest, GetTaskResponse,
    ListTasksRequest, ListTasksResponse, SendInputRequest, SendInputResponse, TaskChannelInfo,
    TaskInfo, TaskListEvent, TaskOutputEvent, TaskService, TaskStatusProto, WatchTaskListRequest,
    WatchTaskRequest,
};
use tddy_task::TaskRegistry;
use tokio_stream::wrappers::ReceiverStream;

/// Resolver that maps a session token to the authenticated GitHub login.
/// Matches the pattern used by `VmServiceImpl` to avoid a circular dep.
pub type SessionUserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Implementation of the `tasks.TaskService` RPC service.
pub struct TaskServiceImpl {
    registry: TaskRegistry,
    user_resolver: SessionUserResolver,
}

impl TaskServiceImpl {
    pub fn new(registry: TaskRegistry, user_resolver: SessionUserResolver) -> Self {
        Self {
            registry,
            user_resolver,
        }
    }

    fn authenticate(&self, token: &str) -> Result<String, Status> {
        (self.user_resolver)(token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session token"))
    }
}

fn task_status_to_proto(status: &tddy_task::TaskStatus) -> i32 {
    match status {
        tddy_task::TaskStatus::Pending => TaskStatusProto::TaskStatusPending as i32,
        tddy_task::TaskStatus::Running => TaskStatusProto::TaskStatusRunning as i32,
        tddy_task::TaskStatus::Completed { .. } => TaskStatusProto::TaskStatusCompleted as i32,
        tddy_task::TaskStatus::Failed { .. } => TaskStatusProto::TaskStatusFailed as i32,
        tddy_task::TaskStatus::Cancelled => TaskStatusProto::TaskStatusCancelled as i32,
    }
}

fn handle_to_proto(handle: &tddy_task::TaskHandle) -> TaskInfo {
    let status = handle.status();
    let exit_code = match &status {
        tddy_task::TaskStatus::Completed { exit_code } => exit_code.unwrap_or(0),
        _ => 0,
    };
    let error_message = match &status {
        tddy_task::TaskStatus::Failed { message } => message.clone(),
        _ => String::new(),
    };
    let channels = handle
        .channels()
        .iter()
        .map(|ch| TaskChannelInfo {
            channel_id: ch.channel_id.clone(),
            name: ch.name.clone(),
            kind: match ch.kind {
                tddy_task::ChannelKind::Stdout => 1,
                tddy_task::ChannelKind::Stderr => 2,
                tddy_task::ChannelKind::Combined => 3,
            },
            accepts_input: ch.accepts_input(),
        })
        .collect();
    let result_json = handle
        .result_json
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();
    TaskInfo {
        task_id: handle.id.0.clone(),
        kind: handle.kind.clone(),
        status: task_status_to_proto(&status),
        exit_code,
        error_message,
        created_unix_ms: handle.created_unix_ms,
        channels,
        result_json,
    }
}

fn registry_event_to_list_event(event: tddy_task::TaskRegistryEvent) -> TaskListEvent {
    match event {
        tddy_task::TaskRegistryEvent::Added(h) => TaskListEvent {
            is_snapshot: false,
            event: Some(task_list_event::Event::TaskAdded(handle_to_proto(&h))),
        },
        tddy_task::TaskRegistryEvent::Updated(h) => TaskListEvent {
            is_snapshot: false,
            event: Some(task_list_event::Event::TaskUpdated(handle_to_proto(&h))),
        },
        tddy_task::TaskRegistryEvent::Removed(id) => TaskListEvent {
            is_snapshot: false,
            event: Some(task_list_event::Event::TaskRemoved(id.0)),
        },
    }
}

#[async_trait]
impl TaskService for TaskServiceImpl {
    type WatchTaskStream = ReceiverStream<Result<TaskOutputEvent, Status>>;
    type WatchTaskListStream = ReceiverStream<Result<TaskListEvent, Status>>;

    async fn list_tasks(
        &self,
        request: Request<ListTasksRequest>,
    ) -> Result<Response<ListTasksResponse>, Status> {
        let req = request.into_inner();
        let _user = self.authenticate(&req.session_token)?;
        // TODO(green): filter by session_id once per-session scoping is implemented.
        let tasks = self.registry.list().await;
        let infos = tasks.iter().map(|h| handle_to_proto(h)).collect();
        Ok(Response::new(ListTasksResponse { tasks: infos }))
    }

    async fn get_task(
        &self,
        request: Request<GetTaskRequest>,
    ) -> Result<Response<GetTaskResponse>, Status> {
        let req = request.into_inner();
        let _user = self.authenticate(&req.session_token)?;
        let task_id = tddy_task::TaskId(req.task_id);
        let handle = self
            .registry
            .get(&task_id)
            .await
            .ok_or_else(|| Status::not_found("task not found"))?;
        Ok(Response::new(GetTaskResponse {
            task: Some(handle_to_proto(&handle)),
        }))
    }

    async fn watch_task(
        &self,
        request: Request<WatchTaskRequest>,
    ) -> Result<Response<Self::WatchTaskStream>, Status> {
        let req = request.into_inner();
        let _user = self.authenticate(&req.session_token)?;

        // Reject remote daemon_instance_id — streaming forward is deferred.
        if !req.daemon_instance_id.is_empty() {
            return Err(Status::failed_precondition(
                "WatchTask does not support remote daemon_instance_id in this version",
            ));
        }

        let task_id = tddy_task::TaskId(req.task_id.clone());
        let handle = self
            .registry
            .get(&task_id)
            .await
            .ok_or_else(|| Status::not_found("task not found"))?;

        // Pick the channel to watch.
        let channel = if req.channel_id.is_empty() {
            handle.channels().first().cloned()
        } else {
            handle.channel(&req.channel_id)
        };
        let channel = channel.ok_or_else(|| Status::not_found("channel not found"))?;

        let (tx, rx) = tokio::sync::mpsc::channel(256);
        let channel_id = channel.channel_id.clone();

        tokio::spawn(async move {
            // Subscribe FIRST so we don't miss any live bytes.
            let mut live_rx = channel.subscribe();
            // Subscribe to status changes before the live-stream loop so a terminal transition
            // that fires while live_rx.recv() is blocking unblocks the select immediately.
            let mut status_rx = handle.status_watch();

            // Replay the capture buffer.
            let replay = channel.replay_capture();
            if !replay.is_empty() {
                let _ = tx
                    .send(Ok(TaskOutputEvent {
                        channel_id: channel_id.clone(),
                        data: replay,
                        is_replay: true,
                        status: 0,
                    }))
                    .await;
            }

            // Stream live bytes.
            loop {
                let status = handle.status();
                if status.is_terminal() {
                    // Emit final event with terminal status.
                    let _ = tx
                        .send(Ok(TaskOutputEvent {
                            channel_id: channel_id.clone(),
                            data: vec![],
                            is_replay: false,
                            status: task_status_to_proto(&status),
                        }))
                        .await;
                    break;
                }
                tokio::select! {
                    recv_result = live_rx.recv() => {
                        match recv_result {
                            Ok(data) => {
                                let _ = tx
                                    .send(Ok(TaskOutputEvent {
                                        channel_id: channel_id.clone(),
                                        data: data.to_vec(),
                                        is_replay: false,
                                        status: 0,
                                    }))
                                    .await;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                log::warn!("WatchTask: lagged by {n} messages on channel {channel_id}");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                // Channel closed — emit terminal event and finish.
                                let terminal_status = handle.status();
                                let _ = tx
                                    .send(Ok(TaskOutputEvent {
                                        channel_id: channel_id.clone(),
                                        data: vec![],
                                        is_replay: false,
                                        status: task_status_to_proto(&terminal_status),
                                    }))
                                    .await;
                                break;
                            }
                        }
                    }
                    _ = status_rx.changed() => {
                        // Status changed — loop back to check is_terminal() at the top.
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn cancel_task(
        &self,
        request: Request<CancelTaskRequest>,
    ) -> Result<Response<CancelTaskResponse>, Status> {
        let req = request.into_inner();
        let _user = self.authenticate(&req.session_token)?;
        let task_id = tddy_task::TaskId(req.task_id);
        let found = self.registry.cancel_task(&task_id).await;
        if found {
            Ok(Response::new(CancelTaskResponse {
                ok: true,
                message: String::new(),
            }))
        } else {
            Err(Status::not_found("task not found"))
        }
    }

    async fn send_input(
        &self,
        request: Request<SendInputRequest>,
    ) -> Result<Response<SendInputResponse>, Status> {
        let req = request.into_inner();
        let _user = self.authenticate(&req.session_token)?;
        let task_id = tddy_task::TaskId(req.task_id);
        let handle = self
            .registry
            .get(&task_id)
            .await
            .ok_or_else(|| Status::not_found("task not found"))?;
        let channel = handle
            .channel(&req.channel_id)
            .ok_or_else(|| Status::not_found("channel not found"))?;
        if !channel.accepts_input() {
            return Err(Status::failed_precondition("channel does not accept input"));
        }
        let sent = channel.send_input(bytes::Bytes::from(req.data));
        Ok(Response::new(SendInputResponse {
            ok: sent,
            message: if sent {
                String::new()
            } else {
                "stdin receiver closed".to_string()
            },
        }))
    }

    async fn watch_task_list(
        &self,
        request: Request<WatchTaskListRequest>,
    ) -> Result<Response<Self::WatchTaskListStream>, Status> {
        let req = request.into_inner();
        let _user = self.authenticate(&req.session_token)?;

        if !req.daemon_instance_id.is_empty() {
            return Err(Status::failed_precondition(
                "WatchTaskList does not support remote daemon_instance_id in this version",
            ));
        }

        let (snapshot, mut rx) = self.registry.list_and_subscribe().await;
        let (tx, receiver) = tokio::sync::mpsc::channel(256);

        tokio::spawn(async move {
            // Emit snapshot events first.
            for handle in &snapshot {
                let event = TaskListEvent {
                    is_snapshot: true,
                    event: Some(task_list_event::Event::TaskAdded(handle_to_proto(handle))),
                };
                if tx.send(Ok(event)).await.is_err() {
                    return;
                }
            }

            // Stream live events until the client disconnects or the registry shuts down.
            loop {
                match rx.recv().await {
                    Ok(registry_event) => {
                        if tx
                            .send(Ok(registry_event_to_list_event(registry_event)))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("WatchTaskList: lagged by {n} events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(receiver)))
    }
}
