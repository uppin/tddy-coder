//! Shared fixtures for sandboxed claude-cli acceptance tests.

use std::path::{Path, PathBuf};

/// Terminal marker when in-jail HTTP shim reaches the host via SessionChannel egress relay.
pub const EGRESS_PROBE_SESSION_CHANNEL_OK: &str = "EGRESS_PROBE: session_channel=ok";

/// Terminal marker when direct outbound TCP from the jail is denied by Seatbelt.
pub const EGRESS_PROBE_DIRECT_DENIED: &str = "EGRESS_PROBE: direct=denied";

/// Terminal marker when a `CONNECT` tunnel through the in-jail HTTPS_PROXY round-trips to the host.
pub const CONNECT_PROBE_TUNNEL_OK: &str = "CONNECT_PROBE: tunnel=ok";

/// Returns whether `pid` is still running (Unix `kill(pid, 0)`).
#[cfg(unix)]
pub fn process_is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
pub fn process_is_alive(_pid: u32) -> bool {
    false
}

/// Fake claude script that probes direct TCP vs the in-jail egress shim (SessionChannel egress path).
///
/// The direct socket probe dials `host:port`, written into the script itself: the jailed agent is
/// given no `TDDY_*` variable, so the target cannot reach it through the environment. The shim
/// probe uses `HTTPS_PROXY`, the proxy the jail gives the agent (the runner's egress shim, which
/// answers `GET /probe` by forwarding an `EgressRequest` to the host).
/// Emits structured `EGRESS_PROBE:` markers on stdout (PTY-visible).
pub fn write_egress_probe_claude_script(dir: &Path, host: &str, port: u16) -> PathBuf {
    let script = dir.join("egress_probe_claude.sh");
    let body = format!(
        r#"#!/bin/sh
echo "ARGV: $@"
HOST="{host}"
PORT="{port}"

if nc -z -G 2 "$HOST" "$PORT" 2>/dev/null; then
  echo "EGRESS_PROBE: direct=ok"
else
  echo "EGRESS_PROBE: direct=denied"
fi

SHIM="${{HTTPS_PROXY:-}}"
if [ -z "$SHIM" ]; then
  echo "EGRESS_PROBE: session_channel=unset"
elif curl -s -o /dev/null --noproxy '*' --connect-timeout 2 "${{SHIM}}/probe" 2>/dev/null; then
  echo "EGRESS_PROBE: session_channel=ok"
else
  echo "EGRESS_PROBE: session_channel=denied"
fi

exec cat
"#
    );
    std::fs::write(&script, body).expect("write egress probe script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

/// Fake claude script that exercises the in-jail HTTPS_PROXY `CONNECT` tunnel.
///
/// Uses `curl --proxytunnel -x "$HTTPS_PROXY"` (the proxy env the runner sets to the egress shim)
/// to force a `CONNECT` to `TDDY_EGRESS_PROBE_HOST:PORT`. The runner relays the tunnel to the host,
/// which dials the real socket; a successful round-trip prints [`CONNECT_PROBE_TUNNEL_OK`].
pub fn write_connect_proxy_claude_script(dir: &Path) -> PathBuf {
    let script = dir.join("connect_proxy_claude.sh");
    let body = r#"#!/bin/sh
echo "ARGV: $@"
HOST="${TDDY_EGRESS_PROBE_HOST:-127.0.0.1}"
PORT="${TDDY_EGRESS_PROBE_PORT:-9}"
PROXY="${HTTPS_PROXY:-${TDDY_EGRESS_SHIM:-}}"

if [ -z "$PROXY" ]; then
  echo "CONNECT_PROBE: tunnel=unset"
elif curl -s --proxytunnel -x "$PROXY" --connect-timeout 5 "http://${HOST}:${PORT}/llm" 2>/dev/null | grep -q LLM_ECHO; then
  echo "CONNECT_PROBE: tunnel=ok"
else
  echo "CONNECT_PROBE: tunnel=denied"
fi

exec cat
"#;
    std::fs::write(&script, body).expect("write connect proxy script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}
