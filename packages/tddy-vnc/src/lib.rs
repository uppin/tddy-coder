//! tddy-vnc — VNC-to-LiveKit bridge library.
//!
//! Provides:
//! - `vnc_client` — RFB VNC client implementing `tddy_screenshare::ScreenSharingClient`.
//!
//! The generic bridge loop, LiveKit streamer, and pixel helpers have moved to
//! `tddy-screenshare`. The `tddy-vnc` binary wires `VncClientState` into `run_bridge`.

pub mod vnc_client;
