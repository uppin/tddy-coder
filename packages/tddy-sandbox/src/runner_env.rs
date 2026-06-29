//! Generic in-jail runner environment (no product-specific variables).

use std::collections::BTreeMap;
use std::path::Path;

/// Minimal in-jail environment for a directly confined command (no sandbox-runner).
pub fn process_jail_env(scratch_home: &Path, scratch_tmp: &Path) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("HOME".into(), scratch_home.to_string_lossy().to_string());
    env.insert("TMPDIR".into(), scratch_tmp.to_string_lossy().to_string());
    env.insert("TERM".into(), "xterm-256color".into());
    env.insert("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into());
    env
}

/// Runner environment for `tddy-sandbox-runner` inside the jail (session + tool IPC + egress).
pub fn scratch_runner_env(
    scratch_home: &Path,
    scratch_tmp: &Path,
    session_id: &str,
    tool_ipc_socket: &Path,
    egress_dir: &Path,
) -> BTreeMap<String, String> {
    let mut env = process_jail_env(scratch_home, scratch_tmp);
    env.insert("TDDY_SANDBOX_SESSION_ID".into(), session_id.to_string());
    env.insert(
        "TDDY_SANDBOX_TOOL_IPC".into(),
        tool_ipc_socket.to_string_lossy().to_string(),
    );
    env.insert(
        "TDDY_SANDBOX_EGRESS_DIR".into(),
        egress_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "RUST_LOG".into(),
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
    );
    for key in [
        "TDDY_EGRESS_PROBE_HOST",
        "TDDY_EGRESS_PROBE_PORT",
        "TDDY_EGRESS_PROBE_URL",
    ] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                env.insert(key.into(), value);
            }
        }
    }
    if let Ok(probe_target) = std::env::var("TDDY_EGRESS_PROBE_TARGET") {
        if !probe_target.trim().is_empty() {
            env.insert("TDDY_EGRESS_PROBE_TARGET".into(), probe_target);
        }
    }
    env
}
