//! The `simctl` port: which simulators exist, and what state each is in.

use std::fmt;

use anyhow::Context as _;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

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
    let list: DeviceList =
        serde_json::from_str(json).context("parsing `xcrun simctl list devices --json`")?;
    Ok(list
        .devices
        .0
        .into_iter()
        .flat_map(|(runtime_id, devices)| {
            let runtime = readable_runtime(&runtime_id);
            devices
                .into_iter()
                .filter(|device| device.is_available)
                .map(move |device| SimDevice {
                    udid: device.udid,
                    name: device.name,
                    runtime: runtime.clone(),
                    state: DeviceState::from_simctl(&device.state),
                })
        })
        .collect())
}

impl DeviceState {
    fn from_simctl(state: &str) -> Self {
        match state {
            "Booted" => DeviceState::Booted,
            "Shutdown" => DeviceState::Shutdown,
            other => DeviceState::Other(other.to_owned()),
        }
    }
}

/// `com.apple.CoreSimulator.SimRuntime.iOS-18-2` → `iOS 18.2`. An identifier of another shape is
/// kept verbatim.
fn readable_runtime(runtime_id: &str) -> String {
    let Some(tail) = runtime_id.strip_prefix("com.apple.CoreSimulator.SimRuntime.") else {
        return runtime_id.to_owned();
    };
    match tail.split_once('-') {
        Some((platform, version)) => format!("{platform} {}", version.replace('-', ".")),
        None => tail.to_owned(),
    }
}

#[derive(Deserialize)]
struct DeviceList {
    devices: RuntimeDevices,
}

#[derive(Deserialize)]
struct ListedDevice {
    udid: String,
    name: String,
    state: String,
    #[serde(rename = "isAvailable")]
    is_available: bool,
}

/// simctl's runtime → devices map, kept in the order simctl lists it.
struct RuntimeDevices(Vec<(String, Vec<ListedDevice>)>);

impl<'de> Deserialize<'de> for RuntimeDevices {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct InOrder;

        impl<'de> Visitor<'de> for InOrder {
            type Value = RuntimeDevices;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map of simulator runtime to its devices")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(RuntimeDevices(entries))
            }
        }

        deserializer.deserialize_map(InOrder)
    }
}

#[async_trait::async_trait]
pub trait Simctl: Send + Sync {
    async fn list_devices(&self) -> anyhow::Result<Vec<SimDevice>>;
}

/// The system's `xcrun`, which dispatches to the Xcode `xcode-select` chose.
const SYSTEM_XCRUN: &str = "/usr/bin/xcrun";

/// The production [`Simctl`]: runs the system's `xcrun simctl`.
///
/// Simulators belong to the Xcode chosen with `xcode-select`, never to a build toolchain. A
/// toolchain such as the nix dev shell puts its own `xcrun` first on `PATH` and points
/// `DEVELOPER_DIR` / `SDKROOT` at an SDK that has no `simctl`, so the system `xcrun` is run by
/// path, with those two variables removed.
#[derive(Debug, Clone)]
pub struct XcrunSimctl {
    xcrun: std::path::PathBuf,
}

impl Default for XcrunSimctl {
    /// The system's `/usr/bin/xcrun`.
    fn default() -> Self {
        Self::at(SYSTEM_XCRUN)
    }
}

impl XcrunSimctl {
    /// Run the `xcrun` at `xcrun` instead of the system's.
    pub fn at(xcrun: impl Into<std::path::PathBuf>) -> Self {
        Self {
            xcrun: xcrun.into(),
        }
    }
}

#[async_trait::async_trait]
impl Simctl for XcrunSimctl {
    async fn list_devices(&self) -> anyhow::Result<Vec<SimDevice>> {
        let output = tokio::process::Command::new(&self.xcrun)
            .env_remove("DEVELOPER_DIR")
            .env_remove("SDKROOT")
            .args(["simctl", "list", "devices", "--json"])
            .output()
            .await
            .context("running `xcrun simctl list devices --json`")?;
        if !output.status.success() {
            anyhow::bail!(
                "`xcrun simctl list devices --json` exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let json = String::from_utf8(output.stdout)
            .context("`xcrun simctl list devices --json` printed non-UTF-8 output")?;
        parse_device_list(&json)
    }
}
