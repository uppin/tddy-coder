//! `tddy-coder --acp`: expose the TDD [`WorkflowEngine`] as a standard ACP agent over stdio.
//!
//! An ACP client drives this agent with `initialize` / `session/new` / `session/prompt` /
//! `session/load`. Each `prompt` runs one turn of the selected workflow recipe against the selected
//! coding backend, streaming the workflow's agent output back as `session/update` notifications and
//! returning a [`acp::StopReason`] when the turn ends.
//!
//! Threading: the ACP SDK is `?Send` and needs a current-thread runtime + `LocalSet` (physical fd 1
//! is the JSON-RPC channel — never `println!` here). [`WorkflowEngine`] is `Send`/multi-thread, so
//! each prompt runs the engine on a dedicated worker thread and streams events back over channels,
//! the same bridge pattern proven in `tddy_core::backend::acp`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use agent_client_protocol::{self as acp, Client as _};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};

use tddy_acp::mapping::{
    execution_status_to_stop_reason, presenter_event_to_session_update,
    progress_event_to_session_update,
};
use tddy_core::output::{create_session_dir_with_id, SESSIONS_SUBDIR};
use tddy_core::presenter::WorkflowEvent;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::workflow::session::workflow_engine_storage_dir;
use tddy_core::{PresenterEvent, SharedBackend, WorkflowEngine, WorkflowRecipe};

/// Message from an [`AcpAgent`] method to the background task that owns the ACP connection.
enum Outbound {
    /// Send a `session/update` notification and wait for it to be delivered.
    Notify(acp::SessionNotification, oneshot::Sender<()>),
}

/// ACP agent backed by a [`WorkflowEngine`]. One instance serves all sessions of one process.
struct AcpAgent {
    /// Coding backend (`--agent`), created once and reused across sessions.
    backend: SharedBackend,
    /// Workflow recipe (`--recipe`), created once and reused across sessions.
    recipe: Arc<dyn WorkflowRecipe>,
    /// Tddy data root; sessions live under `{data_dir}/sessions/<session_id>/`.
    data_dir: PathBuf,
    /// Channel to the background task that owns the connection (for outbound notifications).
    outbound_tx: mpsc::UnboundedSender<Outbound>,
}

impl AcpAgent {
    fn session_dir(&self, session_id: &str) -> PathBuf {
        self.data_dir.join(SESSIONS_SUBDIR).join(session_id)
    }

    /// Send a `session/update` to the client and await delivery.
    async fn notify(
        &self,
        session_id: &acp::SessionId,
        update: acp::SessionUpdate,
    ) -> Result<(), acp::Error> {
        let (tx, rx) = oneshot::channel();
        self.outbound_tx
            .send(Outbound::Notify(
                acp::SessionNotification::new(session_id.clone(), update),
                tx,
            ))
            .map_err(|_| acp::Error::internal_error())?;
        rx.await.map_err(|_| acp::Error::internal_error())
    }
}

/// Collect the text of a prompt's content blocks into a single string.
fn prompt_text(blocks: &[acp::ContentBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if let acp::ContentBlock::Text(t) = block {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&t.text);
        }
    }
    out
}

/// Outcome of running one workflow turn on the worker thread.
type TurnResult = Result<acp::StopReason, String>;

/// Run one workflow turn on a dedicated worker thread.
///
/// Streams the workflow's agent output / progress as ACP session updates over `update_tx` (live,
/// via a forwarder thread) and reports the terminal stop reason over `done_tx`.
fn spawn_turn(
    backend: SharedBackend,
    recipe: Arc<dyn WorkflowRecipe>,
    storage_dir: PathBuf,
    session_id: String,
    prompt: String,
    update_tx: mpsc::UnboundedSender<acp::SessionUpdate>,
    done_tx: oneshot::Sender<TurnResult>,
) {
    std::thread::spawn(move || {
        if let Err(e) = std::fs::create_dir_all(&storage_dir) {
            let _ = done_tx.send(Err(format!("create session storage dir: {e}")));
            return;
        }

        // The recipe's hooks forward agent output / progress to this channel while a task runs.
        let (event_tx, event_rx) = std::sync::mpsc::channel::<WorkflowEvent>();

        // Forward workflow events to ACP session updates on their own thread: the engine thread is
        // blocked in `block_on` while the backend streams, so draining must happen elsewhere.
        let forwarder = std::thread::spawn(move || {
            while let Ok(event) = event_rx.recv() {
                let update = match event {
                    WorkflowEvent::AgentOutput(text) => {
                        presenter_event_to_session_update(&PresenterEvent::AgentOutput(text))
                    }
                    WorkflowEvent::Progress(progress) => {
                        progress_event_to_session_update(&progress)
                    }
                    _ => None,
                };
                if let Some(update) = update {
                    if update_tx.send(update).is_err() {
                        break;
                    }
                }
            }
        });

        let hooks = recipe.create_hooks(Some(event_tx));
        let engine = WorkflowEngine::new(recipe.clone(), backend, storage_dir, Some(hooks));

        let mut context: HashMap<String, serde_json::Value> = HashMap::new();
        context.insert("prompt".to_string(), serde_json::json!(prompt));
        context.insert("feature_input".to_string(), serde_json::json!(prompt));
        context.insert("agent_output".to_string(), serde_json::json!(true));
        context.insert("session_id".to_string(), serde_json::json!(session_id));

        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = done_tx.send(Err(format!("build worker runtime: {e}")));
                return;
            }
        };

        let outcome = rt.block_on(engine.run_full_workflow(context));

        // Drop the engine (and its hooks) so `event_tx` closes and the forwarder finishes; join it
        // so every streamed update is queued before we report the turn's stop reason.
        drop(engine);
        let _ = forwarder.join();

        let result = match outcome {
            Ok(execution) => match &execution.status {
                ExecutionStatus::Error(msg) => Err(msg.clone()),
                // Completed → EndTurn. Free-prompting pauses at WaitingForInput after each single
                // invoke, awaiting the next user prompt; from ACP's perspective that is a completed
                // turn, so fall back to EndTurn for the non-error, non-Completed states.
                // TODO(acp): elicit a blocked `ClarificationQuestion` / elicitation event via
                // `request_permission` (using `tddy_acp::mapping::clarification_permission_options`)
                // and feed the chosen option back into the workflow to continue the turn, rather
                // than ending it here.
                other => {
                    Ok(execution_status_to_stop_reason(other).unwrap_or(acp::StopReason::EndTurn))
                }
            },
            Err(e) => Err(format!("workflow: {e}")),
        };
        let _ = done_tx.send(result);
    });
}

#[async_trait::async_trait(?Send)]
impl acp::Agent for AcpAgent {
    async fn initialize(
        &self,
        _args: acp::InitializeRequest,
    ) -> Result<acp::InitializeResponse, acp::Error> {
        Ok(acp::InitializeResponse::new(acp::ProtocolVersion::V1)
            .agent_capabilities(acp::AgentCapabilities::new().load_session(true))
            .agent_info(
                acp::Implementation::new("tddy-coder", env!("CARGO_PKG_VERSION"))
                    .title("TDDY Coder"),
            ))
    }

    async fn authenticate(
        &self,
        _args: acp::AuthenticateRequest,
    ) -> Result<acp::AuthenticateResponse, acp::Error> {
        Ok(acp::AuthenticateResponse::default())
    }

    async fn new_session(
        &self,
        _args: acp::NewSessionRequest,
    ) -> Result<acp::NewSessionResponse, acp::Error> {
        let session_id = uuid::Uuid::now_v7().to_string();
        create_session_dir_with_id(&self.data_dir, &session_id).map_err(|e| {
            log::error!("[acp] new_session create dir failed: {e}");
            acp::Error::internal_error()
        })?;
        Ok(acp::NewSessionResponse::new(acp::SessionId::new(
            session_id,
        )))
    }

    async fn load_session(
        &self,
        args: acp::LoadSessionRequest,
    ) -> Result<acp::LoadSessionResponse, acp::Error> {
        let session_id = args.session_id.0.to_string();
        let dir = self.session_dir(&session_id);
        if !dir.is_dir() {
            log::warn!("[acp] load_session: unknown session {session_id}");
            return Err(acp::Error::invalid_params());
        }
        Ok(acp::LoadSessionResponse::default())
    }

    async fn prompt(&self, args: acp::PromptRequest) -> Result<acp::PromptResponse, acp::Error> {
        let session_id = args.session_id.clone();
        let session_id_str = session_id.0.to_string();
        let storage_dir = workflow_engine_storage_dir(&self.session_dir(&session_id_str));
        let prompt = prompt_text(&args.prompt);

        let (update_tx, mut update_rx) = mpsc::unbounded_channel::<acp::SessionUpdate>();
        let (done_tx, done_rx) = oneshot::channel::<TurnResult>();

        spawn_turn(
            self.backend.clone(),
            self.recipe.clone(),
            storage_dir,
            session_id_str,
            prompt,
            update_tx,
            done_tx,
        );

        // Stream every update to the client before reporting the stop reason. The channel closes
        // once the worker's forwarder finishes, which is after the workflow run completes.
        while let Some(update) = update_rx.recv().await {
            self.notify(&session_id, update).await?;
        }

        let stop = done_rx
            .await
            .map_err(|_| acp::Error::internal_error())?
            .map_err(|msg| {
                log::error!("[acp] prompt failed: {msg}");
                acp::Error::internal_error()
            })?;
        Ok(acp::PromptResponse::new(stop))
    }

    async fn cancel(&self, _args: acp::CancelNotification) -> Result<(), acp::Error> {
        // TODO(acp): interrupt the in-flight workflow turn (SIGINT the backend child, see
        // `tddy_core::backend::kill_child_process`) instead of letting it run to completion.
        Ok(())
    }
}

/// Serve the workflow-backed ACP agent over stdio until the client disconnects.
pub fn run_acp(
    backend: SharedBackend,
    recipe: Arc<dyn WorkflowRecipe>,
    data_dir: PathBuf,
    _shutdown: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(data_dir.join(SESSIONS_SUBDIR))
        .map_err(|e| anyhow::anyhow!("create sessions dir: {e}"))?;

    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("build acp runtime: {e}"))?;
    let local_set = tokio::task::LocalSet::new();

    rt.block_on(local_set.run_until(async move {
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<Outbound>();
        let agent = AcpAgent {
            backend,
            recipe,
            data_dir,
            outbound_tx,
        };
        let (conn, handle_io) = acp::AgentSideConnection::new(agent, outgoing, incoming, |fut| {
            tokio::task::spawn_local(fut);
        });

        tokio::task::spawn_local(async move {
            while let Some(msg) = outbound_rx.recv().await {
                match msg {
                    Outbound::Notify(notif, ack) => {
                        let _ = conn.session_notification(notif).await;
                        let _ = ack.send(());
                    }
                }
            }
        });

        handle_io.await
    }))
    .map_err(|e| anyhow::anyhow!("acp stdio loop: {e}"))
}
