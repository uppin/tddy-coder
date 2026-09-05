//! Host-side `SessionChannel` driver, shared by the daemon, the standalone app, and tests.
//!
//! The host is a dumb byte relay: it answers `HostPoll`, fulfills CONNECT tunnels by opening the
//! real outbound socket and pumping bytes both ways (TLS stays end-to-end — the host never sees
//! plaintext), relays legacy unary egress, and forwards PTY output to a sink. Tool execution is
//! injected via [`HostToolHandler`] so each caller supplies its own behavior (the daemon runs real
//! tools; tests stub them).

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::Stream;
use prost::Message;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};

use tddy_service::proto::connection::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::sandbox::session_frame::Payload as SessionPayload;
use tddy_service::proto::sandbox::{
    EgressRequest, EgressResponse, HostPoll, SandboxInput, SessionFrame, SubscribeTerminal,
    TunnelClose, TunnelData, TunnelOpen, TunnelOpenAck,
};

use crate::runner::SandboxClient;

/// A decoded `SessionFrame` stream, transport-erased — either a tonic `Streaming<SessionFrame>`
/// or a decoded `tddy-stdio` bidi stream (see [`SessionChannelClient`]).
type SessionFrameStream = Pin<Box<dyn Stream<Item = Result<SessionFrame, String>> + Send>>;

/// Opens the in-jail `SessionChannel` bidi call, transport-agnostically. [`run_host_relay`] is
/// otherwise entirely transport-agnostic already (it only ever touches plain `SessionFrame`
/// structs, shared by both the tonic and stdio/`tddy-rpc` transports since `sandbox.proto`'s
/// message types are `extern_path`-unified across both codegen passes) — this is the one seam
/// where the transports genuinely differ (a typed tonic bidi call vs. `tddy-rpc`'s untyped
/// `call_unary`/`start_bidi_stream` interface, which has no generated client stub).
#[async_trait]
pub trait SessionChannelClient: Send {
    async fn open_session_channel(
        &mut self,
        outbound: ReceiverStream<SessionFrame>,
    ) -> Result<SessionFrameStream, String>;
}

#[async_trait]
impl SessionChannelClient for SandboxClient {
    async fn open_session_channel(
        &mut self,
        outbound: ReceiverStream<SessionFrame>,
    ) -> Result<SessionFrameStream, String> {
        let stream = self
            .session_channel(outbound)
            .await
            .map_err(|e| format!("open session channel: {e}"))?
            .into_inner();
        Ok(Box::pin(stream.map(|r| r.map_err(|e| e.to_string()))))
    }
}

/// [`SessionChannelClient`] over `tddy-stdio`'s untyped bidi interface. `start_bidi_stream`
/// returns a `StdioBidiSender` borrowing from the client it's called on, so the send/receive loop
/// runs entirely inside one spawned task that owns its own `Arc` clone — the borrow never needs
/// to outlive that task's stack frame, sidestepping the `'static` requirement `tokio::spawn` would
/// otherwise conflict with.
pub struct StdioSandboxClient {
    client: Arc<tddy_stdio::StdioRpcClient>,
}

impl StdioSandboxClient {
    pub fn new(client: Arc<tddy_stdio::StdioRpcClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SessionChannelClient for StdioSandboxClient {
    async fn open_session_channel(
        &mut self,
        mut outbound: ReceiverStream<SessionFrame>,
    ) -> Result<SessionFrameStream, String> {
        let client = Arc::clone(&self.client);
        let (result_tx, result_rx) = mpsc::channel::<Result<SessionFrame, String>>(64);

        tokio::spawn(async move {
            let (mut sender, mut receiver) =
                match client.start_bidi_stream("sandbox.SandboxService", "SessionChannel") {
                    Ok(pair) => pair,
                    Err(e) => {
                        let _ = result_tx
                            .send(Err(format!("start SessionChannel bidi call: {e}")))
                            .await;
                        return;
                    }
                };
            loop {
                tokio::select! {
                    frame = outbound.next() => {
                        match frame {
                            Some(frame) => {
                                if sender.send(frame.encode_to_vec(), false).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    item = receiver.recv() => {
                        match item {
                            Some(Ok(bytes)) => {
                                let decoded = SessionFrame::decode(bytes.as_slice())
                                    .map_err(|e| e.to_string());
                                if result_tx.send(decoded).await.is_err() {
                                    break;
                                }
                            }
                            Some(Err(e)) => {
                                let _ = result_tx.send(Err(e.to_string())).await;
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(result_rx)))
    }
}

/// One-shot "the pty session ended" flag, race-free regardless of whether `signal()` or `wait()`
/// runs first (the classic check-notify-check-await pattern for `tokio::sync::Notify`).
struct EndSignal {
    ended: AtomicBool,
    notify: tokio::sync::Notify,
}

impl EndSignal {
    fn new() -> Self {
        Self {
            ended: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
        }
    }

    fn signal(&self) {
        self.ended.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    async fn wait(&self) {
        loop {
            if self.ended.load(Ordering::SeqCst) {
                return;
            }
            let notified = self.notify.notified();
            if self.ended.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }
}

/// Injected tool-execution behavior for the host side of a sandbox `SessionChannel`.
#[async_trait]
pub trait HostToolHandler: Send + Sync + 'static {
    /// Execute a tool requested by the in-jail agent and return its response.
    async fn execute(
        &self,
        session_id: &str,
        tool_name: &str,
        args_json: &str,
    ) -> ExecuteToolResponse;
}

/// Injected RPC-dispatch behavior for the host side of a sandbox `SessionChannel`.
///
/// The in-jail `tddy-tools` reaches its facilitating daemon's `ConnectionService` over the
/// `SessionChannel` for the roster and conversation RPCs a managed session needs. The runner
/// forwards each as an [`RpcRequest`]; the host dispatches it here and the response (unary or
/// server-streaming) rides back as [`RpcStreamFrame`]s multiplexed by `request_id`. The host is
/// the only thing the jail can reach, and these RPCs live on the daemon — so the daemon supplies
/// the implementation that calls its `ConnectionServiceImpl`; the standalone app and tests supply
/// [`NullRpcHandler`], which refuses every call the way a daemon-less session should.
///
/// [`RpcRequest`]: tddy_service::proto::sandbox::RpcRequest
/// [`RpcStreamFrame`]: tddy_service::proto::sandbox::RpcStreamFrame
#[async_trait]
pub trait HostRpcHandler: Send + Sync + 'static {
    /// Dispatch `service`/`method` with the encoded request `payload`, returning either a single
    /// encoded response body or a server stream of them. A `Unary` error or a `ServerStream` error
    /// is carried back to the in-jail caller as a single terminal `RpcStreamFrame` with `error` set.
    async fn handle_rpc(&self, service: &str, method: &str, payload: &[u8]) -> tddy_rpc::RpcResult;
}

/// An RPC handler that refuses every call with `UNIMPLEMENTED`. The correct handler for a session
/// with no daemon in the loop (the standalone app) and for tests that do not exercise the roster
/// or conversation RPCs: the in-jail `tddy-tools` sees the refusal, reports the roster as
/// unavailable, and refuses every `subagent_*` call — which is the safe behaviour the runner TODO
/// described, now reached over the bridge rather than left unreachable.
#[derive(Default, Clone)]
pub struct NullRpcHandler;

#[async_trait]
impl HostRpcHandler for NullRpcHandler {
    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        _payload: &[u8],
    ) -> tddy_rpc::RpcResult {
        tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::unimplemented(format!(
            "this host does not serve {service}/{method}"
        ))))
    }
}

/// Wiring for [`run_host_relay`].
pub struct HostRelayConfig {
    /// Session id used on outbound frames (e.g. `SubscribeTerminal`).
    pub session_id: String,
    /// PTY output bytes from the jail are forwarded here (the daemon fans into broadcast+capture;
    /// tests collect them into a buffer).
    pub terminal_sink: mpsc::UnboundedSender<Bytes>,
    /// Initial terminal dimensions sent with `SubscribeTerminal`.
    pub initial_cols: u32,
    pub initial_rows: u32,
}

impl HostRelayConfig {
    /// Config with the default 80x24 terminal size (suitable for headless callers and tests).
    pub fn new(session_id: impl Into<String>, terminal_sink: mpsc::UnboundedSender<Bytes>) -> Self {
        Self {
            session_id: session_id.into(),
            terminal_sink,
            initial_cols: 80,
            initial_rows: 24,
        }
    }
}

/// How long one in-jail tool call may wait for its answer before its channel is declared lost.
///
/// Comfortably above the tool engine's own ceilings (a blocking `Shell` defaults to 30s and takes
/// its limit from the caller), because a call that outlives this leaves the host unable to tell
/// which answer belongs to which request — the frame carries no request id, so the channel is torn
/// down rather than reused.
///
/// Public, and the daemon's own in-jail exchange (`tddy_daemon::workspace_tool_sandbox`) uses this
/// very constant rather than a matching number of its own. Two hosts drive this protocol into the
/// same jail, and a call still legitimate on one of them must not already be abandoned on the
/// other — an invariant a second declaration states but nothing enforces.
pub const IN_JAIL_TOOL_TIMEOUT: Duration = Duration::from_secs(600);

/// Sends tool calls **into** a jail over an already-running host relay.
///
/// The mirror image of [`HostToolHandler`], and the half a `--workspace-tools` jail needs: there
/// the jail asks the host to reach the worktree for it, here the host — which cannot touch the
/// worktree unconfined, or in the standalone app's case must not — asks the jail.
///
/// One call is outstanding at a time. `in_jail_tool_response` carries no request id (the frame was
/// designed for a host that keeps exactly one in flight), so the discipline is not an optimisation
/// choice: a second concurrent call would have no way to tell which answer is its own.
///
/// A [`HostToolHandler`] that dispatched back through here would deadlock itself: the relay's
/// reader loop awaits the handler inline, and that same loop is the only thing that can deliver
/// the `in_jail_tool_response` the handler would be waiting for. The pairing exists in no session
/// today — a jail that serves in-jail calls hosts no agent to make outward ones, and the app wires
/// [`NullToolHandler`] — but a future caller that wires both must dispatch off the handler's task,
/// not from inside it. [`IN_JAIL_TOOL_TIMEOUT`] would bound such a deadlock, not excuse it.
pub struct InJailToolDispatcher {
    /// The relay's outbound half — the same one the poll loop and the tunnels send on, because the
    /// jail has exactly one `SessionChannel` and everything it is told travels over it.
    host_tx: mpsc::Sender<SessionFrame>,
    /// Where the relay's reader loop hands back the answer to the outstanding call.
    exchange: Arc<InJailToolExchange>,
    /// Held across the send-and-await, so the next caller only sends once this call is answered.
    turn: tokio::sync::Mutex<()>,
    /// How long a call waits for its answer before the channel is declared lost.
    answer_timeout: Duration,
}

impl InJailToolDispatcher {
    /// Wait a different length of time for the jail's answer than [`IN_JAIL_TOOL_TIMEOUT`].
    ///
    /// For a caller that knows its jail's own ceiling is lower — including this crate's dispatch
    /// tests, which hold it to a fraction of a second so a jail that never answers can be shown to
    /// settle within the run rather than ten minutes later.
    pub fn with_answer_timeout(mut self, timeout: Duration) -> Self {
        self.answer_timeout = timeout;
        self
    }

    /// Run one tool call inside the jail and return its answer.
    ///
    /// A tool that failed answers with `is_error`, exactly as the host tool engine does — only the
    /// dispatch is this type's concern. A jail that cannot serve in-jail calls at all (one started
    /// without `--workspace-tools`) says so the same way, and so does a channel that closed before
    /// the answer arrived, or one that let [`IN_JAIL_TOOL_TIMEOUT`] pass without answering: the
    /// caller is an MCP tool call that must end, not wait.
    ///
    /// The timeout is the one failure the *session* does not survive — every later call is refused
    /// — so it is also logged at `error` on the host. The agent's own answer says the same thing
    /// in the terms the agent can act on: restart the session.
    pub async fn execute(&self, request: ExecuteToolRequest) -> ExecuteToolResponse {
        // Kept out of the frame the request is about to move into, so the host can name the call
        // it gave up on. The only thing on the host that ever learns a channel died is this log.
        let tool_name = request.tool_name.clone();
        let _turn = self.turn.lock().await;
        if self.exchange.is_lost() {
            // Written for the model that reads it, not for whoever wrote the frame format. In a
            // `sandboxed` session these calls are the agent's only route to the checkout, so a
            // lost channel is not one failed tool — it is every tool, for the rest of the session.
            // An agent told only that answers can no longer be attributed will keep trying; one
            // told the session has to be restarted can say so and stop.
            return failed_dispatch(
                "an earlier call went unanswered past its budget, so this jail's tool channel is \
                 finished: a late answer carries no request id and would be handed to whichever \
                 call is waiting next. Every tool call from here on will fail the same way — \
                 restart the session to get the tools back",
            );
        }
        let answer = self.exchange.park();

        if self
            .host_tx
            .send(SessionFrame {
                payload: Some(SessionPayload::InJailToolRequest(request)),
            })
            .await
            .is_err()
        {
            self.exchange.abandon();
            return failed_dispatch("the jail is no longer reading its session channel");
        }

        match tokio::time::timeout(self.answer_timeout, answer).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => failed_dispatch("its session channel ended before the call was answered"),
            Err(_) => {
                // Waiting on unbounded faith would hold the turnstile for the life of the session,
                // wedging every later call with nothing to report. Give up on this one — and on
                // the channel with it, because a late answer carries no request id and would be
                // handed to whichever call is parked next. A misattributed answer is worse than
                // the hang it replaced.
                self.exchange.declare_lost();
                // The one record on the host that this happened. Nothing else notices: the agent
                // is a separate process that only sees its own failed call, the jail is still
                // running, and every later call is refused without a frame ever reaching the
                // relay — so an operator watching a session that has gone inert has nothing else
                // to find. `error`, not `warn`, because the session cannot recover from it.
                //
                // `{:?}` rather than a seconds count: the budget is a `Duration` and prints
                // itself as one ("600s"), so a narrowed budget does not report itself as "0s".
                log::error!(
                    target: "tddy_sandbox_runner::host_relay",
                    "in-jail tool call {tool_name} was not answered within {:?}; this jail's tool \
                     channel is now lost and every later call will be refused — the session has \
                     to be restarted",
                    self.answer_timeout
                );
                failed_dispatch(&format!(
                    "it did not answer within {:?}, so this jail's tool channel is finished — \
                     every later call will fail the same way, and the session has to be restarted",
                    self.answer_timeout
                ))
            }
        }
    }
}

/// The one in-jail tool call that may be outstanding, shared between the [`InJailToolDispatcher`]
/// that parks a waiting caller here and the relay's reader loop that fulfils it. A plain slot
/// rather than a map because `in_jail_tool_response` carries no request id to key one by.
#[derive(Default)]
struct InJailToolExchange {
    pending: std::sync::Mutex<Option<oneshot::Sender<ExecuteToolResponse>>>,
    /// Set once an answer went missing rather than merely failing. From then on the slot is
    /// unusable: an answer to the lost call could still arrive, and with no request id on the
    /// frame it would be indistinguishable from an answer to the next one.
    lost: AtomicBool,
}

impl InJailToolExchange {
    /// Claim the slot for a call about to be sent, returning the receiver its caller waits on.
    fn park(&self) -> oneshot::Receiver<ExecuteToolResponse> {
        let (tx, rx) = oneshot::channel();
        *self.pending.lock().unwrap() = Some(tx);
        rx
    }

    /// Hand an arrived `in_jail_tool_response` to the caller waiting for it. An answer nobody is
    /// waiting for is dropped — with no request id on the frame there is no call it could belong to.
    fn answer(&self, response: ExecuteToolResponse) {
        if let Some(tx) = self.pending.lock().unwrap().take() {
            let _ = tx.send(response);
        }
    }

    /// Give up on the outstanding call: dropping its sender wakes `execute` with a failure rather
    /// than leaving it parked on a channel nothing will ever write to.
    fn abandon(&self) {
        self.pending.lock().unwrap().take();
    }

    /// Give up on the outstanding call *and* on every call after it. Used where the answer may
    /// still be in flight, so the exchange can no longer match answers to callers.
    fn declare_lost(&self) {
        self.lost.store(true, Ordering::SeqCst);
        self.abandon();
    }

    /// Whether this exchange has stopped being able to attribute answers.
    fn is_lost(&self) -> bool {
        self.lost.load(Ordering::SeqCst)
    }
}

/// A call that never reached the jail, or never came back from it. Shaped like any other failed
/// tool call so the agent blocked on it gets an answer it can report.
fn failed_dispatch(reason: &str) -> ExecuteToolResponse {
    ExecuteToolResponse {
        is_error: true,
        error_message: format!("the tool call could not be run in its jail: {reason}"),
        ..Default::default()
    }
}

/// [`run_host_relay`] plus an [`InJailToolDispatcher`] for sending tool calls the other way.
///
/// A separate entry point rather than a field on [`HostRelayConfig`] because the two directions are
/// not both live in any one session: a jail that hosts an agent answers no in-jail calls, and a
/// jail that serves them hosts no agent to make the outward ones.
pub async fn run_host_relay_with_in_jail_tools<H: HostToolHandler, C: SessionChannelClient>(
    client: C,
    tool_handler: H,
    config: HostRelayConfig,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
) -> Result<(JoinHandle<()>, InJailToolDispatcher), String> {
    // A jail that serves tool calls has no daemon behind the host dispatching them — the roster and
    // conversation RPCs are refused the same way the standalone app refuses them elsewhere.
    run_host_relay_inner(
        client,
        tool_handler,
        Arc::new(NullRpcHandler),
        config,
        stdin_rx,
    )
    .await
}

/// Open the in-jail `SessionChannel` over `client` (tonic or stdio — see [`SessionChannelClient`]),
/// subscribe to the main terminal, and drive the host side: poll, relay CONNECT tunnels and
/// egress, forward terminal output to [`HostRelayConfig::terminal_sink`], and dispatch tool
/// requests to `tool_handler`. Returns the background task driving inbound frames; `stdin_rx`
/// bytes are written to the jail PTY.
///
/// RPCs the in-jail agent forwards (the roster and conversation RPCs) are refused with
/// `UNIMPLEMENTED` via [`NullRpcHandler`]. A caller with a daemon in the loop — the daemon itself
/// — uses [`run_host_relay_with_rpc`] to supply a real [`HostRpcHandler`].
pub async fn run_host_relay<H: HostToolHandler, C: SessionChannelClient>(
    client: C,
    tool_handler: H,
    config: HostRelayConfig,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
) -> Result<JoinHandle<()>, String> {
    run_host_relay_with_rpc(
        client,
        tool_handler,
        Arc::new(NullRpcHandler),
        config,
        stdin_rx,
    )
    .await
}

/// [`run_host_relay`] with an explicit [`HostRpcHandler`], for callers that serve the roster and
/// conversation RPCs to their in-jail agent. The daemon passes its `ConnectionService` dispatch
/// here so a managed session's `tddy-tools` can follow the live roster and open conversations
/// with remote agents over the same `SessionChannel` that carries its tool calls.
pub async fn run_host_relay_with_rpc<H: HostToolHandler, C: SessionChannelClient>(
    client: C,
    tool_handler: H,
    rpc_handler: Arc<dyn HostRpcHandler>,
    config: HostRelayConfig,
    stdin_rx: mpsc::UnboundedReceiver<Bytes>,
) -> Result<JoinHandle<()>, String> {
    let (reader, _dispatcher) =
        run_host_relay_inner(client, tool_handler, rpc_handler, config, stdin_rx).await?;
    Ok(reader)
}

/// The relay itself, in the one shape both directions need: the reader loop that answers the jail
/// and the dispatcher that asks it are two ends of the same `SessionChannel`, so they are built
/// together and the entry points above keep whichever half their caller has a use for.
async fn run_host_relay_inner<H: HostToolHandler, C: SessionChannelClient>(
    mut client: C,
    tool_handler: H,
    rpc_handler: Arc<dyn HostRpcHandler>,
    config: HostRelayConfig,
    mut stdin_rx: mpsc::UnboundedReceiver<Bytes>,
) -> Result<(JoinHandle<()>, InJailToolDispatcher), String> {
    let (host_tx, host_rx) = mpsc::channel(64);
    let host_tx_dispatch = host_tx.clone();
    let host_stream = ReceiverStream::new(host_rx);
    let mut session = client.open_session_channel(host_stream).await?;

    host_tx
        .send(SessionFrame {
            payload: Some(SessionPayload::SubscribeTerminal(SubscribeTerminal {
                session_id: config.session_id.clone(),
                terminal_id: "main".to_string(),
                initial_cols: config.initial_cols,
                initial_rows: config.initial_rows,
            })),
        })
        .await
        .map_err(|_| "session channel closed before subscribe".to_string())?;

    let session_id = config.session_id.clone();
    let terminal_sink = config.terminal_sink;
    let host_tx_reader = host_tx.clone();
    let end_signal = Arc::new(EndSignal::new());
    let in_jail_tools = Arc::new(InJailToolExchange::default());

    let reader = tokio::spawn({
        let end_signal = Arc::clone(&end_signal);
        let rpc_handler = Arc::clone(&rpc_handler);
        let in_jail_tools = Arc::clone(&in_jail_tools);
        async move {
            // CONNECT tunnels: tunnel_id → sender feeding agent→host bytes into the outbound TCP socket.
            let mut tunnels: HashMap<String, mpsc::UnboundedSender<Bytes>> = HashMap::new();
            while let Some(Ok(frame)) = session.next().await {
                match frame.payload {
                    Some(SessionPayload::SessionEnded(_)) => {
                        // The pty command exited — stop polling and let both ends of the stream drop,
                        // so the in-jail gRPC server can finish shutting down (see `signal_session_ended`).
                        end_signal.signal();
                        break;
                    }
                    Some(SessionPayload::ToolRequest(req)) => {
                        let resp = tool_handler
                            .execute(&session_id, &req.tool_name, &req.args_json)
                            .await;
                        let _ = host_tx_reader
                            .send(SessionFrame {
                                payload: Some(SessionPayload::ToolResponse(resp)),
                            })
                            .await;
                    }
                    Some(SessionPayload::EgressRequest(req)) => {
                        let resp = relay_egress_request(req).await;
                        let _ = host_tx_reader
                            .send(SessionFrame {
                                payload: Some(SessionPayload::EgressResponse(resp)),
                            })
                            .await;
                    }
                    Some(SessionPayload::RpcRequest(req)) => {
                        // An RPC the in-jail `tddy-tools` asked the host to dispatch to its
                        // `ConnectionService` (roster + conversation). The response is multiplexed
                        // back as `RpcStreamFrame`s addressed by `request_id` — a unary RPC is one
                        // terminal frame, a server stream is many followed by a terminal one. Sent
                        // on the outbound stream directly (not poll-gated), the same way tunnel
                        // frames are, so a lifetime-long `StreamSessionAgents` does not occupy the
                        // single `awaiting_tool` slot the poll path uses.
                        let request_id = req.request_id;
                        let tx = host_tx_reader.clone();
                        match rpc_handler
                            .handle_rpc(&req.service, &req.method, &req.payload)
                            .await
                        {
                            tddy_rpc::RpcResult::Unary(Ok(body)) => {
                                let _ = tx
                                    .send(SessionFrame {
                                        payload: Some(SessionPayload::RpcStreamFrame(
                                            tddy_service::proto::sandbox::RpcStreamFrame {
                                                request_id,
                                                payload: body,
                                                end_of_stream: true,
                                                error: String::new(),
                                            },
                                        )),
                                    })
                                    .await;
                            }
                            tddy_rpc::RpcResult::Unary(Err(status)) => {
                                let _ = tx
                                    .send(SessionFrame {
                                        payload: Some(SessionPayload::RpcStreamFrame(
                                            tddy_service::proto::sandbox::RpcStreamFrame {
                                                request_id,
                                                payload: Vec::new(),
                                                end_of_stream: true,
                                                error: status.message,
                                            },
                                        )),
                                    })
                                    .await;
                            }
                            tddy_rpc::RpcResult::ServerStream(Ok(mut rx)) => {
                                tokio::spawn(async move {
                                    while let Some(frame) = rx.recv().await {
                                        let (payload, error) = match frame {
                                            Ok(bytes) => (bytes, String::new()),
                                            Err(status) => (Vec::new(), status.message),
                                        };
                                        let is_end = !error.is_empty();
                                        if tx
                                            .send(SessionFrame {
                                                payload: Some(SessionPayload::RpcStreamFrame(
                                                    tddy_service::proto::sandbox::RpcStreamFrame {
                                                        request_id: request_id.clone(),
                                                        payload,
                                                        end_of_stream: is_end,
                                                        error,
                                                    },
                                                )),
                                            })
                                            .await
                                            .is_err()
                                        {
                                            return;
                                        }
                                        if is_end {
                                            return;
                                        }
                                    }
                                    // The stream ended cleanly (sender dropped) without an error
                                    // frame — send the terminal marker so the in-jail caller's
                                    // `recv()` loop observes end-of-stream rather than hanging.
                                    let _ = tx
                                        .send(SessionFrame {
                                            payload: Some(SessionPayload::RpcStreamFrame(
                                                tddy_service::proto::sandbox::RpcStreamFrame {
                                                    request_id,
                                                    payload: Vec::new(),
                                                    end_of_stream: true,
                                                    error: String::new(),
                                                },
                                            )),
                                        })
                                        .await;
                                });
                            }
                            tddy_rpc::RpcResult::ServerStream(Err(status)) => {
                                let _ = tx
                                    .send(SessionFrame {
                                        payload: Some(SessionPayload::RpcStreamFrame(
                                            tddy_service::proto::sandbox::RpcStreamFrame {
                                                request_id,
                                                payload: Vec::new(),
                                                end_of_stream: true,
                                                error: status.message,
                                            },
                                        )),
                                    })
                                    .await;
                            }
                        }
                    }
                    Some(SessionPayload::TunnelOpen(open)) => {
                        // Agent issued CONNECT host:port — the host owns the real outbound socket.
                        let (tcp_in_tx, tcp_in_rx) = mpsc::unbounded_channel::<Bytes>();
                        tunnels.insert(open.tunnel_id.clone(), tcp_in_tx);
                        spawn_tunnel(open, tcp_in_rx, host_tx_reader.clone());
                    }
                    Some(SessionPayload::TunnelData(data)) => {
                        // Agent→host bytes: feed into the outbound socket for this tunnel.
                        if let Some(tx) = tunnels.get(&data.tunnel_id) {
                            if tx.send(Bytes::from(data.data)).is_err() {
                                tunnels.remove(&data.tunnel_id);
                            }
                        }
                    }
                    Some(SessionPayload::TunnelClose(close)) => {
                        // Agent closed its end: drop the sender so the socket writer shuts down.
                        tunnels.remove(&close.tunnel_id);
                    }
                    Some(SessionPayload::InJailToolResponse(resp)) => {
                        // The jail answering a call this host sent it — hand it to the caller
                        // parked on it (see [`InJailToolDispatcher`]).
                        in_jail_tools.answer(resp);
                    }
                    Some(SessionPayload::TerminalOutput(out)) => {
                        if !out.data.is_empty() {
                            let _ = terminal_sink.send(Bytes::from(out.data));
                        }
                    }
                    _ => {}
                }
            }
            // The channel is gone: an in-jail call still waiting for an answer will never get one,
            // and the caller is a tool call that must end rather than wait.
            in_jail_tools.abandon();
        }
    });

    let session_id_in = config.session_id.clone();
    tokio::spawn(async move {
        let mut poll = tokio::time::interval(Duration::from_millis(25));
        // Keep polling even after the caller drops its stdin sender — a closed stdin must not stop
        // `HostPoll` (which drives terminal output and poll-gated frames). Polling does stop once
        // the pty session has ended (`end_signal`), so both ends of the stream can drop and the
        // in-jail gRPC server can finish shutting down.
        let mut stdin_open = true;
        loop {
            tokio::select! {
                _ = end_signal.wait() => break,
                chunk = stdin_rx.recv(), if stdin_open => {
                    match chunk {
                        Some(chunk) => {
                            let _ = host_tx.send(SessionFrame {
                                payload: Some(SessionPayload::TerminalInput(SandboxInput {
                                    session_id: session_id_in.clone(),
                                    terminal_id: "main".to_string(),
                                    data: chunk.to_vec(),
                                })),
                            }).await;
                        }
                        None => stdin_open = false,
                    }
                }
                _ = poll.tick() => {
                    if host_tx.send(SessionFrame {
                        payload: Some(SessionPayload::HostPoll(HostPoll {})),
                    }).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    Ok((
        reader,
        InJailToolDispatcher {
            host_tx: host_tx_dispatch,
            exchange: in_jail_tools,
            turn: tokio::sync::Mutex::new(()),
            answer_timeout: IN_JAIL_TOOL_TIMEOUT,
        },
    ))
}

/// Open the real outbound TCP connection for a relayed `CONNECT` tunnel and pump bytes both ways
/// over the `SessionChannel`. The host is a dumb byte relay — TLS stays end-to-end between the
/// in-jail agent and the target, so credentials never appear in plaintext here.
fn spawn_tunnel(
    open: TunnelOpen,
    mut tcp_in_rx: mpsc::UnboundedReceiver<Bytes>,
    host_tx: mpsc::Sender<SessionFrame>,
) {
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let tunnel_id = open.tunnel_id.clone();
        let addr = format!("{}:{}", open.host, open.port);
        let stream = match tokio::net::TcpStream::connect(&addr).await {
            Ok(s) => s,
            Err(e) => {
                let _ = host_tx
                    .send(SessionFrame {
                        payload: Some(SessionPayload::TunnelOpenAck(TunnelOpenAck {
                            tunnel_id,
                            ok: false,
                            error: format!("connect {addr}: {e}"),
                        })),
                    })
                    .await;
                return;
            }
        };
        let _ = host_tx
            .send(SessionFrame {
                payload: Some(SessionPayload::TunnelOpenAck(TunnelOpenAck {
                    tunnel_id: tunnel_id.clone(),
                    ok: true,
                    error: String::new(),
                })),
            })
            .await;

        let (mut read_half, mut write_half) = stream.into_split();

        // host → agent: forward outbound-socket bytes as TunnelData; signal close on EOF/error.
        let up_tx = host_tx.clone();
        let up_id = tunnel_id.clone();
        let up = tokio::spawn(async move {
            let mut buf = [0u8; 16384];
            loop {
                match read_half.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if up_tx
                            .send(SessionFrame {
                                payload: Some(SessionPayload::TunnelData(TunnelData {
                                    tunnel_id: up_id.clone(),
                                    data: buf[..n].to_vec(),
                                })),
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = up_tx
                .send(SessionFrame {
                    payload: Some(SessionPayload::TunnelClose(TunnelClose {
                        tunnel_id: up_id,
                        error: String::new(),
                    })),
                })
                .await;
        });

        // agent → host: drain inbound bytes into the outbound socket until the agent closes.
        while let Some(bytes) = tcp_in_rx.recv().await {
            if write_half.write_all(&bytes).await.is_err() {
                break;
            }
        }
        let _ = write_half.shutdown().await;
        up.abort();
    });
}

/// Perform a legacy unary egress request on the host's behalf (used for `GET /probe`).
pub async fn relay_egress_request(req: EgressRequest) -> EgressResponse {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return EgressResponse {
                request_id: req.request_id,
                error_message: format!("build http client: {e}"),
                ..Default::default()
            };
        }
    };

    let method = reqwest::Method::from_bytes(req.method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut builder = client.request(method, &req.url);
    for header in &req.headers {
        builder = builder.header(&header.name, &header.value);
    }
    if !req.body.is_empty() {
        builder = builder.body(req.body.clone());
    }

    match builder.send().await {
        Ok(resp) => {
            let status_code = resp.status().as_u16() as u32;
            let body = resp.bytes().await.unwrap_or_default();
            EgressResponse {
                request_id: req.request_id,
                status_code,
                body: body.to_vec(),
                ..Default::default()
            }
        }
        Err(e) => EgressResponse {
            request_id: req.request_id,
            error_message: format!("outbound fetch failed: {e}"),
            ..Default::default()
        },
    }
}

/// Tool handler for a session that answers none: every `tool_request` the jail sends **out** to
/// the host is refused.
///
/// The outward direction, and only that one — the inward `in_jail_tool_request` this file also
/// carries is [`InJailToolDispatcher`]'s, and a session can serve one, the other, or neither. Two
/// kinds of session wire this: a generic confined pty action, which runs a command and hosts no
/// agent to make tool calls; and a `sandboxed` codebase session, where the tools run *inside* the
/// jail and a request coming the other way is one nothing in the session could have made.
pub struct NullToolHandler;

#[async_trait]
impl HostToolHandler for NullToolHandler {
    async fn execute(
        &self,
        _session_id: &str,
        tool_name: &str,
        _args_json: &str,
    ) -> ExecuteToolResponse {
        ExecuteToolResponse {
            is_error: true,
            error_message: format!(
                "this session serves no host-side tools, so {tool_name} cannot be run here"
            ),
            ..Default::default()
        }
    }
}
