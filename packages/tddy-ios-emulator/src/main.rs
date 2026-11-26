//! tddy-ios-emulator bridge binary.
//!
//! Reads a JSON `BridgeConfig` from stdin (its `device` is the simulator udid) and streams that
//! simulator into LiveKit until it stops or an error occurs. With `--inspect` it reads an
//! `InspectTarget` and reports that a simulator asks for no sign-in. With `--list-devices` it
//! prints the booted simulators as one JSON line and exits.

use std::process::ExitCode;

use tddy_ios_emulator::{list_booted_devices, IosSimulatorClient, XcrunSimctl, LIST_DEVICES_FLAG};
use tddy_screenshare::{run_bridge, run_inspect, BridgeConfig, BridgeMode, InspectTarget};

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    env_logger::init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().map(String::as_str).eq([LIST_DEVICES_FLAG]) {
        return Ok(list_devices().await);
    }

    match BridgeMode::from_args(args.into_iter())? {
        BridgeMode::Bridge => {
            let config: BridgeConfig = serde_json::from_reader(std::io::stdin())?;
            run_bridge::<IosSimulatorClient>(config).await?;
        }
        BridgeMode::Inspect => {
            let target: InspectTarget = serde_json::from_reader(std::io::stdin())?;
            run_inspect::<IosSimulatorClient>(target).await?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Print the booted devices to stdout; on failure, the reason to stderr and a failing exit.
async fn list_devices() -> ExitCode {
    match list_booted_devices(&XcrunSimctl::default()).await {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("tddy-ios-emulator: listing simulators failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}
