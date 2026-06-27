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

use crate::client::ScreenSharingClient;
use crate::streamer::ScreenSharingStreamer;

/// Configuration passed to a bridge binary via stdin as JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeConfig {
    /// Remote desktop host.
    pub host: String,
    /// Remote desktop port (e.g. 5900 for VNC, 3389 for RDP).
    pub port: u16,
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
pub async fn run_bridge<C: ScreenSharingClient>(config: BridgeConfig) -> Result<()> {
    info!(
        "bridge starting: target={} host={}:{} room={}",
        config.target_id, config.host, config.port, config.livekit_room
    );

    let password = if config.password.is_empty() {
        None
    } else {
        Some(config.password.as_str())
    };

    let mut client = C::connect(&config.host, config.port, password)
        .await
        .context("screen sharing connect failed")?;

    let (width, height) = wait_for_dimensions(&mut client).await?;
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
    .context("LiveKit connect failed")?;

    // Publish the video track on the same room connection
    let local = participant.room().local_participant().clone();
    let streamer =
        ScreenSharingStreamer::from_local_participant(local, &config.track_name, width, height)
            .await
            .context("failed to publish video track")?;

    info!("bridge: LiveKit track '{}' published", config.track_name);

    // Drive the RPC event loop in the background
    tokio::spawn(async move { participant.run().await });

    client.request_frame_update(false).await?;

    let frame_interval = Duration::from_millis(1000 / config.fps.max(1) as u64);
    let mut sigterm = signal(SignalKind::terminate()).context("failed to install SIGTERM handler")?;

    let result = pump_loop(&mut client, &streamer, frame_interval, &mut sigterm, input_rx).await;

    if let Err(e) = streamer.stop().await {
        warn!("bridge: streamer stop error: {}", e);
    }
    if let Err(e) = client.stop().await {
        warn!("bridge: client stop error: {}", e);
    }

    info!("bridge: exiting target={}", config.target_id);
    result
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
