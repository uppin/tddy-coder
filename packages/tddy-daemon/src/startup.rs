//! Startup validation helpers for tddy-daemon.
//!
//! Centralises the port/bundle-path checks that `main()` performs at startup,
//! so they can be unit-tested without spawning a full process.

use std::path::PathBuf;

use crate::config::DaemonConfig;

/// Validate configuration and return `(port, bundle_path)` for the server.
///
/// * `relay = true`  — `web_bundle_path` is **not** required; returns `(port, None)`.
/// * `relay = false` — `web_bundle_path` **must** be present; returns `(port, Some(path))`,
///   or an error whose message mentions `web_bundle_path` when it is absent.
///
/// Always returns an error when `config.listen.web_port` is absent.
pub fn startup_config_check(
    config: &DaemonConfig,
    relay: bool,
) -> anyhow::Result<(u16, Option<PathBuf>)> {
    let port = config
        .listen
        .web_port
        .ok_or_else(|| anyhow::anyhow!("config.listen.web_port is required"))?;

    if relay {
        Ok((port, None))
    } else {
        let bundle_path = config
            .web_bundle_path
            .clone()
            .ok_or_else(|| anyhow::anyhow!("config.web_bundle_path is required"))?;
        Ok((port, Some(bundle_path)))
    }
}
