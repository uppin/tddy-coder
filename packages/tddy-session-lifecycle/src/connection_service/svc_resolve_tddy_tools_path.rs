use crate::config::DaemonConfig;

use std::path::PathBuf;

use tddy_rpc::Status;

use super::DaemonSessionHost;

impl DaemonSessionHost {
    /// Resolve the `tddy-tools` binary from this deployment's **toolchain** — the directory its
    /// tddy binaries are installed in ([`tddy_daemon_kernel::toolchain`]).
    ///
    /// This used to derive the path from `allowed_tools[0].path` by swapping the filename, which
    /// made two mistakes at once. It took the toolchain's location from a *UI menu* of
    /// `tddy-coder` builds, and it kept whatever form that entry had — relative in every dev
    /// config (`target/debug/tddy-coder`). The relative result was written verbatim into a
    /// session's `claude-mcp-config.json`, then spawned by an agent whose cwd is its session
    /// context dir, where no `target/` exists: the MCP server never started, and an agent that had
    /// every native tool withdrawn was left with no replacement it could reach. Nothing logged it.
    ///
    /// A path that does not exist is **refused here**, naming it, rather than written into a
    /// config for something downstream to fail on silently.
    /// The agent-facing tool socket, when this daemon is embedded in an application and therefore
    /// serves no HTTP listener for a co-located agent to POST to.
    ///
    /// Decided by whether the socket is *there* rather than by a host-kind flag threaded through
    /// the session layer: an embedded daemon binds it at startup (`tddy_daemon::runtime`), a binary
    /// one never does, and the path is derived from the same data dir on both sides — so the
    /// question "does this host serve that socket" is answered by the host itself, and a stale
    /// flag cannot disagree with reality.
    pub(crate) fn agent_tool_socket_for_embedded_host(&self) -> Option<String> {
        let path = tddy_daemon_kernel::agent_tool_socket_path(&self.tddy_data_dir);
        path.exists().then(|| path.to_string_lossy().into_owned())
    }

    pub(crate) fn resolve_tddy_tools_path(&self) -> Result<PathBuf, Status> {
        resolve_tddy_tools_path(&self.config)
    }
}

mod svc_host_builders;

/// [`DaemonSessionHost::resolve_tddy_tools_path`] over the one field it reads, so a family handler
/// above this crate (`tddy-daemon-rpc`'s catalogue, probing an agent's models) resolves the binary
/// exactly the way session start does, without holding the host.
pub fn resolve_tddy_tools_path(config: &DaemonConfig) -> Result<PathBuf, Status> {
    config.toolchain().binary("tddy-tools").map_err(|e| {
        log::error!("resolve_tddy_tools_path: {e}");
        Status::failed_precondition(e.to_string())
    })
}
