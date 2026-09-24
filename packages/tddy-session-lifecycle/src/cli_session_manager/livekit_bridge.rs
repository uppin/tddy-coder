use async_trait::async_trait;
use prost::Message as _;
use tddy_livekit::LiveKitParticipant;

use tddy_livekit::TokenGenerator;

use tddy_rpc::ResponseBody;

use tddy_service::proto::terminal::TerminalInput;

use tddy_service::proto::terminal::TerminalOutput;

use tddy_rpc::BidiStreamOutput;

use tddy_livekit::RpcResult;

use tddy_rpc::RpcMessage;

use tddy_livekit::RpcService;
use tokio::sync::{broadcast, mpsc, watch};

use std::sync::Arc;

use bytes::Bytes;

use crate::cli_session_manager::pty_handle;

/// Strip an OSC resize sequence (`\x1b]resize;{cols};{rows}\x07`) from `data`.
///
/// Returns `(Some((cols, rows)), remaining)` when found, or `(None, original)` otherwise.
/// The escape sequence is removed from the returned bytes so it is not forwarded to the PTY stdin.
pub(super) fn strip_resize(data: &[u8]) -> (Option<(u16, u16)>, Bytes) {
    let prefix = b"\x1b]resize;";
    let start = match (0..data.len().saturating_sub(prefix.len()))
        .find(|&i| data[i..].starts_with(prefix))
    {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let after = &data[start + prefix.len()..];
    let bel = match after.iter().position(|&b| b == 0x07) {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let inner = &after[..bel];
    let semi = match inner.iter().position(|&b| b == b';') {
        Some(i) => i,
        None => return (None, Bytes::copy_from_slice(data)),
    };
    let parsed = std::str::from_utf8(&inner[..semi])
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .zip(
            std::str::from_utf8(&inner[semi + 1..])
                .ok()
                .and_then(|s| s.parse::<u16>().ok()),
        );
    match parsed {
        Some((cols, rows)) => {
            let end = start + prefix.len() + bel + 1;
            let mut remaining = data[..start].to_vec();
            remaining.extend_from_slice(&data[end..]);
            (Some((cols, rows)), Bytes::from(remaining))
        }
        None => (None, Bytes::copy_from_slice(data)),
    }
}

/// LiveKit RPC service that bridges `terminal.TerminalService/StreamTerminalIO` to a PTY handle.
struct PtyLiveKitService {
    pub(crate) handle: Arc<pty_handle::PtyHandle>,
}

#[async_trait]
impl RpcService for PtyLiveKitService {
    fn is_bidi_stream(&self, service: &str, method: &str) -> bool {
        service == "terminal.TerminalService" && method == "StreamTerminalIO"
    }

    async fn handle_rpc(&self, _service: &str, _method: &str, _msg: &RpcMessage) -> RpcResult {
        RpcResult::Unary(Err(tddy_rpc::Status::unimplemented("use bidi stream")))
    }

    async fn start_bidi_stream(
        &self,
        service: &str,
        method: &str,
        _metadata: tddy_rpc::RequestMetadata,
        mut input_rx: mpsc::Receiver<RpcMessage>,
    ) -> Result<BidiStreamOutput, tddy_rpc::Status> {
        if service != "terminal.TerminalService" || method != "StreamTerminalIO" {
            return Err(tddy_rpc::Status::not_found(format!(
                "{}/{}",
                service, method
            )));
        }

        let (out_tx, out_rx) = mpsc::channel::<Result<Vec<u8>, tddy_rpc::Status>>(256);

        // Replay the mouse-tracking prologue plus all output since session start, so the client's
        // own VT ends up in the same modes the application enabled — even when the bytes that
        // enabled them have long been evicted from the capture ring.
        if let Ok(cap) = self.handle.capture.lock() {
            let replay = cap.replay();
            if !replay.is_empty() {
                let frame = TerminalOutput { data: replay }.encode_to_vec();
                let _ = out_tx.try_send(Ok(frame));
            }
        }

        // PTY stdout → bidi output stream. Breaks when:
        // - broadcast sender closed (session dropped)
        // - pty_done watch fires (reader exited; no more output coming)
        // - out_tx_clone send fails (client disconnected)
        let mut stdout_rx = self.handle.stdout_tx.subscribe();
        let mut pty_done = self.handle.pty_done.clone();
        let out_tx_clone = out_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = stdout_rx.recv() => {
                        match result {
                            Ok(bytes) => {
                                let frame = TerminalOutput { data: bytes.to_vec() }.encode_to_vec();
                                if out_tx_clone.send(Ok(frame)).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                log::warn!(
                                    target: "tddy_daemon::claude_cli_session",
                                    "LiveKit bridge: PTY output lagged {} messages",
                                    n
                                );
                            }
                        }
                    }
                    // PTY reader exited — drain any remaining broadcast items then stop.
                    _ = pty_done.changed() => break,
                }
            }
        });

        // Bidi input stream → PTY stdin. Resize escape sequences are intercepted and applied
        // to the PTY via SIGWINCH rather than forwarded as raw bytes.
        let handle_for_input = Arc::clone(&self.handle);
        tokio::spawn(async move {
            while let Some(msg) = input_rx.recv().await {
                if let Ok(input) = TerminalInput::decode(&msg.payload[..]) {
                    if !input.data.is_empty() {
                        // VirtualTui `TerminalInput` carries no offset; 0 = unset (no ACK).
                        handle_for_input.send_input(bytes::Bytes::from(input.data), 0);
                    }
                }
            }
        });

        // Trigger a SIGWINCH so the TUI repaints for the new subscriber.
        self.handle.trigger_redraw();

        Ok(BidiStreamOutput {
            output: ResponseBody::Streaming(out_rx),
        })
    }
}

/// How often a claude-cli session re-publishes its `session` participant block.
///
/// Matches the owned-project-count poller's 30s cadence in `tddy-livekit`, and for the same reason:
/// it is a bounded refresh of a value the room must not silently lose, not a live feed. Nothing in
/// the block changes between ticks — what changes is whether the last publish landed.
const SESSION_METADATA_REPUBLISH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Spawn a LiveKit participant that bridges a PTY session to the LiveKit room.
///
/// `session_metadata` is the `session` block this participant publishes about itself. It is what a
/// PR-Stack view on **another host** has to work with: `ListSessions` does not fan out, so a row for
/// a session running here is synthesized one host over from the common room's participants and
/// hydrated from exactly this block. A claude-cli session that publishes none arrives there as a
/// short session id with no branch, no orchestrator and no node — which is how a planned PR's child
/// started on the wrong host went missing from the row that started it (D37).
///
/// Published at spawn and re-published unchanged on [`SESSION_METADATA_REPUBLISH_INTERVAL`], because
/// a value published once is a value a single failed `set_metadata` — or a room rejoin — loses for
/// the life of the session. Static fields only: the daemon knows this session's identity, its stack
/// association and its static fields, and nothing about the agent's progress, so the live workflow
/// fields (`workflow_goal`, `workflow_state`, `activity_status`, …) stay empty for a claude-cli
/// session, exactly as they were when nothing was published at all. Filling them means a workflow
/// tap the way `tddy-coder` has one — logged in `docs/dev/TODO.md` § Future Enhancements, not closed
/// here.
///
/// Returns the task running the participant, so a caller that tracks its session's bridge can tell
/// a live one from one whose room or PTY has since ended.
#[allow(clippy::too_many_arguments)]
pub async fn spawn_livekit_bridge(
    handle: Arc<pty_handle::PtyHandle>,
    livekit_url: &str,
    room_name: &str,
    api_key: &str,
    api_secret: &str,
    server_identity: &str,
    session_metadata: Option<tddy_core::session_participant_metadata::SessionParticipantMetadata>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let token = TokenGenerator::new(
        api_key.to_string(),
        api_secret.to_string(),
        room_name.to_string(),
        server_identity.to_string(),
        std::time::Duration::from_secs(86400),
    )
    .generate()
    .map_err(|e| anyhow::anyhow!("token generate: {}", e))?;

    let service = PtyLiveKitService { handle };
    let participant =
        LiveKitParticipant::connect(livekit_url, &token, service, Default::default(), None, None)
            .await
            .map_err(|e| anyhow::anyhow!("LiveKitParticipant::connect: {}", e))?;

    let identity_owned = server_identity.to_string();
    // Taken before the event loop consumes the participant, and shared with the participant's own
    // metadata publishers so two writers never race a `set_metadata` past each other.
    let local = participant.room().local_participant().clone();
    let publish_lock = participant.metadata_publish_lock();
    let metadata_json = session_metadata
        .as_ref()
        .map(tddy_core::session_participant_metadata::session_metadata_json);
    let serving = tokio::spawn(async move {
        // The watcher publishes on *change*, so the channel starts empty and the block is sent
        // after the receiver exists: a value put into a channel before anyone subscribed is the
        // subscriber's already-seen initial value and would never reach the room.
        //
        // Re-sent on an interval rather than once, on the shape the participant's other publishers
        // already use (the codex-OAuth and owned-project-count pollers republish on a timer for
        // this reason). A single `set_metadata` failure is logged and dropped by the watcher, and a
        // room rejoin starts the participant's wire metadata over — so a session that publishes one
        // value and never another loses its cross-host association permanently on either, which is
        // the exact symptom this block exists to remove. `tddy-coder` needs no such timer because
        // every workflow transition re-sends on the same channel; a claude-cli session has no
        // workflow tap and so no other occasion to publish.
        let republishing = metadata_json.map(|json| {
            let (tx, rx) = watch::channel(String::new());
            tddy_livekit::spawn_local_participant_metadata_watcher(rx, local, publish_lock);
            tokio::spawn(async move {
                let mut ticks = tokio::time::interval(SESSION_METADATA_REPUBLISH_INTERVAL);
                loop {
                    // The first tick completes immediately, so the block reaches the room at spawn
                    // and every interval thereafter.
                    ticks.tick().await;
                    // `watch::Sender::send` marks the value changed whether or not it differs, so
                    // each tick is a real publish and not a no-op the watcher would skip. It fails
                    // only once the watcher is gone, which is the end of this publisher too.
                    if tx.send(json.clone()).is_err() {
                        break;
                    }
                }
            })
        });
        participant.run().await;
        // The room is gone, so there is nothing left to advertise to.
        if let Some(republishing) = republishing {
            republishing.abort();
        }
        log::info!(
            target: "tddy_daemon::claude_cli_session",
            "LiveKit bridge participant exited for identity {}",
            identity_owned
        );
    });

    Ok(serving)
}
