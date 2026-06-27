//! tddy-rdp — RDP-to-LiveKit bridge library.
//!
//! Provides `RdpClient`, which implements `tddy_screenshare::ScreenSharingClient`
//! using the IronRDP library. The binary wires it into `run_bridge`.
//!
//! FIXME: The RDP protocol implementation is a skeleton — `connect()` returns an error.
//! Add IronRDP dependencies and implement the connection phase before shipping.

pub mod rdp_client;
