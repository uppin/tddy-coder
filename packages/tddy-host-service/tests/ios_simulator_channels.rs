//! A Mac's booted iOS Simulators, as the desktop channels its Hosts row offers.
//!
//! PRD: `docs/ft/web/1-WIP/PRD-2026-11-26-ios-simulator-screen-sharing.md` AC-IOS-1.

use pretty_assertions::assert_eq;
use tddy_host_service::ios_simulators::{ios_simulator_channels, BootedSimulator, SimulatorLister};
use tddy_service::proto::host::{DesktopChannelKind, HostRemoteDesktop, ProbeOutcome};

const PROTOCOL_IOS_SIMULATOR: i32 = 3;
const BRIDGE_PRESENT: bool = true;

struct AMacListing(Vec<BootedSimulator>);

impl SimulatorLister for AMacListing {
    fn booted(&self) -> Result<Vec<BootedSimulator>, String> {
        Ok(self.0.clone())
    }
}

struct AHostThatCannotList(&'static str);

impl SimulatorLister for AHostThatCannotList {
    fn booted(&self) -> Result<Vec<BootedSimulator>, String> {
        Err(self.0.to_string())
    }
}

fn a_booted(udid: &str, name: &str, runtime: &str) -> BootedSimulator {
    BootedSimulator {
        udid: udid.into(),
        name: name.into(),
        runtime: runtime.into(),
    }
}

fn the_channel_for(udid: &str, label: &str, can_bridge: bool) -> HostRemoteDesktop {
    HostRemoteDesktop {
        outcome: ProbeOutcome::Ok as i32,
        protocol: PROTOCOL_IOS_SIMULATOR,
        can_bridge,
        desktop_reachable: true,
        port: 0,
        failure_reason: String::new(),
        channel_id: format!("ios:{udid}"),
        kind: DesktopChannelKind::IosSimulator as i32,
        display: 0,
        label: label.into(),
        device_id: udid.into(),
    }
}

#[test]
fn each_booted_simulator_is_offered_as_a_channel_named_by_device_and_runtime() {
    // Given
    let mac = AMacListing(vec![
        a_booted("UDID-IPHONE", "iPhone 16 Pro", "iOS 18.2"),
        a_booted("UDID-IPAD", "iPad Air 11-inch (M2)", "iOS 18.2"),
    ]);

    // When
    let channels = ios_simulator_channels(&mac, BRIDGE_PRESENT);

    // Then
    assert_eq!(
        channels,
        vec![
            the_channel_for("UDID-IPHONE", "iPhone 16 Pro · iOS 18.2", true),
            the_channel_for("UDID-IPAD", "iPad Air 11-inch (M2) · iOS 18.2", true),
        ]
    );
}

#[test]
fn a_simulator_is_offered_but_marked_unbridgeable_when_the_bridge_binary_is_missing() {
    // Given
    let mac = AMacListing(vec![a_booted("UDID-IPHONE", "iPhone 16 Pro", "iOS 18.2")]);

    // When
    let channels = ios_simulator_channels(&mac, false);

    // Then
    assert_eq!(
        channels,
        vec![the_channel_for(
            "UDID-IPHONE",
            "iPhone 16 Pro · iOS 18.2",
            false
        )]
    );
}

#[test]
fn a_host_that_cannot_list_simulators_offers_none() {
    // Given
    let linux = AHostThatCannotList("xcrun: not found");

    // When
    let channels = ios_simulator_channels(&linux, BRIDGE_PRESENT);

    // Then
    assert_eq!(channels, vec![]);
}
