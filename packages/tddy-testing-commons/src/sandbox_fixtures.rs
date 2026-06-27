//! Shared fixtures for sandboxed claude-cli acceptance tests.

use std::path::{Path, PathBuf};

/// Terminal marker when in-jail HTTP shim reaches the host via SessionChannel egress relay.
pub const EGRESS_PROBE_SESSION_CHANNEL_OK: &str = "EGRESS_PROBE: session_channel=ok";

/// Terminal marker when direct outbound TCP from the jail is denied by Seatbelt.
pub const EGRESS_PROBE_DIRECT_DENIED: &str = "EGRESS_PROBE: direct=denied";

/// Returns whether `pid` is still running (Unix `kill(pid, 0)`).
#[cfg(unix)]
pub fn process_is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
pub fn process_is_alive(_pid: u32) -> bool {
    false
}

/// Fake claude script that probes direct TCP vs in-jail HTTP shim (SessionChannel egress path).
///
/// Reads `TDDY_EGRESS_PROBE_HOST` / `TDDY_EGRESS_PROBE_PORT` for direct socket probe and
/// `TDDY_EGRESS_SHIM` for the in-jail HTTP shim (sandbox-runner forwards via `EgressRequest`).
/// Emits structured `EGRESS_PROBE:` markers on stdout (PTY-visible).
pub fn write_egress_probe_claude_script(dir: &Path) -> PathBuf {
    let script = dir.join("egress_probe_claude.sh");
    let body = r#"#!/bin/sh
echo "ARGV: $@"
HOST="${TDDY_EGRESS_PROBE_HOST:-127.0.0.1}"
PORT="${TDDY_EGRESS_PROBE_PORT:-9}"

if nc -z -G 2 "$HOST" "$PORT" 2>/dev/null; then
  echo "EGRESS_PROBE: direct=ok"
else
  echo "EGRESS_PROBE: direct=denied"
fi

SHIM="${TDDY_EGRESS_SHIM:-}"
if [ -z "$SHIM" ]; then
  echo "EGRESS_PROBE: session_channel=unset"
elif curl -s -o /dev/null --connect-timeout 2 "${SHIM}/probe" 2>/dev/null; then
  echo "EGRESS_PROBE: session_channel=ok"
else
  echo "EGRESS_PROBE: session_channel=denied"
fi

exec cat
"#;
    std::fs::write(&script, body).expect("write egress probe script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}
