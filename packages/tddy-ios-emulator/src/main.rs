//! tddy-ios-emulator bridge binary.
//!
//! Reads a JSON `BridgeConfig` from stdin (its `device` is the simulator udid) and streams that
//! simulator into LiveKit until it stops or an error occurs. With `--inspect` it reads an
//! `InspectTarget` and reports that a simulator asks for no sign-in.

use tddy_ios_emulator::IosSimulatorClient;
use tddy_screenshare::{run_bridge, run_inspect, BridgeConfig, BridgeMode, InspectTarget};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    match BridgeMode::from_args(std::env::args().skip(1))? {
        BridgeMode::Bridge => {
            let config: BridgeConfig = serde_json::from_reader(std::io::stdin())?;
            run_bridge::<IosSimulatorClient>(config).await
        }
        BridgeMode::Inspect => {
            let target: InspectTarget = serde_json::from_reader(std::io::stdin())?;
            run_inspect::<IosSimulatorClient>(target).await
        }
    }
}
