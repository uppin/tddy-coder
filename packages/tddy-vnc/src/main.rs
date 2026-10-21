//! tddy-vnc bridge binary.
//!
//! Reads a JSON `BridgeConfig` from stdin (to avoid exposing credentials in argv/ps),
//! then runs the VNC↔LiveKit bridge until the target VNC connection closes or an error
//! occurs.
//!
//! With `--inspect` it instead reads an `InspectTarget` from stdin, writes one line naming the
//! sign-in methods the desktop advertises, and exits without signing in.

use tddy_screenshare::{run_bridge, run_inspect, BridgeConfig, BridgeMode, InspectTarget};
use tddy_vnc::vnc_client::VncClientState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    match BridgeMode::from_args(std::env::args().skip(1))? {
        BridgeMode::Bridge => {
            let config: BridgeConfig = serde_json::from_reader(std::io::stdin())?;
            run_bridge::<VncClientState>(config).await
        }
        BridgeMode::Inspect => {
            let target: InspectTarget = serde_json::from_reader(std::io::stdin())?;
            run_inspect::<VncClientState>(target).await
        }
    }
}
