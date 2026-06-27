//! tddy-rdp — RDP-to-LiveKit bridge library.
//!
//! Provides `RdpClient`, which implements `tddy_screenshare::ScreenSharingClient`
//! using the IronRDP library. The binary wires it into `run_bridge`.

pub mod rdp_client;
