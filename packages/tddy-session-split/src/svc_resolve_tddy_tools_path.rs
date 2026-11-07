use tddy_daemon_kernel::config::DaemonConfig;

use std::path::PathBuf;

use tddy_rpc::Status;

use crate::split_ports::SplitSessions;

impl SplitSessions {
    /// `os_user`'s agent-tool socket, served by this daemon (bound first when it is not yet): where
    /// a co-located agent of that user reports its hooks and makes its tool calls. A user this
    /// daemon cannot give one — under the supervisor, one not listed under `agent_tool_sockets:` —
    /// fails the start `FAILED_PRECONDITION`, naming the setting.
    pub(crate) async fn agent_tool_socket_of(&self, os_user: &str) -> Result<String, Status> {
        self.agent_tool_sockets
            .ensure_bound(os_user, &self.tddy_data_dir)
            .await
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|e| Status::failed_precondition(format!("{e:#}")))
    }

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
    pub(crate) fn resolve_tddy_tools_path(&self) -> Result<PathBuf, Status> {
        resolve_tddy_tools_path(&self.config)
    }
}

/// [`DaemonSessionHost::resolve_tddy_tools_path`] over the one field it reads, so a family handler
/// above this crate (`tddy-daemon-rpc`'s catalogue, probing an agent's models) resolves the binary
/// exactly the way session start does, without holding the host.
pub fn resolve_tddy_tools_path(config: &DaemonConfig) -> Result<PathBuf, Status> {
    config.toolchain().binary("tddy-tools").map_err(|e| {
        log::error!("resolve_tddy_tools_path: {e}");
        Status::failed_precondition(e.to_string())
    })
}
