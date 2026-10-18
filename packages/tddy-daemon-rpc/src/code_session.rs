//! `code_navigation.CodeSession` — the code pane's bidirectional stream to its worktree's index.
//!
//! The first frame is `open`, authorised exactly as every other navigation method
//! ([`WorktreeServiceImpl::resolve_listed_worktree`]). Opening asks the index daemon to **warm** the
//! worktree (`code_index.Warm`, drained here: it is the trigger, not the progress) and then follows
//! the root (`code_index.WatchRoot`), relaying every message as a `progress` event for as long as the
//! stream is open — so a reload after an edit shows here too.
//!
//! Each query is asked of the index on a task of its own, so a slow hover never delays a probe. A
//! hover, definition or references query the index answers `LOADING` is **held**: the pane is told
//! (`held`), and the query is asked again once the root reports `READY` after it was asked — or
//! answered with the root's `error` once it reports `FAILED`. A probe never waits: while loading it
//! answers `UNKNOWN`. `cancel` drops a query, held or in flight, so no answer is ever sent for it.
//!
//! Every event is sent from the one loop that owns the session's state, which reads the pane's frames
//! before anything else: a `cancel` the pane sent before the index became ready is always applied
//! before the held query is released.

use std::collections::HashMap;
use std::sync::Arc;

use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;
use tddy_rpc::Status;
use tddy_service::proto::code_navigation::code_query_answer::Result as Answered;
use tddy_service::proto::code_navigation::code_session_event::Event;
use tddy_service::proto::code_navigation::code_session_request::Frame;
use tddy_service::proto::code_navigation::{
    CodeQueryAnswer, CodeQueryHeld, CodeSessionEvent, CodeSessionRequest,
};
use tddy_worktree_service::WorktreeServiceImpl;
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinError, JoinSet};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::Channel;

use crate::code_navigation::{connect_index, web_progress, IndexChannelSource};

mod ask;

use ask::{ask_index, Query, Reply};

/// How many events a session buffers for a pane that has not read them yet.
const EVENT_BUFFER: usize = 32;

type Events = mpsc::Sender<Result<CodeSessionEvent, Status>>;

/// Serves one code session over `frames`, returning the stream its events are sent on. A session
/// that cannot open, or breaks, ends that stream with the reason as its last item.
pub(crate) fn start(
    worktrees: Arc<WorktreeServiceImpl>,
    index_daemon: Option<Arc<dyn IndexChannelSource>>,
    frames: tddy_rpc::Streaming<CodeSessionRequest>,
) -> ReceiverStream<Result<CodeSessionEvent, Status>> {
    let (events, rx) = mpsc::channel(EVENT_BUFFER);
    tokio::spawn(async move {
        if let Err(status) = run(&worktrees, index_daemon.as_ref(), frames, &events).await {
            // Fails only when the pane has already gone, and then there is no one to tell.
            let _ = events.send(Err(status)).await;
        }
    });
    ReceiverStream::new(rx)
}

/// Opens the session from its first frame, then serves it until the pane leaves.
async fn run(
    worktrees: &WorktreeServiceImpl,
    index_daemon: Option<&Arc<dyn IndexChannelSource>>,
    mut frames: tddy_rpc::Streaming<CodeSessionRequest>,
    events: &Events,
) -> Result<(), Status> {
    let Some(first) = frames.next().await else {
        return Ok(());
    };
    let Some(Frame::Open(open)) = first?.frame else {
        return Err(Status::invalid_argument(
            "a code session's first frame must be `open`",
        ));
    };
    let root = worktrees.resolve_listed_worktree(
        &open.session_token,
        &open.project_id,
        &open.worktree_path,
    )?;
    let mut client = connect_index(index_daemon).await?;
    let workspace_root = root.display().to_string();

    let warming = start_warm(client.clone(), workspace_root.clone()).await;
    let progress = client
        .watch_root(index::WatchRootRequest {
            workspace_root: workspace_root.clone(),
        })
        .await
        .map_err(tddy_service::to_rpc_status)?
        .into_inner();
    let served = Session::new(client, workspace_root, events.clone())
        .serve(frames, progress)
        .await;
    if let Some(warming) = warming {
        warming.abort();
    }
    served
}

/// Asks the index daemon to warm `workspace_root` and drains the answer on a task of its own: the
/// progress the pane sees comes from `WatchRoot`, which reports a warm that cannot start as `FAILED`
/// with its reason — so a refused warm is only logged here.
async fn start_warm(
    mut client: CodeIndexServiceClient<Channel>,
    workspace_root: String,
) -> Option<tokio::task::JoinHandle<()>> {
    let mut stream = match client
        .warm(index::WarmRequest {
            workspace_root: workspace_root.clone(),
        })
        .await
    {
        Ok(answer) => answer.into_inner(),
        Err(status) => {
            log::warn!(
                "code index warm of {workspace_root} did not start: {}",
                status.message()
            );
            return None;
        }
    };
    Some(tokio::spawn(async move {
        loop {
            match stream.message().await {
                Ok(Some(_)) => {}
                Ok(None) => return,
                Err(status) => {
                    log::warn!(
                        "code index warm of {workspace_root} broke off: {}",
                        status.message()
                    );
                    return;
                }
            }
        }
    }))
}

/// A query's task's result, with how many progress messages had arrived when it was asked.
struct Outcome {
    query: Query,
    asked_at: u64,
    reply: Result<Reply, tonic::Status>,
}

/// One open code session's state, owned by its loop.
struct Session {
    client: CodeIndexServiceClient<Channel>,
    workspace_root: String,
    events: Events,
    /// The root's state on the latest progress message, and its `error` when `FAILED`.
    state: index::IndexState,
    error: String,
    /// How many progress messages have arrived: a held query is released by a `READY` that came
    /// after it was asked, never by one it was already asked under.
    progress_seen: u64,
    /// Every query being asked; dropping the session aborts them.
    asking: JoinSet<Outcome>,
    in_flight: HashMap<u64, AbortHandle>,
    held: HashMap<u64, Query>,
}

impl Session {
    fn new(
        client: CodeIndexServiceClient<Channel>,
        workspace_root: String,
        events: Events,
    ) -> Self {
        Self {
            client,
            workspace_root,
            events,
            state: index::IndexState::Unspecified,
            error: String::new(),
            progress_seen: 0,
            asking: JoinSet::new(),
            in_flight: HashMap::new(),
            held: HashMap::new(),
        }
    }

    /// Serves frames, progress and answers until the pane leaves or the index stops following the
    /// root. Frames come first, so a `cancel` is applied before anything it raced with.
    async fn serve(
        mut self,
        mut frames: tddy_rpc::Streaming<CodeSessionRequest>,
        mut progress: tonic::Streaming<index::IndexProgress>,
    ) -> Result<(), Status> {
        loop {
            tokio::select! {
                biased;
                frame = frames.next() => match frame {
                    Some(frame) => self.on_frame(frame?).await?,
                    None => return Ok(()),
                },
                update = progress.message() => match update.map_err(tddy_service::to_rpc_status)? {
                    Some(update) => self.on_progress(update).await?,
                    None => {
                        return Err(Status::unavailable(
                            "the index daemon stopped following the worktree",
                        ))
                    }
                },
                Some(done) = self.asking.join_next_with_id(), if !self.asking.is_empty() => {
                    match done {
                        Ok((task, outcome)) => self.on_outcome(task, outcome).await?,
                        Err(failed) => self.on_task_failure(failed).await?,
                    }
                }
                () = self.events.closed() => return Ok(()),
            }
        }
    }

    async fn on_frame(&mut self, frame: CodeSessionRequest) -> Result<(), Status> {
        match frame.frame {
            Some(Frame::Query(query)) => {
                let request_id = query.request_id;
                // A query reusing an id replaces the one asked under it.
                self.forget(request_id);
                match Query::parse(query) {
                    Ok(query) => {
                        self.ask(query);
                        Ok(())
                    }
                    Err(reason) => self.answer(request_id, Answered::Error(reason)).await,
                }
            }
            Some(Frame::Cancel(cancel)) => {
                self.forget(cancel.request_id);
                Ok(())
            }
            Some(Frame::Open(_)) => Err(Status::invalid_argument("a code session is opened once")),
            None => Err(Status::invalid_argument(
                "a code session frame carries `open`, `query` or `cancel`",
            )),
        }
    }

    async fn on_progress(&mut self, update: index::IndexProgress) -> Result<(), Status> {
        self.progress_seen += 1;
        self.state = update.state();
        self.error.clone_from(&update.error);
        self.send(Event::Progress(web_progress(update))).await?;
        if matches!(
            self.state,
            index::IndexState::Ready | index::IndexState::Failed
        ) {
            let held: Vec<Query> = self.held.drain().map(|(_, query)| query).collect();
            for query in held {
                self.release(query).await?;
            }
        }
        Ok(())
    }

    async fn on_outcome(&mut self, task: tokio::task::Id, outcome: Outcome) -> Result<(), Status> {
        let request_id = outcome.query.request_id;
        // A query cancelled — or replaced — after its task finished is not answered.
        if self.in_flight.get(&request_id).map(AbortHandle::id) != Some(task) {
            return Ok(());
        }
        self.in_flight.remove(&request_id);
        match outcome.reply {
            Ok(Reply::Answer(result)) => self.answer(request_id, result).await,
            Ok(Reply::Loading) => self.hold(outcome.query, outcome.asked_at).await,
            Err(status) => {
                self.answer(request_id, Answered::Error(status.message().to_string()))
                    .await
            }
        }
    }

    /// A query's task that ended without an outcome: cancelled tasks were forgotten already; one
    /// that panicked is answered with an error so the pane does not wait for it.
    async fn on_task_failure(&mut self, failed: JoinError) -> Result<(), Status> {
        if failed.is_cancelled() {
            return Ok(());
        }
        let Some(request_id) = self
            .in_flight
            .iter()
            .find_map(|(request_id, task)| (task.id() == failed.id()).then_some(*request_id))
        else {
            return Ok(());
        };
        self.in_flight.remove(&request_id);
        log::error!(
            "code query {request_id} on {} panicked",
            self.workspace_root
        );
        self.answer(
            request_id,
            Answered::Error("the query failed inside the daemon".to_string()),
        )
        .await
    }

    /// Asks `query` of the index on a task of its own.
    fn ask(&mut self, query: Query) {
        let request_id = query.request_id;
        let client = self.client.clone();
        let workspace_root = self.workspace_root.clone();
        let asked_at = self.progress_seen;
        let task = self.asking.spawn(async move {
            let reply = ask_index(client, workspace_root, &query).await;
            Outcome {
                query,
                asked_at,
                reply,
            }
        });
        self.in_flight.insert(request_id, task);
    }

    /// The index answered `query` while loading: tell the pane once, and wait for a `READY` (or
    /// `FAILED`) that arrived after it was asked — at once, if one already has.
    async fn hold(&mut self, mut query: Query, asked_at: u64) -> Result<(), Status> {
        if !query.held {
            query.held = true;
            self.send(Event::Held(CodeQueryHeld {
                request_id: query.request_id,
            }))
            .await?;
        }
        if self.progress_seen > asked_at {
            self.release(query).await
        } else {
            self.held.insert(query.request_id, query);
            Ok(())
        }
    }

    /// A held query, once the root's state may settle it: asked again when ready, answered with the
    /// reason when failed, held on otherwise.
    async fn release(&mut self, query: Query) -> Result<(), Status> {
        match self.state {
            index::IndexState::Ready => {
                self.ask(query);
                Ok(())
            }
            index::IndexState::Failed => {
                let reason = format!("the code index failed: {}", self.error);
                self.answer(query.request_id, Answered::Error(reason)).await
            }
            _ => {
                self.held.insert(query.request_id, query);
                Ok(())
            }
        }
    }

    /// Drops `request_id`, held or in flight, so it is never answered.
    fn forget(&mut self, request_id: u64) {
        if let Some(task) = self.in_flight.remove(&request_id) {
            task.abort();
        }
        self.held.remove(&request_id);
    }

    async fn answer(&self, request_id: u64, result: Answered) -> Result<(), Status> {
        self.send(Event::Answer(CodeQueryAnswer {
            request_id,
            result: Some(result),
        }))
        .await
    }

    async fn send(&self, event: Event) -> Result<(), Status> {
        self.events
            .send(Ok(CodeSessionEvent { event: Some(event) }))
            .await
            .map_err(|_| Status::cancelled("the code pane left its code session"))
    }
}
