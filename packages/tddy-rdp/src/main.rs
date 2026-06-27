//! tddy-rdp bridge binary.
//!
//! Reads a JSON `BridgeConfig` from stdin (to avoid exposing credentials in argv/ps),
//! then runs the RDP↔LiveKit bridge until the session closes or an error occurs.

use tddy_rdp::rdp_client::RdpClient;
use tddy_screenshare::{run_bridge, BridgeConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config: BridgeConfig = serde_json::from_reader(std::io::stdin())?;
    run_bridge::<RdpClient>(config).await
}
