//! tddy-vnc bridge binary.
//!
//! Reads a JSON `BridgeConfig` from stdin (to avoid exposing credentials in argv/ps),
//! then runs the VNC↔LiveKit bridge until the target VNC connection closes or an error
//! occurs.

use tddy_vnc::bridge::{run, BridgeConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // Read BridgeConfig from stdin as a single JSON object.
    let stdin = std::io::stdin();
    let config: BridgeConfig = serde_json::from_reader(stdin)?;

    run(config).await
}
