//! LiveKit testcontainer implementation.

use anyhow::Result;
use livekit_api::access_token::{AccessToken, VideoGrants};
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

/// Manages a LiveKit server Docker container for use in tests.
pub struct LiveKitTestkit {
    _container: testcontainers::ContainerAsync<GenericImage>,
    host_port: u16,
}

impl LiveKitTestkit {
    /// Start a LiveKit server container.
    pub async fn start() -> Result<Self> {
        let http_wait = HttpWaitStrategy::new("/")
            .with_port(LIVEKIT_PORT.tcp())
            .with_expected_status_code(200u16);

        let image = GenericImage::new(LIVEKIT_IMAGE, LIVEKIT_TAG)
            .with_exposed_port(LIVEKIT_PORT.tcp())
            .with_wait_for(WaitFor::from(http_wait))
            .with_cmd(["--dev", "--bind", "0.0.0.0"]);

        let container: testcontainers::ContainerAsync<GenericImage> = image.start().await?;
        let host_port = container.get_host_port_ipv4(LIVEKIT_PORT.tcp()).await?;

        Ok(Self {
            _container: container,
            host_port,
        })
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
