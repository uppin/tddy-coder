//! Relay daemon lifecycle: discovery, health-check, and spawn.
//!
//! `ensure_relay_daemon` checks whether a relay daemon is already running (via a
//! discovery file), and if not attempts to spawn one from the configured binary path.

use std::path::PathBuf;

use anyhow::Result;

/// A running relay daemon endpoint.
#[derive(Debug)]
pub struct RelayEndpoint {
    /// TCP port the relay daemon listens on.
    pub port: u16,
}

impl RelayEndpoint {
    /// Returns the base URL for the relay daemon (e.g. `http://127.0.0.1:9321`).
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

/// Configuration for relay daemon discovery and spawn.
pub struct RelayConfig {
    /// Directory that holds the `daemon.json` discovery file and other relay state.
    pub base_dir: PathBuf,
    /// Auto-shutdown the daemon after this many seconds of inactivity.
    pub idle_timeout_secs: u64,
    /// Path to the `tddy-daemon` binary to spawn when no relay is running.
    pub daemon_binary: PathBuf,
}

/// Serialized shape of `daemon.json`.
#[derive(serde::Deserialize, serde::Serialize)]
struct DiscoveryFile {
    port: u16,
    pid: u32,
    #[serde(default)]
    started_at: u64,
}

/// Check TCP connectivity to `127.0.0.1:{port}`.
///
/// Returns `true` when a connection can be established (i.e. the daemon is up).
fn is_port_reachable(port: u16) -> bool {
    use std::net::TcpStream;
    use std::time::Duration;
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_secs(2),
    )
    .is_ok()
}

/// Ensure a relay daemon is running and return its endpoint.
///
/// Algorithm:
/// 1. Try reading `{base_dir}/daemon.json`. If it exists and the daemon responds to a TCP
///    probe on the listed port → return `Ok(RelayEndpoint { port })`.
/// 2. If no reachable daemon is found, check whether `cfg.daemon_binary` exists.
///    If not → return `Err` (graceful failure, no panic).
/// 3. Spawn the daemon binary with `--relay`, poll until reachable (up to 5 s),
///    write a new `daemon.json`, and return the endpoint.
pub fn ensure_relay_daemon(cfg: &RelayConfig) -> Result<RelayEndpoint> {
    let discovery_path = cfg.base_dir.join("daemon.json");

    // Step 1: try to reuse a running relay from the discovery file.
    if discovery_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&discovery_path) {
            if let Ok(disc) = serde_json::from_str::<DiscoveryFile>(&contents) {
                if is_port_reachable(disc.port) {
                    return Ok(RelayEndpoint { port: disc.port });
                }
            }
        }
    }

    // Step 2: no reachable daemon — check binary exists.
    if !cfg.daemon_binary.exists() {
        anyhow::bail!(
            "relay daemon binary not found: {}",
            cfg.daemon_binary.display()
        );
    }

    // Step 3: spawn the daemon and wait for it to be reachable.
    // We ask the OS for an ephemeral port by binding to port 0, recording the port,
    // then closing the listener before handing the port to the daemon.
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| anyhow::anyhow!("failed to bind ephemeral port: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| anyhow::anyhow!("failed to get local addr: {}", e))?
        .port();
    drop(listener);

    let child = std::process::Command::new(&cfg.daemon_binary)
        .arg("--relay")
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn relay daemon: {}", e))?;

    let pid = child.id();

    // Poll until reachable (up to 5 s at 100 ms intervals).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if is_port_reachable(port) {
            break;
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "relay daemon did not become reachable within 5 seconds on port {}",
                port
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Write discovery file.
    let disc = DiscoveryFile {
        port,
        pid,
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };
    let json = serde_json::to_string(&disc)
        .map_err(|e| anyhow::anyhow!("failed to serialize discovery file: {}", e))?;
    std::fs::write(&discovery_path, json)
        .map_err(|e| anyhow::anyhow!("failed to write discovery file: {}", e))?;

    Ok(RelayEndpoint { port })
}
