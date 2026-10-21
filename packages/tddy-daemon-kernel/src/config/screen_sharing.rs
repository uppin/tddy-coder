use crate::config::DaemonConfig;

/// Screen-sharing bridge binary configuration. Loaded from daemon YAML under `screen_sharing:`.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenSharingConfig {
    /// Path to the `tddy-vnc` bridge binary.
    /// Empty string (default) → resolved at runtime: current_exe sibling, then PATH.
    #[serde(default)]
    pub vnc_binary_path: String,
    /// Path to the `tddy-rdp` bridge binary.
    /// Empty string (default) → resolved at runtime: current_exe sibling, then PATH.
    #[serde(default)]
    pub rdp_binary_path: String,
    /// Path to `x11vnc`, which serves an X11 display to the VNC bridge. Not shipped with tddy.
    /// Empty string (default) → `x11vnc` on PATH.
    #[serde(default)]
    pub x11vnc_binary_path: String,
    /// Directory holding the X server sockets (`X<N>`) whose displays are offered as channels.
    /// Empty string (default) → `/tmp/.X11-unix`.
    #[serde(default)]
    pub x11_socket_dir: String,
}

/// Resolve the actual path to the `tddy-vnc` binary.
///
/// Resolution order:
/// 1. Explicit `vnc_binary_path` in `screen_sharing` config (if non-empty).
/// 2. Sibling of the current executable (same directory).
/// 3. Fallback to `"tddy-vnc"` (PATH lookup).
pub fn resolve_vnc_binary_path(config: &DaemonConfig) -> String {
    let explicit = config
        .screen_sharing
        .as_ref()
        .map(|c| c.vnc_binary_path.as_str())
        .unwrap_or("");
    if !explicit.is_empty() {
        return explicit.to_string();
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("tddy-vnc")))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "tddy-vnc".to_string())
}

/// Resolve the actual path to the `tddy-rdp` binary.
///
/// Resolution order:
/// 1. Explicit `rdp_binary_path` in `screen_sharing` config (if non-empty).
/// 2. Sibling of the current executable (same directory).
/// 3. Fallback to `"tddy-rdp"` (PATH lookup).
pub fn resolve_rdp_binary_path(config: &DaemonConfig) -> String {
    let explicit = config
        .screen_sharing
        .as_ref()
        .map(|c| c.rdp_binary_path.as_str())
        .unwrap_or("");
    if !explicit.is_empty() {
        return explicit.to_string();
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("tddy-rdp")))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "tddy-rdp".to_string())
}

/// Resolve the path to `x11vnc`: the explicit `x11vnc_binary_path`, else the bare name for a PATH
/// lookup. No current-exe sibling step — x11vnc is a system package, never shipped beside tddy.
pub fn resolve_x11vnc_binary_path(config: &ScreenSharingConfig) -> std::path::PathBuf {
    if config.x11vnc_binary_path.is_empty() {
        std::path::PathBuf::from("x11vnc")
    } else {
        std::path::PathBuf::from(&config.x11vnc_binary_path)
    }
}
