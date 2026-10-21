use crate::chunking;
use crate::envelope::encode_response;
use crate::participant::RPC_TOPIC;
use crate::proto::RpcResponse;
use crate::rpc_trace;
use livekit::prelude::*;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

/// Shared publisher that survives room reconnection cycles.
/// Output tasks hold a clone and retry publishing through the latest `LocalParticipant`
/// when the room reconnects during token refresh.
#[derive(Clone)]
pub(crate) struct SharedPublisher {
    local: Arc<Mutex<Option<LocalParticipant>>>,
    notify: Arc<tokio::sync::Notify>,
}

impl SharedPublisher {
    pub(super) fn new() -> Self {
        Self {
            local: Arc::new(Mutex::new(None)),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub(super) async fn update(&self, lp: LocalParticipant) {
        *self.local.lock().await = Some(lp);
        self.notify.notify_waiters();
    }

    /// Publish data on `topic`, retrying with the latest LocalParticipant during reconnection gaps.
    ///
    /// `topic` is a parameter because the same indirection carries RPC responses ([`RPC_TOPIC`],
    /// addressed) and room-wide broadcasts ([`crate::BroadcastPublisher`], unaddressed): both need
    /// the current participant rather than the one that existed when they were wired up.
    pub(crate) async fn publish_data(
        &self,
        payload: Vec<u8>,
        topic: &str,
        destination_identities: &[ParticipantIdentity],
    ) -> Result<(), String> {
        for attempt in 0..30 {
            let local = { self.local.lock().await.clone() };
            if let Some(lp) = local {
                let packet = DataPacket {
                    payload: payload.clone(),
                    topic: Some(topic.to_string()),
                    reliable: true,
                    destination_identities: destination_identities.to_vec(),
                };
                if lp.publish_data(packet).await.is_ok() {
                    return Ok(());
                }
                if attempt == 0 {
                    log::debug!("[reconnect] publish_data failed, waiting for new participant");
                }
            }
            tokio::select! {
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        }
        Err("publish_data failed after 30 retries during reconnection".to_string())
    }
}

/// Spawn the response-drain task for a one-shot (non-reconnecting) connection ([`LiveKitParticipant::connect`]):
/// publishes directly through the given `LocalParticipant` snapshot.
pub(super) fn spawn_response_drain(
    mut outgoing_rx: mpsc::Receiver<(String, RpcResponse)>,
    local: LocalParticipant,
) {
    tokio::spawn(async move {
        while let Some((peer, response)) = outgoing_rx.recv().await {
            let request_id = response.request_id;
            match encode_response(response) {
                Ok(encoded) => {
                    // Fits-in-one-packet responses go out raw; oversized ones (e.g. a big
                    // StreamSessionActivity snapshot) are split into per-packet chunk frames.
                    let message_id = chunking::next_message_id();
                    let dest = vec![ParticipantIdentity::from(peer)];
                    let mut publish_error = None;
                    for frame in chunking::frame_for_transport(message_id, &encoded) {
                        let packet = DataPacket {
                            payload: frame,
                            topic: Some(RPC_TOPIC.to_string()),
                            reliable: true,
                            destination_identities: dest.clone(),
                        };
                        if let Err(e) = local.publish_data(packet).await {
                            publish_error = Some(e);
                            break;
                        }
                    }
                    if let Some(e) = publish_error {
                        log::error!(
                            "[echo_server] RPC request_id={} publish failed: {}",
                            request_id,
                            e
                        );
                    } else {
                        rpc_trace!(
                            "LiveKitParticipant: published response request_id={}",
                            request_id
                        );
                    }
                }
                Err(e) => {
                    log::error!(
                        "[echo_server] RPC request_id={} response encode failed: {}",
                        request_id,
                        e
                    );
                }
            }
        }
        log::debug!("LiveKitParticipant: response drain task ended");
    });
}

/// Spawn the response-drain task for a reconnect-resilient connection ([`LiveKitParticipant::run_with_reconnect_metadata`]):
/// publishes through `shared_publisher`, which retries against whatever `LocalParticipant` is
/// current across reconnection cycles.
pub(super) fn spawn_response_drain_reconnectable(
    mut outgoing_rx: mpsc::Receiver<(String, RpcResponse)>,
    shared_publisher: SharedPublisher,
) {
    tokio::spawn(async move {
        while let Some((peer, response)) = outgoing_rx.recv().await {
            let request_id = response.request_id;
            match encode_response(response) {
                Ok(encoded) => {
                    // See spawn_response_drain: raw when it fits, chunk frames when oversized.
                    let message_id = chunking::next_message_id();
                    let dest = [ParticipantIdentity::from(peer)];
                    let mut publish_error = None;
                    for frame in chunking::frame_for_transport(message_id, &encoded) {
                        if let Err(e) = shared_publisher.publish_data(frame, RPC_TOPIC, &dest).await
                        {
                            publish_error = Some(e);
                            break;
                        }
                    }
                    if let Some(e) = publish_error {
                        log::error!(
                            "[echo_server] RPC request_id={} reconnectable publish failed: {}",
                            request_id,
                            e
                        );
                    } else {
                        rpc_trace!(
                            "LiveKitParticipant: published response (reconnectable) request_id={}",
                            request_id
                        );
                    }
                }
                Err(e) => {
                    log::error!(
                        "[echo_server] RPC request_id={} response encode failed: {}",
                        request_id,
                        e
                    );
                }
            }
        }
        log::debug!("LiveKitParticipant: response drain task ended");
    });
}
