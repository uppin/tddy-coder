//! The agent-tool socket **per OS user** (`#agent-tools` 26/28): where a session running as user X
//! reaches this daemon's tool roster — its hooks, `tddy-tools notify` and its tool calls.
//!
//! **Where.** `<data dir>/run-agent-tools/<os user>/agent.sock`: a directory of its own, because
//! `tddy-supervisor` hands `run/<os user>/` to the host-session socket alone. Directory `0700`,
//! socket `0600`, both owned by that user, so only that user (and root) can connect.
//!
//! **Under `tddy-supervisor`** the daemon is an unprivileged child and cannot `chown` a socket to
//! somebody else. A user listed under `agent_tool_sockets:` in `supervisor.yaml` has their socket
//! created by the root supervisor and handed over, announced in `LISTEN_FDNAMES` as
//! `agent-tools.<os user>` ([`AGENT_TOOLS_FD_NAME_PREFIX`]); the daemon serves it where it is. A
//! user who is not listed is **refused** — the start fails naming the setting — rather than given a
//! path that user cannot reach, whose every report the hooks' fail-quiet contract would swallow.
//!
//! **An embedded host** (Tddy Desktop) is single-user and keeps its one socket at
//! [`crate::agent_tool_socket_path`], served by the runtime ([`AgentToolSocketPlacement::Embedded`]).
//!
//! **What is served.** The daemon's finished RPC roster, installed once `runtime::build` has
//! assembled it ([`AgentToolSockets::install_roster`]) — the very handlers the UI reaches.
//!
//! Lives here, not in `tddy-daemon`, because its consumers (`tddy-agent-launch`,
//! `tddy-session-split`) are below the daemon and cannot name it.

use std::os::fd::RawFd;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

/// Prefix of the `LISTEN_FDNAMES` entry of a per-OS-user agent-tool socket the supervisor hands
/// over: `agent-tools.<os user>`. The supervisor's contract, spelled again here because the daemon
/// does not link the supervisor; a test pins the two together.
pub const AGENT_TOOLS_FD_NAME_PREFIX: &str = "agent-tools.";

/// Directory under the data dir holding one subdirectory per OS user.
pub const AGENT_TOOL_SOCKETS_RUN_DIR: &str = "run-agent-tools";

/// File name of a user's agent-tool socket inside their directory.
pub const AGENT_TOOL_SOCKET_FILE: &str = "agent.sock";

/// `<data dir>/run-agent-tools/<os user>/agent.sock`, or why there can be none: a user name that is
/// not one plain path segment, or a path too long for `AF_UNIX`.
pub fn agent_tool_socket_path_for_user(data_dir: &Path, os_user: &str) -> anyhow::Result<PathBuf> {
    // TODO(agent-tool-socket-per-user): implement — refuse an unsafe segment and an over-long path.
    Ok(data_dir
        .join(AGENT_TOOL_SOCKETS_RUN_DIR)
        .join(os_user)
        .join(AGENT_TOOL_SOCKET_FILE))
}

/// `(os user, descriptor)` for each agent-tool socket the `LISTEN_*` variables announce — nothing
/// unless they were set for this process (`LISTEN_PID`) and the names account for exactly
/// `LISTEN_FDS` descriptors. The same reading `tddy_supervisor`'s host-session resolver makes.
pub fn agent_tool_fds(
    my_pid: u32,
    listen_pid: Option<&str>,
    listen_fds: Option<&str>,
    listen_fdnames: Option<&str>,
) -> Vec<(String, RawFd)> {
    // TODO(agent-tool-socket-per-user): implement
    let _ = (my_pid, listen_pid, listen_fds, listen_fdnames);
    Vec::new()
}

/// A listener the supervisor created for one OS user, vetted (owner and mode) and ready to serve.
#[derive(Debug)]
pub struct InheritedAgentToolSocket {
    pub os_user: String,
    pub listener: UnixListener,
    pub path: PathBuf,
}

/// Which agent-tool socket a session is told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentToolSocketPlacement {
    /// One socket for the whole daemon, at [`crate::agent_tool_socket_path`] — an embedded host,
    /// which serves no HTTP and runs every session as its own user.
    Embedded,
    /// One socket per OS user, bound here or inherited from `tddy-supervisor` — a server daemon.
    PerOsUser,
}

/// This daemon's agent-tool sockets: the per-OS-user ones it serves (bound lazily, or adopted from
/// the supervisor), or the one an embedded host serves.
pub struct AgentToolSockets {
    placement: AgentToolSocketPlacement,
    // TODO(agent-tool-socket-per-user): implement — the roster, the servers by user and the
    // inherited listeners, as `HostSessionSockets` holds its own.
}

impl AgentToolSockets {
    /// Sockets placed as `placement`, serving no roster until [`Self::install_roster`].
    pub fn new(placement: AgentToolSocketPlacement) -> Self {
        Self { placement }
    }

    /// How this daemon's sessions are told where to report.
    pub fn placement(&self) -> AgentToolSocketPlacement {
        self.placement
    }

    /// Serve `entries` — the daemon's finished roster — on every socket, from now on. Called once by
    /// `runtime::build`, after the roster exists.
    pub fn install_roster(&self, entries: Vec<tddy_rpc::ServiceEntry>) {
        // TODO(agent-tool-socket-per-user): implement
        let _ = entries;
    }

    /// Serve these supervisor-created sockets for their users from now on (served on first use);
    /// a user with one is never bound by this daemon itself.
    pub fn adopt_inherited(&self, sockets: Vec<InheritedAgentToolSocket>) {
        // TODO(agent-tool-socket-per-user): implement
        let _ = sockets;
    }

    /// The path of `os_user`'s agent-tool socket, binding and serving it first if this daemon is not
    /// already. Refuses a user it cannot give an owner-only socket to — under the supervisor, a user
    /// not listed under `agent_tool_sockets:` — naming the setting to add.
    pub async fn ensure_bound(&self, os_user: &str, data_dir: &Path) -> anyhow::Result<PathBuf> {
        // TODO(agent-tool-socket-per-user): implement — today's answer is the embedded probe.
        let _ = os_user;
        crate::embedded_host_agent_tool_socket(data_dir)
            .map(PathBuf::from)
            .ok_or_else(|| {
                anyhow::anyhow!("no agent-tool socket is served for {}", data_dir.display())
            })
    }
}

#[cfg(test)]
#[path = "agent_tool_sockets_tests.rs"]
mod tests;
