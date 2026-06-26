//! VNC↔LiveKit bridge pump loop.
//!
//! Connects to a VNC server, publishes its framebuffer as a LiveKit video track,
//! and serves `VncInputService` over the LiveKit data channel so the browser can
//! forward mouse/keyboard events back to the VNC server.
//!
//! # STUB
//! `run` always errors immediately. Full implementation comes in the green phase.

use serde::Deserialize;

/// Configuration passed to the bridge via stdin as JSON.
#[derive(Debug, Deserialize)]
pub struct BridgeConfig {
    /// VNC server host.
    pub vnc_host: String,
    /// VNC server port (default 5900).
    pub vnc_port: u16,
    /// Decrypted VNC password (empty string for password-less targets).
    pub vnc_password: String,
    /// LiveKit server WebSocket URL.
    pub livekit_url: String,
    /// Pre-minted JWT token for the bridge participant.
    pub livekit_token: String,
    /// LiveKit room name.
    pub livekit_room: String,
    /// LiveKit participant identity for this bridge (e.g. `vnc-<session>-<target>`).
    pub livekit_identity: String,
    /// Video track name (e.g. `vnc:<target_id>`).
    pub track_name: String,
    /// Target framebuffer width.
    pub width: u32,
    /// Target framebuffer height.
    pub height: u32,
    /// Target ID (used for log context).
    pub target_id: String,
    /// Frames per second for the VNC pump loop.
    #[serde(default = "default_fps")]
    pub fps: u32,
}

fn default_fps() -> u32 {
    30
}

/// Run the bridge until the VNC connection closes, an error occurs, or the process
/// receives SIGTERM.
///
/// Reads config from stdin as JSON. The password is passed via the config struct
/// (not argv) to avoid appearing in the process table.
///
/// # Errors
/// **STUB — always errors immediately.** Full implementation comes in the green phase.
pub async fn run(_config: BridgeConfig) -> anyhow::Result<()> {
    anyhow::bail!("bridge::run: not implemented")
}
