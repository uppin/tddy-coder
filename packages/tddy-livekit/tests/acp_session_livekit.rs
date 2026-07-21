//! End-to-end: drive `AcpService.Session` over a **real LiveKit hop** (the testkit's dev server or a
//! reused `LIVEKIT_TESTKIT_WS_URL`). This is the transport-level proof for the user's requirement —
//! "the ACP mirror should be done via the LiveKit session connection" — complementing the in-memory
//! `tddy_rpc` acceptance in `tddy-service`. A server participant mounts the production
//! `session_view_adapter_surface` (which includes `tddy.acp.v1.AcpService`); a client participant
//! opens the `Session` bidi RPC over the LiveKit data channel, sends `initialize`, and reads back the
//! correlated `InitializeResponse`, then sees a live `AgentOutput` as an `AgentMessageChunk`.
//!
//! Requires a LiveKit server: `./run-livekit-testkit-server` (sets `LIVEKIT_TESTKIT_WS_URL`) or
//! Docker for testcontainers — same gating as the other `tddy-livekit` tests.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use livekit::prelude::*;
use prost::Message;
use serial_test::serial;

use tddy_core::{
    AnyBackend, Presenter, PresenterEvent, SharedBackend, StubBackend, ViewConnection,
};
use tddy_livekit::{LiveKitParticipant, RpcClient};
use tddy_livekit_testkit::LiveKitTestkit;
use tddy_service::proto::acp::{
    acp_agent_message, acp_client_message, content_block, session_update, AcpAgentMessage,
    AcpClientMessage, InitializeRequest,
};
use tddy_service::session_view_adapter_surface;
use tddy_workflow_recipes::TddRecipe;

const SERVER_IDENTITY: &str = "server";
const CLIENT_IDENTITY: &str = "client";
const PARTICIPANT_TIMEOUT: Duration = Duration::from_secs(10);

type ViewFactory = Arc<dyn Fn() -> Option<ViewConnection> + Send + Sync>;

/// Block until `identity` appears as a remote participant (copied from `rpc_scenarios.rs`).
async fn wait_for_participant(
    room: &Room,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    identity: &str,
) -> Result<()> {
    let target: ParticipantIdentity = identity.to_string().into();
    if room.remote_participants().contains_key(&target) {
        return Ok(());
    }
    tokio::time::timeout(PARTICIPANT_TIMEOUT, async {
        while let Some(event) = events.recv().await {
            if let RoomEvent::ParticipantConnected(p) = event {
                if p.identity() == target {
                    return;
                }
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("timed out waiting for participant '{}'", identity))?;
    Ok(())
}

/// Guard that stops the presenter poll thread when dropped.
struct PresenterPollGuard {
    shutdown: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Drop for PresenterPollGuard {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// A real Presenter (StubBackend + TddRecipe) on a background poll thread — the view source the ACP
/// service needs (its `session()` opens a view before replying). Adapted from the `tddy-service`
/// `a_running_presenter` helper; uses a timestamp-suffixed temp dir instead of a uuid dep.
fn a_running_presenter() -> (
    tokio::sync::broadcast::Sender<PresenterEvent>,
    ViewFactory,
    PresenterPollGuard,
) {
    use std::sync::mpsc;
    let (event_tx, _) = tokio::sync::broadcast::channel(256);
    let event_tx_for_test = event_tx.clone();
    let (intent_tx, intent_rx) = mpsc::channel();

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tddy_data_dir = std::env::temp_dir().join(format!("tddy-acp-lk-{nanos}"));
    std::fs::create_dir_all(&tddy_data_dir).unwrap();
    let mut presenter = Presenter::new("stub", "opus", Arc::new(TddRecipe), tddy_data_dir)
        .with_broadcast(event_tx)
        .with_intent_sender(intent_tx);
    let backend = SharedBackend::from_any(AnyBackend::Stub(StubBackend::new()));
    let output_dir = std::env::temp_dir().join(format!("tddy-acp-lk-out-{nanos}"));
    std::fs::create_dir_all(&output_dir).unwrap();
    presenter.start_workflow(
        backend, output_dir, None, None, None, None, false, None, None, None,
    );
    let presenter = Arc::new(Mutex::new(presenter));

    let shutdown = Arc::new(AtomicBool::new(false));
    let join = std::thread::spawn({
        let shutdown = shutdown.clone();
        let presenter = presenter.clone();
        move || {
            for _ in 0..1000 {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                if let Ok(mut p) = presenter.lock() {
                    while let Ok(intent) = intent_rx.try_recv() {
                        p.handle_intent(intent);
                    }
                    p.poll_workflow();
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    });

    let view_factory: ViewFactory = {
        let presenter = presenter.clone();
        Arc::new(move || presenter.lock().ok().and_then(|p| p.connect_view()))
    };
    (
        event_tx_for_test,
        view_factory,
        PresenterPollGuard {
            shutdown,
            join: Some(join),
        },
    )
}

/// Read the next non-empty `AcpAgentMessage` from the client bidi stream (skips the EOS frame).
async fn recv_agent_msg(
    rx: &mut tokio::sync::mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
) -> Result<AcpAgentMessage> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let chunk = rx
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("session stream closed unexpectedly"))?;
            let bytes = chunk.map_err(|e| anyhow::anyhow!("session frame error: {}", e))?;
            if !bytes.is_empty() {
                return AcpAgentMessage::decode(&bytes[..]).map_err(Into::into);
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("timeout waiting for an AcpAgentMessage"))?
}

#[tokio::test]
#[serial]
async fn acp_session_over_real_livekit_handshakes_and_streams_agent_output() -> Result<()> {
    // (a) A LiveKit room via the testkit (reuses LIVEKIT_TESTKIT_WS_URL or starts a container).
    let livekit = LiveKitTestkit::start().await?;
    let url = livekit.get_ws_url();
    let room_name = "acp-session-scenarios";
    let server_token = livekit.generate_token(room_name, SERVER_IDENTITY)?;
    let client_token = livekit.generate_token(room_name, CLIENT_IDENTITY)?;

    // (b) Server participant mounts the production surface (includes tddy.acp.v1.AcpService).
    let (event_tx, view_factory, _guard) = a_running_presenter();
    let surface = session_view_adapter_surface(vec![], view_factory);
    let server = LiveKitParticipant::connect(
        &url,
        &server_token,
        surface,
        RoomOptions::default(),
        None,
        None,
    )
    .await?;
    let server_handle = tokio::spawn(async move { server.run().await });

    // Client participant + RpcClient targeting the server identity.
    let (client_room, mut client_events) =
        Room::connect(&url, &client_token, RoomOptions::default())
            .await
            .map_err(|e| anyhow::anyhow!("client connect: {}", e))?;
    let rpc_events = client_room.subscribe();
    wait_for_participant(&client_room, &mut client_events, SERVER_IDENTITY).await?;
    let rpc_client = RpcClient::new(client_room, SERVER_IDENTITY.to_string(), rpc_events);

    // (c) Open AcpService.Session and send `initialize` (id = 1), keeping the stream open.
    let (mut sender, mut rx) = rpc_client
        .start_bidi_stream("tddy.acp.v1.AcpService", "Session")
        .map_err(|e| anyhow::anyhow!("start Session: {}", e))?;
    let init = AcpClientMessage {
        id: 1,
        msg: Some(acp_client_message::Msg::Initialize(
            InitializeRequest::default(),
        )),
    };
    sender
        .send(init.encode_to_vec(), false)
        .await
        .map_err(|e| anyhow::anyhow!("send initialize: {}", e))?;

    // (d) The handshake replies with a correlated InitializeResponse.
    let reply = recv_agent_msg(&mut rx).await?;
    assert_eq!(reply.id, 1, "InitializeResponse must echo the request id");
    assert!(
        matches!(reply.msg, Some(acp_agent_message::Msg::Initialize(_))),
        "first reply must be InitializeResponse, got {:?}",
        reply.msg
    );

    // (e) A live agent output reaches the ACP client as an AgentMessageChunk over LiveKit.
    event_tx
        .send(PresenterEvent::AgentOutput("hello over livekit".into()))
        .expect("broadcast agent output");
    let mut got_text = None;
    for _ in 0..20 {
        let m = recv_agent_msg(&mut rx).await?;
        if let Some(acp_agent_message::Msg::SessionUpdate(notif)) = m.msg {
            if let Some(session_update::Update::AgentMessageChunk(chunk)) =
                notif.update.and_then(|u| u.update)
            {
                if let Some(content_block::Block::Text(t)) = chunk.content.and_then(|c| c.block) {
                    got_text = Some(t.text);
                    break;
                }
            }
        }
    }
    assert_eq!(
        got_text.as_deref(),
        Some("hello over livekit"),
        "live AgentOutput must arrive as an AgentMessageChunk over the LiveKit AcpService.Session"
    );

    server_handle.abort();
    Ok(())
}
