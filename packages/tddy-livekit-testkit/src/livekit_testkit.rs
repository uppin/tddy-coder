//! LiveKit testcontainer implementation.
//!
//! When `LIVEKIT_TESTKIT_WS_URL` is set (e.g. `ws://127.0.0.1:12345`), connects to
//! that existing instance instead of starting a new container. Use
//! `run-livekit-testkit-server` to launch a reusable server and get the URL.

use anyhow::Result;
use livekit_api::access_token::{AccessToken, VideoGrants};
use livekit_api::services::room::RoomClient;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use testcontainers::core::wait::{HttpWaitStrategy, WaitFor};
use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use testcontainers::GenericImage;
use testcontainers::ImageExt;

/// The pinned server image, `name:tag`. One file is the source of truth, so the testkit, the CI
/// server script (`scripts/livekit-ci-server.sh`) and `run-livekit-testkit-server` cannot drift.
// TODO(shared-livekit-ci): the pin is not yet verified by two consecutive suite runs on CI.
const LIVEKIT_IMAGE_REF: &str = include_str!("../../../.config/livekit-server.image");
const DEV_API_KEY: &str = "devkey";
const DEV_API_SECRET: &str = "secret";
const API_READY_TIMEOUT: Duration = Duration::from_secs(15);
const API_READY_INTERVAL: Duration = Duration::from_millis(200);

/// Bind to :0 to let the OS assign a free TCP port; returns that port.
///
/// There is an inherent TOCTOU window between release and Docker binding, but the
/// window is tiny and much preferable to hardcoding well-known ports.
fn free_tcp_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind :0 to find free TCP port")
        .local_addr()
        .expect("local_addr")
        .port()
}

/// Same as [`free_tcp_port`] but for UDP.
fn free_udp_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .expect("bind :0 to find free UDP port")
        .local_addr()
        .expect("local_addr")
        .port()
}

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

        let (image_name, image_tag) = LIVEKIT_IMAGE_REF
            .trim()
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("livekit-server.image must be name:tag"))?;
        log::debug!(
            "LiveKitTestkit::start launching {}:{} container",
            image_name,
            image_tag
        );

        // Find three free ports on the host. LiveKit embeds its internal (container) port
        // numbers in ICE candidates, so the host port MUST equal the container port for
        // WebRTC to be reachable. We solve this by telling LiveKit to listen on the same
        // free port numbers we select, then mapping host:N → container:N for each.
        let port_ws = free_tcp_port();
        let port_ice_tcp = free_tcp_port();
        let port_ice_udp = free_udp_port();

        // Configure LiveKit ports via --config-body (inline YAML) for HTTP+ICE/TCP,
        // and via UDP_PORT env var for ICE/UDP (which does not accept a YAML key here).
        let config_body = format!("port: {port_ws}\nrtc:\n  tcp_port: {port_ice_tcp}\n");

        let http_wait = HttpWaitStrategy::new("/")
            .with_port(port_ws.tcp())
            .with_expected_status_code(200u16);

        let image = GenericImage::new(image_name, image_tag)
            .with_wait_for(WaitFor::from(http_wait))
            .with_cmd([
                "--dev",
                "--bind",
                "0.0.0.0",
                "--node-ip",
                "127.0.0.1",
                "--config-body",
                &config_body,
            ])
            .with_env_var("UDP_PORT", port_ice_udp.to_string())
            .with_mapped_port(port_ws, port_ws.tcp())
            .with_mapped_port(port_ice_tcp, port_ice_tcp.tcp())
            .with_mapped_port(port_ice_udp, port_ice_udp.udp());

        let container: testcontainers::ContainerAsync<GenericImage> = image.start().await?;
        let host_port = container.get_host_port_ipv4(port_ws.tcp()).await?;

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

    /// A room name no other test will use: `<prefix>-<hex nanos>-<pid>-<counter>`.
    ///
    /// Every test that talks to a LiveKit server names its room with this, so two tests can never
    /// share a room whether they run in one process, in two, or against one shared server
    /// (`LIVEKIT_TESTKIT_WS_URL`). The prefix is the room's purpose, so a room left behind by an
    /// aborted test can be attributed.
    pub fn unique_room(prefix: &str) -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since_epoch| since_epoch.as_nanos())
            .unwrap_or_default();
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{nanos:x}-{}-{counter}", std::process::id())
    }

    /// Get the WebSocket URL for connecting to the LiveKit server.
    pub fn get_ws_url(&self) -> String {
        self.ws_url.clone()
    }

    /// Evict `identity` from `room`, as the server does when it loses a participant: the participant
    /// leaves the room at once and everyone still in it is told.
    ///
    /// Dropping or aborting a client task is not that. A client that vanishes without a clean leave
    /// stays listed as `ACTIVE` until the server's own timeout expires (more than 12s on this server
    /// image), so a test that wants "the daemon is gone" must ask the server to remove it — and a
    /// daemon has more than one participant in the room, so every identity it joined with.
    pub async fn remove_participant(&self, room: &str, identity: &str) -> Result<()> {
        let http_url = format!("http://{}", self.ws_url.trim_start_matches("ws://"));
        RoomClient::with_api_key(&http_url, DEV_API_KEY, DEV_API_SECRET)
            .remove_participant(room, identity)
            .await
            .map_err(|e| anyhow::anyhow!("remove participant {identity} from {room}: {e}"))
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
                can_update_own_metadata: true,
                ..Default::default()
            })
            .to_jwt()?;
        Ok(token)
    }
}
