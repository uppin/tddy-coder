//! The booted iOS Simulators on this Mac, offered as desktop channels.
//!
//! Listed through the `tddy-ios-emulator` bridge binary (`--list-devices`) rather than by linking
//! it, so this crate takes no dependency on the bridge's LiveKit stack, and a host with no Xcode,
//! or one that is not a Mac, lists none.

use tddy_service::proto::host::HostRemoteDesktop;

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
pub fn ios_simulator_channels(lister: &dyn SimulatorLister, can_bridge: bool) -> Vec<HostRemoteDesktop> {
    let _ = (lister, can_bridge);
    todo!("ios_simulator_channels")
}
