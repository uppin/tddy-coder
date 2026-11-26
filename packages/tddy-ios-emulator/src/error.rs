//! Why a simulator could not be bridged. Every variant names the device by udid, so a failed start
//! tells the operator which device it was and what state it was in.

use crate::simctl::DeviceState;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SimulatorError {
    /// The bridge config named no device: an iOS Simulator target always carries a udid.
    #[error("an iOS Simulator target must name a device udid")]
    NoDevice,
    /// `simctl` lists no device with this udid.
    #[error("no iOS Simulator with udid {udid}")]
    DeviceNotFound { udid: String },
    /// The device exists but is not booted, so there is no screen to stream.
    #[error("iOS Simulator {udid} is {state}, not booted")]
    DeviceNotBooted { udid: String, state: DeviceState },
    /// The device is booted but Simulator.app shows no window for it.
    #[error("Simulator.app shows no window for {name} ({udid})")]
    WindowNotFound { udid: String, name: String },
    /// This build cannot bridge a simulator (not macOS).
    #[error("iOS Simulator {udid} can only be bridged on macOS")]
    Unsupported { udid: String },
}
