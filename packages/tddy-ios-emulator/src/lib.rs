//! iOS Simulator bridge for tddy screen sharing.
//!
//! [`IosSimulatorClient`] implements [`tddy_screenshare::ScreenSharingClient`] over three ports:
//! [`Simctl`] (which devices exist and which are booted), [`SimulatorWindows`] (the Simulator.app
//! window showing a device, and its pixels) and [`InputSink`] (touch and keyboard events, sent to the
//! device's `dtuhidd` in production — [`dtuhid`]). [`DeviceViewport`] crops the window to the device
//! screen and maps device pixels to points on it, which the input adapter sends as positions on the
//! device screen.

pub mod client;
pub mod dtuhid;
#[cfg(target_os = "macos")]
pub mod dtuhid_xpc;
pub mod error;
pub mod geometry;
pub mod input;
pub mod listing;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod simctl;
pub mod window;

pub use client::{swipe_path, IosSimulatorClient, SimulatorPorts};
#[cfg(target_os = "macos")]
pub use dtuhid_xpc::DtuhidInput;
pub use error::SimulatorError;
pub use geometry::{DeviceViewport, Rect, ScreenPoint};
pub use input::{key_for_keysym, InputEvent, InputSink, KeyStroke, SpecialKey};
pub use listing::{booted_devices_json, list_booted_devices, LIST_DEVICES_FLAG};
#[cfg(target_os = "macos")]
pub use macos::XcapSimulatorWindows;
pub use simctl::{parse_device_list, DeviceState, SimDevice, Simctl, XcrunSimctl};
pub use window::{Frame, SimulatorWindow, SimulatorWindows};
