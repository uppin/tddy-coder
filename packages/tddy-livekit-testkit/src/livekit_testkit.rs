//! LiveKit testcontainer implementation.
//!
//! When `LIVEKIT_TESTKIT_WS_URL` is set (e.g. `ws://127.0.0.1:12345`), connects to
//! that existing instance instead of starting a new container. Use
//! `run-livekit-testkit-server` to launch a reusable server and get the URL.

use anyhow::Result;
use livekit_api::access_token::{AccessToken, VideoGrants};
use livekit_api::services::room::RoomClient;
use std::time::Duration;
use testcontainers::core::wait::{HttpWaitStrategy, WaitFor};
use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use testcontainers::GenericImage;
use testcontainers::ImageExt;

const LIVEKIT_IMAGE: &str = "livekit/livekit-server";
const LIVEKIT_TAG: &str = "master";
const LIVEKIT_PORT: u16 = 7880;
/// ICE/TCP fallback port (required for WebRTC).
const LIVEKIT_ICE_TCP_PORT: u16 = 7881;
/// ICE/UDP mux port (required for WebRTC).
const LIVEKIT_ICE_UDP_PORT: u16 = 7882;
const DEV_API_KEY: &str = "devkey";
const DEV_API_SECRET: &str = "secret";
const API_READY_TIMEOUT: Duration = Duration::from_secs(15);
const API_READY_INTERVAL: Duration = Duration::from_millis(200);

/// Env var to reuse an existing LiveKit server instead of starting a container.
/// Value: `ws://HOST:PORT` (e.g. `ws://127.0.0.1:54321`).
pub const LIVEKIT_TESTKIT_WS_URL_ENV: &str = "LIVEKIT_TESTKIT_WS_URL";

fn parse_ws_url(ws_url: &str) -> Result<(String, u16)> {
    let after_scheme = ws_url
        .strip_prefix("ws://")
        .or_else(|| ws_url.strip_prefix("wss://"))
        .ok_or_else(|| anyhow::anyhow!("Invalid URL: expected ws:// or wss://, got {}", ws_url))?;
    let (host, port_str) = after_scheme
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("Invalid URL: no port in {}", ws_url))?;
    let port: u16 = port_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid port in {}: {}", ws_url, port_str))?;
    Ok((host.to_string(), port))
}

/// Manages a LiveKit server Docker container for use in tests.
///
/// When `LIVEKIT_TESTKIT_WS_URL` is set, uses that instance (no container lifecycle).
pub struct LiveKitTestkit {
    _container: Option<testcontainers::ContainerAsync<GenericImage>>,
    ws_url: String,
}

impl LiveKitTestkit {
    /// Start a LiveKit server container, or connect to an existing one if
    /// `LIVEKIT_TESTKIT_WS_URL` is set.
    ///
    /// Blocks until the server's Twirp API is fully responsive (not just HTTP).
    pub async fn start() -> Result<Self> {
        if let Ok(ws_url) = std::env::var(LIVEKIT_TESTKIT_WS_URL_ENV) {
            let ws_url = ws_url.trim().to_string();
            if !ws_url.is_empty() {
                log::debug!(
                    "LiveKitTestkit::start reusing existing instance from {}",
                    LIVEKIT_TESTKIT_WS_URL_ENV
                );
                let (host, port) = parse_ws_url(&ws_url)?;
                let http_url = format!("http://{}:{}", host, port);
                Self::wait_for_api_url_async(&http_url).await?;
                log::debug!("LiveKitTestkit: API ready at {}", ws_url);
                return Ok(Self {
                    _container: None,
                    ws_url,
                });
            }
        }

        log::debug!(
            "LiveKitTestkit::start launching {}:{} container",
            LIVEKIT_IMAGE,
            LIVEKIT_TAG
        );

        let http_wait = HttpWaitStrategy::new("/")
            .with_port(LIVEKIT_PORT.tcp())
            .with_expected_status_code(200u16);

        // Use ephemeral host ports to avoid conflicts when 7880 is already in use
        // (e.g. from ./run-livekit-testkit-server). Set LIVEKIT_TESTKIT_WS_URL to reuse.
        let image = GenericImage::new(LIVEKIT_IMAGE, LIVEKIT_TAG)
            .with_exposed_port(LIVEKIT_PORT.tcp())
            .with_exposed_port(LIVEKIT_ICE_TCP_PORT.tcp())
            .with_exposed_port(LIVEKIT_ICE_UDP_PORT.udp())
            .with_wait_for(WaitFor::from(http_wait))
            .with_cmd(["--dev", "--bind", "0.0.0.0", "--node-ip", "127.0.0.1"]);

        let container: testcontainers::ContainerAsync<GenericImage> = image.start().await?;
        let host_port = container.get_host_port_ipv4(LIVEKIT_PORT.tcp()).await?;

        log::debug!(
            "LiveKitTestkit: HTTP ready on port {}, probing API...",
            host_port
        );

        Self::wait_for_api(host_port).await?;

        log::debug!("LiveKitTestkit: API ready on port {}", host_port);

        let ws_url = format!("ws://127.0.0.1:{}", host_port);

        Ok(Self {
            _container: Some(container),
            ws_url,
        })
    }

    async fn wait_for_api_url_async(http_url: &str) -> Result<()> {
        let client = RoomClient::with_api_key(http_url, DEV_API_KEY, DEV_API_SECRET);

        tokio::time::timeout(API_READY_TIMEOUT, async {
            loop {
                match client.list_rooms(vec![]).await {
                    Ok(_) => return,
                    Err(e) => {
                        log::debug!("LiveKitTestkit: API not ready yet: {}", e);
                        tokio::time::sleep(API_READY_INTERVAL).await;
                    }
                }
            }
        })
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "LiveKit API at {} did not become ready within {:?}",
                http_url,
                API_READY_TIMEOUT
            )
        })
    }

    /// Poll ListRooms until the Twirp API responds, proving the full server
    /// stack (including RTC engine) has initialized.
    async fn wait_for_api(host_port: u16) -> Result<()> {
        let url = format!("http://127.0.0.1:{}", host_port);
        Self::wait_for_api_url_async(&url).await
    }

    /// Get the WebSocket URL for connecting to the LiveKit server.
    pub fn get_ws_url(&self) -> String {
        self.ws_url.clone()
    }

    /// Generate an access token for a participant to join a room.
    pub fn generate_token(&self, room: &str, identity: &str) -> Result<String> {
        let token = AccessToken::with_api_key(DEV_API_KEY, DEV_API_SECRET)
            .with_identity(identity)
            .with_ttl(std::time::Duration::from_secs(3600))
            .with_grants(VideoGrants {
                room_join: true,
                room: room.to_string(),
                can_publish: true,
                can_subscribe: true,
                ..Default::default()
            })
            .to_jwt()?;
        Ok(token)
    }
}
