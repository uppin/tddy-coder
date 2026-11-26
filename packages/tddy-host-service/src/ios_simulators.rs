//! The booted iOS Simulators on this Mac, offered as desktop channels.
//!
//! Listed through the `tddy-ios-emulator` bridge binary (`--list-devices`) rather than by linking
//! it, so this crate takes no dependency on the bridge's LiveKit stack, and a host with no Xcode,
//! or one that is not a Mac, lists none.

use std::path::PathBuf;

use tddy_service::proto::host::{DesktopChannelKind, HostRemoteDesktop, ProbeOutcome};

/// `screen_sharing.proto`'s `PROTOCOL_IOS_SIMULATOR`.
const PROTOCOL_IOS_SIMULATOR: i32 = 3;

/// One booted simulator, as the bridge binary lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootedSimulator {
    pub udid: String,
    pub name: String,
    /// Human-readable runtime, e.g. `iOS 18.2`.
    pub runtime: String,
}

/// Lists this host's booted simulators.
///
/// An `Err` is a host where simulators cannot be listed (no Xcode, not macOS, no bridge binary):
/// [`ios_simulator_channels`] lists none for it rather than failing the whole tooling probe.
pub trait SimulatorLister: Send + Sync {
    fn booted(&self) -> Result<Vec<BootedSimulator>, String>;
}

/// The desktop channel readings for the booted simulators `lister` reports.
///
/// `can_bridge` is whether the `tddy-ios-emulator` binary is present; it is the same for every
/// simulator on the host.
pub fn ios_simulator_channels(
    lister: &dyn SimulatorLister,
    can_bridge: bool,
) -> Vec<HostRemoteDesktop> {
    let booted = match lister.booted() {
        Ok(booted) => booted,
        Err(reason) => {
            log::debug!("ios_simulators: no simulators listed on this host: {reason}");
            return Vec::new();
        }
    };
    booted
        .into_iter()
        .map(|simulator| HostRemoteDesktop {
            outcome: ProbeOutcome::Ok as i32,
            protocol: PROTOCOL_IOS_SIMULATOR,
            can_bridge,
            // Listed as booted is what reachable means for a simulator: it has a screen to stream.
            desktop_reachable: true,
            port: 0,
            failure_reason: String::new(),
            channel_id: format!("ios:{}", simulator.udid),
            kind: DesktopChannelKind::IosSimulator as i32,
            display: 0,
            label: format!("{} · {}", simulator.name, simulator.runtime),
            device_id: simulator.udid,
        })
        .collect()
}

/// The production [`SimulatorLister`]: runs `<bridge> --list-devices`.
#[derive(Debug, Clone)]
pub struct BridgeBinarySimulatorLister {
    bridge: PathBuf,
}

impl BridgeBinarySimulatorLister {
    /// List through the `tddy-ios-emulator` binary at `bridge`.
    #[must_use]
    pub fn new(bridge: PathBuf) -> Self {
        Self { bridge }
    }
}

#[derive(serde::Deserialize)]
struct ListedDevice {
    udid: String,
    name: String,
    runtime: String,
}

/// Parse the one JSON line `tddy-ios-emulator --list-devices` prints.
pub fn parse_listed_devices(stdout: &str) -> Result<Vec<BootedSimulator>, String> {
    let listed: Vec<ListedDevice> = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("unreadable simulator list: {e}"))?;
    Ok(listed
        .into_iter()
        .map(|device| BootedSimulator {
            udid: device.udid,
            name: device.name,
            runtime: device.runtime,
        })
        .collect())
}

impl SimulatorLister for BridgeBinarySimulatorLister {
    /// Simulators run only on macOS, so only there is the bridge asked.
    #[cfg(target_os = "macos")]
    fn booted(&self) -> Result<Vec<BootedSimulator>, String> {
        let output = std::process::Command::new(&self.bridge)
            .arg("--list-devices")
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("running {} --list-devices: {e}", self.bridge.display()))?;
        if !output.status.success() {
            return Err(format!(
                "{} --list-devices exited with {}: {}",
                self.bridge.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        parse_listed_devices(&String::from_utf8_lossy(&output.stdout))
    }

    #[cfg(not(target_os = "macos"))]
    fn booted(&self) -> Result<Vec<BootedSimulator>, String> {
        Err("iOS Simulators run only on macOS".to_string())
    }
}
