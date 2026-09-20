//! The agent-facing tool socket: how a co-located agent reaches an **embedded** daemon.
//!
//! [`crate::runtime::RuntimeHost::Embedded`] — Tddy Desktop — serves no HTTP listener and no
//! tonic local socket; its UI reaches the daemon over the application's own transport. That leaves
//! an agent spawned *beside* a jailed checkout (the `sandboxed_codebase` placement) with no way to
//! call back: it is a separate OS process, so it cannot use the application's in-process bridge,
//! and `listen.web_port` on such a host is only where a GitHub sign-in returns — a port that
//! answers every other path with an empty 404. Every tool call came back `relay parse error`.
//!
//! This is the missing channel, and it is deliberately the *same wire* the sandbox tool socket
//! already uses: a Unix stream carrying `tddy-rpc`'s length-prefixed framing, which
//! `tddy_session_tool_client::connect_sandbox_ipc` speaks. The only difference is who is on the
//! other end — the daemon's own service roster rather than a jail's relay — and that the caller
//! carries a `SessionToolEnvelope`, because an agent outside the jail is not identified by the
//! connection the way an in-jail one is.
//!
//! One connection per caller, like the sandbox socket, for the same reason: a dispatch sharing a
//! connection with a long-lived server stream would interleave with frames it does not read.

use std::future::Future;
use std::path::Path;

use tddy_rpc::{MultiRpcService, ServiceEntry};

/// Clone the service roster for one more connection — `ServiceEntry` holds `Arc`s, so this shares
/// the services rather than rebuilding them.
fn cloned_entries(entries: &[ServiceEntry]) -> Vec<ServiceEntry> {
    entries
        .iter()
        .map(|e| ServiceEntry {
            name: e.name,
            service: std::sync::Arc::clone(&e.service),
        })
        .collect()
}

/// Bind `socket_path` and serve the daemon's RPC roster to co-located agents until `shutdown`.
pub async fn serve_agent_tool_socket(
    socket_path: &Path,
    entries: Vec<ServiceEntry>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    // A socket file outlives the process that bound it, so a daemon that did not shut down cleanly
    // leaves one behind and `bind` fails with EADDRINUSE. Removing it is safe here because the path
    // is this daemon's alone (digested from its data dir).
    let _ = std::fs::remove_file(socket_path);
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let listener = tokio::net::UnixListener::bind(socket_path)
        .map_err(|e| anyhow::anyhow!("bind agent tool socket {}: {e}", socket_path.display()))?;
    log::info!(
        target: "tddy_daemon::agent_tool_socket",
        "serving co-located agents on {}",
        socket_path.display()
    );

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { continue };
                let entries = cloned_entries(&entries);
                tokio::spawn(async move {
                    let (read_half, write_half) = tokio::io::split(stream);
                    let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                        read_half,
                        write_half,
                        MultiRpcService::new(entries),
                    );
                    endpoint.run().await;
                });
            }
        }
    }
    let _ = std::fs::remove_file(socket_path);
    Ok(())
}

pub use tddy_daemon_kernel::agent_tool_socket_path;
