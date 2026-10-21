//! Generic screen-sharing bridge pump loop.
//!
//! Connects to a remote desktop via any `ScreenSharingClient`, joins LiveKit as a single
//! participant that serves both the H.264 video track and the `ScreenSharingInputService`
//! RPC over the data channel. Input events arrive via the bidi RPC stream and are injected
//! into the client; video frames are pushed to the LiveKit track on each timer tick.

use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt as _;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use tddy_livekit::{LiveKitParticipant, RoomOptions};
use tddy_rpc::{Request, Response, Status, Streaming};
use tddy_service::proto::screen_sharing_input::{
    screen_sharing_input_event::Event, ScreenSharingInputAck, ScreenSharingInputEvent,
    ScreenSharingInputService, ScreenSharingInputServiceServer,
};

use crate::client::{Credentials, ScreenSharingClient};
use crate::streamer::ScreenSharingStreamer;
use tddy_daemon_extensions::bridge_readiness::{
    write_readiness, AuthenticationRefused, BridgeReadiness, FailureKind,
};

/// Configuration passed to a bridge binary via stdin as JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeConfig {
    /// Remote desktop host.
    pub host: String,
    /// Remote desktop port (e.g. 5900 for VNC, 3389 for RDP).
    pub port: u16,
    /// Login username (required for RDP; empty for VNC/password-only targets).
    #[serde(default)]
    pub username: String,
    /// Windows domain the RDP account belongs to (empty for local accounts and for VNC).
    #[serde(default)]
    pub domain: String,
    /// Decrypted password (empty string for password-less targets).
    pub password: String,
    /// LiveKit server WebSocket URL.
    pub livekit_url: String,
    /// Pre-minted JWT token for the bridge participant.
    pub livekit_token: String,
    /// LiveKit room name.
    pub livekit_room: String,
    /// LiveKit participant identity for this bridge.
    pub livekit_identity: String,
    /// Video track name (e.g. `screenshare:<target_id>`).
    pub track_name: String,
    /// Target framebuffer width hint (actual dimensions come from the server).
    pub width: u32,
    /// Target framebuffer height hint.
    pub height: u32,
    /// Target ID (used for log context).
    pub target_id: String,
    /// Frames per second for the pump loop.
    #[serde(default = "default_fps")]
    pub fps: u32,
}

fn default_fps() -> u32 {
    30
}

impl BridgeConfig {
    /// The credentials the bridge signs in with: each empty field becomes absent.
    pub fn credentials(&self) -> Credentials {
        let present = |value: &str| Some(value.to_owned()).filter(|v| !v.is_empty());
        Credentials {
            username: present(&self.username),
            domain: present(&self.domain),
            password: present(&self.password),
        }
    }
}

/// Input command forwarded from the RPC data channel to the pump loop.
enum InputCmd {
    Pointer { x: u32, y: u32, button_mask: u32 },
    Key { keysym: u32, pressed: bool },
}

/// Implements `ScreenSharingInputService` by forwarding events to the pump loop channel.
struct InputForwarder {
    tx: mpsc::Sender<InputCmd>,
}

#[async_trait]
impl ScreenSharingInputService for InputForwarder {
    type StreamInputStream = ReceiverStream<Result<ScreenSharingInputAck, Status>>;

    async fn stream_input(
        &self,
        request: Request<Streaming<ScreenSharingInputEvent>>,
    ) -> Result<Response<Self::StreamInputStream>, Status> {
        let mut stream = request.into_inner();
        let cmd_tx = self.tx.clone();
        let (ack_tx, ack_rx) = mpsc::channel::<Result<ScreenSharingInputAck, Status>>(16);

        tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                let event = match item {
                    Ok(e) => e,
                    Err(e) => {
                        warn!("bridge: input stream error: {}", e);
                        break;
                    }
                };
                let cmd = match event.event {
                    Some(Event::Pointer(p)) => InputCmd::Pointer {
                        x: p.x,
                        y: p.y,
                        button_mask: p.button_mask,
                    },
                    Some(Event::Key(k)) => InputCmd::Key {
                        keysym: k.keysym,
                        pressed: k.pressed,
                    },
                    None => continue,
                };
                if cmd_tx.send(cmd).await.is_err() {
                    break;
                }
                let _ = ack_tx.send(Ok(ScreenSharingInputAck {})).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(ack_rx)))
    }
}

/// Run the bridge until the remote session closes, an error occurs, or SIGTERM is received.
///
/// Connects to the remote desktop and joins LiveKit as a single participant that publishes
/// the video track and serves `ScreenSharingInputService` over the data channel.
/// The `BridgeConfig` is typically read from stdin as JSON by the bridge binary.
///
/// Stdout carries the readiness line the daemon waits on (`crate::readiness`): exactly one, written
/// once the start has settled, reporting either the desktop's size or why it never came up.
pub async fn run_bridge<C: ScreenSharingClient>(config: BridgeConfig) -> Result<()> {
    info!(
        "bridge starting: target={} host={}:{} room={}",
        config.target_id, config.host, config.port, config.livekit_room
    );

    let started = match start_bridge::<C>(&config).await {
        Ok(started) => started,
        Err(failure) => {
            let readiness = BridgeReadiness::Failed {
                kind: failure.kind,
                message: failure.message,
            };
            if let Err(e) = report_readiness(&readiness) {
                warn!("bridge: could not report the failed start: {e}");
            }
            return Err(failure.error);
        }
    };

    report_readiness(&BridgeReadiness::Ready {
        width: started.width,
        height: started.height,
    })
    .context("reporting the desktop ready")?;

    let StartedBridge {
        mut client,
        streamer,
        input_rx,
        ..
    } = started;

    let frame_interval = Duration::from_millis(1000 / config.fps.max(1) as u64);
    let mut sigterm =
        signal(SignalKind::terminate()).context("failed to install SIGTERM handler")?;

    let result = pump_loop(
        &mut client,
        &streamer,
        frame_interval,
        &mut sigterm,
        input_rx,
    )
    .await;

    if let Err(e) = streamer.stop().await {
        warn!("bridge: streamer stop error: {}", e);
    }
    if let Err(e) = client.stop().await {
        warn!("bridge: client stop error: {}", e);
    }

    info!("bridge: exiting target={}", config.target_id);
    result
}

/// A bridge that has signed in, published its track, and asked for its first frame.
struct StartedBridge<C> {
    client: C,
    streamer: ScreenSharingStreamer,
    input_rx: mpsc::Receiver<InputCmd>,
    width: u32,
    height: u32,
}

/// A start that did not come up: the kind the daemon is told, what it is told, and the error the
/// bridge exits with.
struct StartFailure {
    kind: FailureKind,
    message: String,
    error: anyhow::Error,
}

/// Classify a start failure. A refused sign-in is [`FailureKind::Authentication`] wherever it sits in
/// the error chain, and its message is the desktop's own reason; anything else is `otherwise`, and
/// its message is the whole chain.
fn classify(error: anyhow::Error, otherwise: FailureKind) -> StartFailure {
    let refusal = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<AuthenticationRefused>())
        .map(|refusal| refusal.0.clone());
    match refusal {
        Some(reason) => StartFailure {
            kind: FailureKind::Authentication,
            message: reason,
            error,
        },
        None => StartFailure {
            kind: otherwise,
            message: format!("{error:#}"),
            error,
        },
    }
}

/// Write the readiness line the daemon is waiting on, to stdout.
fn report_readiness(readiness: &BridgeReadiness) -> std::io::Result<()> {
    write_readiness(&mut std::io::stdout().lock(), readiness)
}

/// Sign in to the desktop, publish its track, and request its first frame.
async fn start_bridge<C: ScreenSharingClient>(
    config: &BridgeConfig,
) -> Result<StartedBridge<C>, StartFailure> {
    let credentials = config.credentials();

    let mut client = C::connect(&config.host, config.port, &credentials)
        .await
        .context("screen sharing connect failed")
        .map_err(|e| classify(e, FailureKind::Connect))?;

    let (width, height) = wait_for_dimensions(&mut client)
        .await
        .map_err(|e| classify(e, FailureKind::Connect))?;
    info!("bridge: framebuffer {}x{}", width, height);

    // Channel: input RPC handler → pump loop
    let (input_tx, input_rx) = mpsc::channel::<InputCmd>(128);

    // Single LiveKit connection: serves ScreenSharingInputService over data channel
    let input_svc = ScreenSharingInputServiceServer::new(InputForwarder { tx: input_tx });
    let participant = LiveKitParticipant::connect(
        &config.livekit_url,
        &config.livekit_token,
        input_svc,
        RoomOptions::default(),
        None,
        None,
    )
    .await
    .context("LiveKit connect failed")
    .map_err(|e| classify(e, FailureKind::LiveKit))?;

    // Publish the video track on the same room connection
    let local = participant.room().local_participant().clone();
    let streamer =
        ScreenSharingStreamer::from_local_participant(local, &config.track_name, width, height)
            .await
            .context("failed to publish video track")
            .map_err(|e| classify(e, FailureKind::LiveKit))?;

    info!("bridge: LiveKit track '{}' published", config.track_name);

    // Drive the RPC event loop in the background
    tokio::spawn(async move { participant.run().await });

    client
        .request_frame_update(false)
        .await
        .map_err(|e| classify(e, FailureKind::Connect))?;

    Ok(StartedBridge {
        client,
        streamer,
        input_rx,
        width,
        height,
    })
}

/// Poll events until the client reports non-zero framebuffer dimensions.
async fn wait_for_dimensions<C: ScreenSharingClient>(client: &mut C) -> Result<(u32, u32)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for initial framebuffer dimensions");
        }
        client.poll_events().await?;
        let (w, h) = client.framebuffer_dimensions();
        if w > 0 && h > 0 {
            return Ok((w, h));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Main pump loop: drain input events → poll remote events → push frame → request update.
async fn pump_loop<C: ScreenSharingClient>(
    client: &mut C,
    streamer: &ScreenSharingStreamer,
    frame_interval: Duration,
    sigterm: &mut tokio::signal::unix::Signal,
    mut input_rx: mpsc::Receiver<InputCmd>,
) -> Result<()> {
    let mut next_frame = tokio::time::Instant::now() + frame_interval;

    loop {
        tokio::select! {
            biased;

            _ = sigterm.recv() => {
                info!("bridge: SIGTERM received, shutting down");
                return Ok(());
            }

            _ = tokio::time::sleep_until(next_frame) => {
                next_frame = tokio::time::Instant::now() + frame_interval;

                // Inject any pending input events before sampling the framebuffer.
                while let Ok(cmd) = input_rx.try_recv() {
                    match cmd {
                        InputCmd::Pointer { x, y, button_mask } => {
                            if let Err(e) = client.inject_pointer(x, y, button_mask).await {
                                error!("bridge: inject_pointer error: {}", e);
                                return Err(e);
                            }
                        }
                        InputCmd::Key { keysym, pressed } => {
                            if let Err(e) = client.inject_key(keysym, pressed).await {
                                error!("bridge: inject_key error: {}", e);
                                return Err(e);
                            }
                        }
                    }
                }

                // Drain all pending events from the remote.
                loop {
                    match client.poll_events().await {
                        Ok(true) => {}
                        Ok(false) => break,
                        Err(e) => {
                            error!("bridge: event error: {}", e);
                            return Err(e);
                        }
                    }
                }

                let (width, height) = client.framebuffer_dimensions();
                if width == 0 || height == 0 {
                    continue;
                }

                let fb = client.get_rgba_frame().to_vec();
                if fb.is_empty() {
                    continue;
                }

                if let Err(e) = streamer.push_rgba_frame(&fb, width, height).await {
                    error!("bridge: push_rgba_frame error: {}", e);
                    return Err(e);
                }

                if let Err(e) = client.request_frame_update(true).await {
                    error!("bridge: request_frame_update error: {}", e);
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::{json, Value};

    use super::*;

    /// The stdin JSON a daemon wrote before bridges knew about domains: no `domain` key.
    fn a_bridge_config_json() -> Value {
        json!({
            "host": "10.0.0.5",
            "port": 3389,
            "username": "alice",
            "password": "s3cret",
            "livekit_url": "ws://127.0.0.1:7880",
            "livekit_token": "token",
            "livekit_room": "room",
            "livekit_identity": "bridge",
            "track_name": "screenshare:desktop-1",
            "width": 1280,
            "height": 720,
            "target_id": "desktop-1",
        })
    }

    fn with_fields(mut config: Value, fields: Value) -> Value {
        let object = config
            .as_object_mut()
            .expect("bridge config is a JSON object");
        for (key, value) in fields.as_object().expect("fields are a JSON object") {
            object.insert(key.clone(), value.clone());
        }
        config
    }

    fn parsed(config: Value) -> BridgeConfig {
        serde_json::from_value(config).expect("bridge config parses")
    }

    #[test]
    fn a_bridge_config_without_a_domain_still_parses() {
        // Given
        let stdin = a_bridge_config_json();

        // When
        let config = parsed(stdin);

        // Then
        assert_eq!(
            config.credentials(),
            Credentials {
                username: Some("alice".into()),
                domain: None,
                password: Some("s3cret".into()),
            }
        );
    }

    #[test]
    fn a_bridge_config_with_a_domain_carries_it_into_the_credentials() {
        // Given
        let stdin = with_fields(a_bridge_config_json(), json!({ "domain": "CORP" }));

        // When
        let config = parsed(stdin);

        // Then
        assert_eq!(
            config.credentials(),
            Credentials {
                username: Some("alice".into()),
                domain: Some("CORP".into()),
                password: Some("s3cret".into()),
            }
        );
    }

    #[test]
    fn empty_config_fields_become_absent_credentials() {
        // Given
        let stdin = with_fields(
            a_bridge_config_json(),
            json!({ "username": "", "domain": "", "password": "" }),
        );

        // When
        let config = parsed(stdin);

        // Then
        assert_eq!(config.credentials(), Credentials::default());
    }
}
