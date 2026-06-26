//! tddy-vnc — VNC-to-LiveKit bridge library.
//!
//! Provides:
//! - `vault` — encrypted credential storage for VNC targets in the session dir.
//! - `vnc_client` — RFB VNC client (connect, framebuffer capture, input forwarding).
//! - `streamer` — LiveKit video track publisher (framebuffer → H.264 via I420).
//! - `bridge` — pump loop: VNC framebuffer → LiveKit + `VncInputService` RPC server.
//! - `common` — shared pixel helpers (`char_to_keysym`, `rgba_to_abgr`).

pub mod bridge;
pub mod common;
pub mod streamer;
pub mod vnc_client;
