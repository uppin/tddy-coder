//! iOS Simulator bridge for tddy screen sharing.
//!
//! [`IosSimulatorClient`] implements [`tddy_screenshare::ScreenSharingClient`] over three ports:
//! [`Simctl`] (which devices exist and which are booted), [`SimulatorWindows`] (the Simulator.app
//! window showing a device, and its pixels) and [`InputSink`] (synthetic mouse and keyboard events
//! at screen coordinates). [`DeviceViewport`] crops the window to the device screen and maps device
//! pixels back to screen points, so input lands where the viewer clicked.

pub mod client;
pub mod error;
pub mod geometry;
pub mod input;
pub mod simctl;
pub mod window;

pub use client::{IosSimulatorClient, SimulatorPorts};
pub use error::SimulatorError;
pub use geometry::{DeviceViewport, Rect, ScreenPoint};
pub use input::{key_for_keysym, InputEvent, InputSink, KeyStroke, SpecialKey};
pub use simctl::{parse_device_list, DeviceState, SimDevice, Simctl, XcrunSimctl};
pub use window::{Frame, SimulatorWindow, SimulatorWindows};
