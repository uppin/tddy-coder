use std::path::{Path, PathBuf};

/// Environment variable naming the shared sandbox config file explicitly (highest priority).
pub const SANDBOX_CONFIG_ENV: &str = "TDDY_SANDBOX_CONFIG";

/// Basename (no extension) of the shared claude-sandbox config file. The per-OS variant inserts the
/// OS token: `claude-sandbox.<os>.yaml`; the generic form is `claude-sandbox.yaml`.
pub const SANDBOX_CONFIG_BASENAME: &str = "claude-sandbox";

/// OS token used in per-OS config filenames (e.g. `claude-sandbox.darwin.yaml`).
///
/// Uses the repo's convention — `"darwin"` for macOS (matching the `tddy-sandbox-darwin` crate),
/// `"linux"` for Linux — rather than Rust's `std::env::consts::OS` (`"macos"`). A per-OS file is
/// host-neutral: the same `claude-sandbox.darwin.yaml` works on any macOS host.
pub fn sandbox_config_os_token() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "darwin"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        std::env::consts::OS
    }
}

/// Resolve which shared claude-sandbox config file the daemon should load, searching `base_dir`.
///
/// Order (first existing wins):
/// 1. `TDDY_SANDBOX_CONFIG` env — an explicit path, used as-is when it exists.
/// 2. Per-OS `<base>/claude-sandbox.<os>.yaml` (e.g. `claude-sandbox.darwin.yaml`) — committed per
///    platform, host-neutral within that OS.
/// 3. Generic `<base>/claude-sandbox.yaml`.
///
/// `None` → no file found; the daemon falls back to built-in defaults (zero-config still works).
pub fn resolve_sandbox_config_path(base_dir: &Path) -> Option<PathBuf> {
    resolve_sandbox_config_path_with(
        base_dir,
        sandbox_config_os_token(),
        std::env::var(SANDBOX_CONFIG_ENV).ok().as_deref(),
        |p| p.is_file(),
    )
}

/// Pure core of [`resolve_sandbox_config_path`] — env override, OS token, and an existence probe
/// are all injected so the precedence is unit-testable without touching the real filesystem/env.
pub(super) fn resolve_sandbox_config_path_with(
    base_dir: &Path,
    os_token: &str,
    env_override: Option<&str>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(env) = env_override.map(str::trim).filter(|s| !s.is_empty()) {
        let p = PathBuf::from(env);
        if exists(&p) {
            return Some(p);
        }
    }
    let per_os = base_dir.join(format!("{SANDBOX_CONFIG_BASENAME}.{os_token}.yaml"));
    if exists(&per_os) {
        return Some(per_os);
    }
    let generic = base_dir.join(format!("{SANDBOX_CONFIG_BASENAME}.yaml"));
    if exists(&generic) {
        return Some(generic);
    }
    None
}
