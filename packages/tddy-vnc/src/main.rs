//! tddy-vnc bridge binary.
//!
//! Reads a JSON `BridgeConfig` from stdin (to avoid exposing credentials in argv/ps),
//! then runs the VNC↔LiveKit bridge until the target VNC connection closes or an error
//! occurs.

use tddy_screenshare::{run_bridge, BridgeConfig};
use tddy_vnc::vnc_client::VncClientState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config: BridgeConfig = serde_json::from_reader(std::io::stdin())?;
    run_bridge::<VncClientState>(config).await
}
