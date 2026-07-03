//! TddyRemote gRPC service implementation.

use std::sync::mpsc;

use tokio::sync::broadcast;
use tokio_stream::wrappers::ReceiverStream;
use tonic::codec::Streaming;
use tonic::{Request, Response, Status};

use tddy_core::{PresenterEvent, PresenterHandle};

use crate::convert::{client_message_to_intent, event_to_server_message};
use crate::gen::{
    tddy_remote_server::TddyRemote, ClientMessage, GetSessionRequest, GetSessionResponse,
    ListSessionsRequest, ListSessionsResponse, ServerMessage,
};

/// Builds a fresh [`tddy_core::ViewConnection`] (state snapshot + live event subscription + intent
/// sender) per opened stream — the server-side equivalent of the TUI attaching via
/// `Presenter::connect_view`.
type ViewFactory = std::sync::Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync>;

/// How a `TddyRemoteService` reaches the Presenter for an opened stream.
enum ViewSource {
    /// Raw handle: live events only, no snapshot. For callers/tests without Presenter state access.
    Handle {
        event_tx: broadcast::Sender<PresenterEvent>,
        intent_tx: mpsc::Sender<tddy_core::UserIntent>,
    },
    /// `connect_view`-based: each opened stream gets an atomic state snapshot (replayed to the View)
    /// plus a live subscription, so a View connecting after output was produced still sees it.
    View(ViewFactory),
}

/// gRPC service that bridges Presenter events and intents.
pub struct TddyRemoteService {
    source: ViewSource,
}

impl TddyRemoteService {
    /// Live-events-only construction from a raw handle (no snapshot-on-connect).
    pub fn new(handle: PresenterHandle) -> Self {
        Self {
            source: ViewSource::Handle {
                event_tx: handle.event_tx,
                intent_tx: handle.intent_tx,
            },
        }
    }

    /// Construction that mirrors the TUI's `connect_view`: each opened stream replays the current
    /// state snapshot to the View, then forwards live events — so a View connecting after agent
    /// output was produced still sees the prior transcript.
    pub fn with_view_factory(view_factory: ViewFactory) -> Self {
        Self {
            source: ViewSource::View(view_factory),
        }
    }

    /// Open a View subscription: `(snapshot messages to replay first, live event receiver, intent
    /// sender)`. For the `connect_view` source the snapshot and subscription come from a single
    /// [`tddy_core::ViewConnection`], so no live event can slip in between them.
    fn open_view(
        &self,
    ) -> Result<
        (
            Vec<ServerMessage>,
            broadcast::Receiver<PresenterEvent>,
            mpsc::Sender<tddy_core::UserIntent>,
        ),
        &'static str,
    > {
        match &self.source {
            ViewSource::Handle {
                event_tx,
                intent_tx,
            } => {
                log::info!(
                    "[TddyRemote] open_view: Handle source (live events only, no snapshot); subscribers now={}",
                    event_tx.receiver_count() + 1
                );
                Ok((Vec::new(), event_tx.subscribe(), intent_tx.clone()))
            }
            ViewSource::View(view_factory) => {
                log::info!("[TddyRemote] open_view: View source — calling view_factory (connect_view)");
                let conn = match view_factory() {
                    Some(c) => c,
                    None => {
                        log::warn!("[TddyRemote] open_view: connect_view returned None (broadcast/intent unset?)");
                        return Err("presenter unavailable");
                    }
                };
                let activity_len = conn.state_snapshot.activity_log.len();
                let agent_output_entries = conn
                    .state_snapshot
                    .activity_log
                    .iter()
                    .filter(|e| matches!(e.kind, tddy_core::ActivityKind::AgentOutput))
                    .count();
                let replay = crate::convert::snapshot_replay_messages(&conn.state_snapshot);
                log::info!(
                    "[TddyRemote] open_view: connect_view snapshot has {} activity entries ({} agent-output); replaying {} message(s), mode={:?}, goal={:?}",
                    activity_len,
                    agent_output_entries,
                    replay.len(),
                    conn.state_snapshot.mode,
                    conn.state_snapshot.current_goal,
                );
                Ok((replay, conn.event_rx, conn.intent_tx))
            }
        }
    }
}

/// Assemble the RPC service surface a session process exposes to remote UIs (the browser View
/// adapter and any other RPC client) over a given transport.
///
/// Architecture: UI → View-adapter RPC → Presenter (actions in as intents, events out as a
/// broadcast). The Presenter is the source of truth; LiveKit is *just an RPC protocol* carrying
/// this surface. So the Presenter's `TddyRemote` View-adapter must be reachable on **every**
/// transport that carries the surface — a session that serves it only on the local gRPC port but
/// omits it from the LiveKit `MultiRpcService` leaves a browser View unable to reach the Presenter
/// at all (no agent responses, no way to send prompts).
///
/// Callers pass the transport-specific base services already built (terminal, token, tunnel, …)
/// plus the Presenter's `connect_view` factory; this centralizes mounting the Presenter View-adapter
/// + a reflection entry so the two transports can never diverge again. Using the factory (rather
/// than a raw handle) means each opened stream replays the current state snapshot to the View, like
/// the TUI.
pub fn session_view_adapter_surface(
    mut base_entries: Vec<tddy_rpc::ServiceEntry>,
    view_factory: std::sync::Arc<dyn Fn() -> Option<tddy_core::ViewConnection> + Send + Sync>,
) -> tddy_rpc::MultiRpcService {
    use crate::proto::remote::TddyRemoteServer;
    base_entries.push(tddy_rpc::ServiceEntry {
        name: TddyRemoteServer::<TddyRemoteService>::NAME,
        service: std::sync::Arc::new(TddyRemoteServer::new(TddyRemoteService::with_view_factory(
            view_factory,
        ))) as std::sync::Arc<dyn tddy_rpc::RpcService>,
    });
    let names: Vec<&str> = base_entries.iter().map(|e| e.name).collect();
    base_entries.push(crate::reflection_entry_from(&names));
    tddy_rpc::MultiRpcService::new(base_entries)
}

impl TddyRemoteService {
    /// Shared outbound wiring for both transports (gRPC/tonic and LiveKit/`tddy_rpc`).
    ///
    /// Opens the view (snapshot + live-event subscription + intent channel), spawns the single
    /// forwarder that first replays the snapshot then streams live presenter events as tonic
    /// [`ServerMessage`]s, and returns the outbound receiver together with the intent sender. Each
    /// transport maps the (wire-identical) tonic `ServerMessage`s to its own codegen type and pumps
    /// client intents into `intent_tx` — so both share this one implementation and one set of logs.
    ///
    /// `transport` is a short label (e.g. `"gRPC"`, `"LiveKit"`) used only for logging, so both
    /// paths are observable identically.
    fn open_view_stream(
        &self,
        transport: &str,
    ) -> Result<
        (
            tokio::sync::mpsc::Receiver<ServerMessage>,
            mpsc::Sender<tddy_core::UserIntent>,
        ),
        String,
    > {
        log::info!("[TddyRemote] {} stream() ENTER (before open_view)", transport);
        let (replay, mut event_rx, intent_tx) = self.open_view().map_err(|e| e.to_string())?;
        log::info!(
            "[TddyRemote] {} stream opened; replaying {} snapshot message(s)",
            transport,
            replay.len()
        );

        let (tx, rx) = tokio::sync::mpsc::channel::<ServerMessage>(64);
        let transport = transport.to_string();
        tokio::spawn(async move {
            for msg in replay {
                if tx.send(msg).await.is_err() {
                    return;
                }
            }
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        if tx.send(event_to_server_message(event)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        log::warn!(
                            "[TddyRemote] {} stream: broadcast receiver lagged; skipped {} presenter event(s)",
                            transport,
                            skipped
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            log::info!("[TddyRemote] {} stream: outbound forwarder ended", transport);
        });

        Ok((rx, intent_tx))
    }
}

#[tonic::async_trait]
impl TddyRemote for TddyRemoteService {
    type StreamStream = ReceiverStream<Result<ServerMessage, Status>>;

    async fn stream(
        &self,
        request: Request<Streaming<ClientMessage>>,
    ) -> Result<Response<Self::StreamStream>, Status> {
        let (mut outbound_rx, intent_tx) =
            self.open_view_stream("gRPC").map_err(Status::unavailable)?;
        let mut client_stream = request.into_inner();

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
                if tx.send(Ok(msg)).await.is_err() {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            while let Ok(Some(msg)) = client_stream.message().await {
                if let Some(intent) = client_message_to_intent(msg) {
                    let _ = intent_tx.send(intent);
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_session(
        &self,
        _request: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        Err(Status::unimplemented(
            "GetSession is only available in daemon mode",
        ))
    }

    async fn list_sessions(
        &self,
        _request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        Err(Status::unimplemented(
            "ListSessions is only available in daemon mode",
        ))
    }
}

/// `RpcService`-compatible impl of `TddyRemote` — the transport-agnostic counterpart to the
/// tonic impl above, so `TddyRemoteService` can be mounted on any `tddy_rpc` transport
/// (`--stdio`, or a LiveKit `MultiRpcService`; the plain-gRPC impl above only satisfies tonic's
/// server trait, which neither of those can wrap). Generated with `extern_path` back onto the
/// same `crate::gen::{ClientMessage, ServerMessage, ...}` structs used above (see build.rs), so
/// this is the *same* Rust type on both sides of `remote.proto` — no re-encoding needed, and
/// `event_to_server_message`/`client_message_to_intent` work unchanged. Delegates the
/// snapshot-replay + live-event forwarding to the single shared implementation
/// (`TddyRemoteService::open_view_stream`), exactly like the tonic impl.
#[async_trait::async_trait]
impl crate::proto::remote::TddyRemote for TddyRemoteService {
    type StreamStream = ReceiverStream<Result<ServerMessage, tddy_rpc::Status>>;

    async fn stream(
        &self,
        request: tddy_rpc::Request<tddy_rpc::Streaming<ClientMessage>>,
    ) -> Result<tddy_rpc::Response<Self::StreamStream>, tddy_rpc::Status> {
        let (mut outbound_rx, intent_tx) = self
            .open_view_stream("stdio/LiveKit")
            .map_err(tddy_rpc::Status::internal)?;
        let mut client_stream = request.into_inner();

        let (tx, rx) = tokio::sync::mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
                if tx.send(Ok(msg)).await.is_err() {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            use futures_util::StreamExt;
            while let Some(Ok(msg)) = client_stream.next().await {
                if let Some(intent) = client_message_to_intent(msg) {
                    let _ = intent_tx.send(intent);
                }
            }
        });

        Ok(tddy_rpc::Response::new(ReceiverStream::new(rx)))
    }

    async fn get_session(
        &self,
        _request: tddy_rpc::Request<GetSessionRequest>,
    ) -> Result<tddy_rpc::Response<GetSessionResponse>, tddy_rpc::Status> {
        Err(tddy_rpc::Status::unimplemented(
            "GetSession is only available in daemon mode",
        ))
    }

    async fn list_sessions(
        &self,
        _request: tddy_rpc::Request<ListSessionsRequest>,
    ) -> Result<tddy_rpc::Response<ListSessionsResponse>, tddy_rpc::Status> {
        Err(tddy_rpc::Status::unimplemented(
            "ListSessions is only available in daemon mode",
        ))
    }
}
