//! The `simctl` port: which simulators exist, and what state each is in.

use std::fmt;

/// One simulator device as `xcrun simctl list devices --json` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimDevice {
    pub udid: String,
    pub name: String,
    /// Human-readable runtime, e.g. `iOS 18.2` (from `com.apple.CoreSimulator.SimRuntime.iOS-18-2`).
    pub runtime: String,
    pub state: DeviceState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    Booted,
    Shutdown,
    /// Any other state simctl reports (`Booting`, `Shutting Down`, …), kept verbatim.
    Other(String),
}

impl fmt::Display for DeviceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceState::Booted => f.write_str("Booted"),
            DeviceState::Shutdown => f.write_str("Shutdown"),
            DeviceState::Other(state) => f.write_str(state),
        }
    }
}

/// Parse `xcrun simctl list devices --json`.
///
/// Devices whose runtime is unavailable (`isAvailable: false`) are dropped: they cannot boot.
pub fn parse_device_list(json: &str) -> anyhow::Result<Vec<SimDevice>> {
    let _ = json;
    todo!("parse_device_list")
}

#[async_trait::async_trait]
pub trait Simctl: Send + Sync {
    async fn list_devices(&self) -> anyhow::Result<Vec<SimDevice>>;
}

/// The production [`Simctl`]: runs `xcrun simctl`.
#[derive(Debug, Default, Clone, Copy)]
pub struct XcrunSimctl;

#[async_trait::async_trait]
impl Simctl for XcrunSimctl {
    async fn list_devices(&self) -> anyhow::Result<Vec<SimDevice>> {
        todo!("XcrunSimctl::list_devices")
    }
}
