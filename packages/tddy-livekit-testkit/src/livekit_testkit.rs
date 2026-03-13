//! LiveKit testcontainer implementation.

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
const DEV_API_KEY: &str = "devkey";
const DEV_API_SECRET: &str = "secret";
const API_READY_TIMEOUT: Duration = Duration::from_secs(15);
const API_READY_INTERVAL: Duration = Duration::from_millis(200);

/// Manages a LiveKit server Docker container for use in tests.
pub struct LiveKitTestkit {
    _container: testcontainers::ContainerAsync<GenericImage>,
    host_port: u16,
}

impl LiveKitTestkit {
    /// Start a LiveKit server container.
    ///
    /// Blocks until the server's Twirp API is fully responsive (not just HTTP).
    pub async fn start() -> Result<Self> {
        log::debug!(
            "LiveKitTestkit::start launching {}:{} container",
            LIVEKIT_IMAGE,
            LIVEKIT_TAG
        );

        let http_wait = HttpWaitStrategy::new("/")
            .with_port(LIVEKIT_PORT.tcp())
            .with_expected_status_code(200u16);

        let image = GenericImage::new(LIVEKIT_IMAGE, LIVEKIT_TAG)
            .with_exposed_port(LIVEKIT_PORT.tcp())
            .with_wait_for(WaitFor::from(http_wait))
            .with_cmd(["--dev", "--bind", "0.0.0.0"]);

        let container: testcontainers::ContainerAsync<GenericImage> = image.start().await?;
        let host_port = container.get_host_port_ipv4(LIVEKIT_PORT.tcp()).await?;

        log::debug!(
            "LiveKitTestkit: HTTP ready on port {}, probing API...",
            host_port
        );

        Self::wait_for_api(host_port).await?;

        log::debug!("LiveKitTestkit: API ready on port {}", host_port);

        Ok(Self {
            _container: container,
            host_port,
        })
    }

    /// Poll ListRooms until the Twirp API responds, proving the full server
    /// stack (including RTC engine) has initialized.
    async fn wait_for_api(host_port: u16) -> Result<()> {
        let url = format!("http://127.0.0.1:{}", host_port);
        let client = RoomClient::with_api_key(&url, DEV_API_KEY, DEV_API_SECRET);

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
        .map_err(|_| anyhow::anyhow!(
            "LiveKit API did not become ready within {:?}",
            API_READY_TIMEOUT
        ))
    }

    /// Get the WebSocket URL for connecting to the LiveKit server.
    pub fn get_ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.host_port)
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
