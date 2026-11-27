//! `tddy-ios-emulator --list-devices`: the booted simulators a host offers, as one JSON line.

use serde::Serialize;

use crate::simctl::{DeviceState, SimDevice, Simctl};

/// The flag that switches the binary to listing devices.
pub const LIST_DEVICES_FLAG: &str = "--list-devices";

#[derive(Serialize)]
struct ListedDevice<'a> {
    udid: &'a str,
    name: &'a str,
    runtime: &'a str,
}

/// The booted devices among `devices`, as a JSON array of `{udid, name, runtime}`.
pub fn booted_devices_json(devices: &[SimDevice]) -> String {
    let booted: Vec<ListedDevice<'_>> = devices
        .iter()
        .filter(|device| device.state == DeviceState::Booted)
        .map(|device| ListedDevice {
            udid: &device.udid,
            name: &device.name,
            runtime: &device.runtime,
        })
        .collect();
    serde_json::to_string(&booted).expect("a device list always serialises")
}

/// List the devices `simctl` reports and render the booted ones.
pub async fn list_booted_devices(simctl: &dyn Simctl) -> anyhow::Result<String> {
    Ok(booted_devices_json(&simctl.list_devices().await?))
}
