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

use std::collections::HashMap;
use std::os::fd::RawFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context};
use tddy_rpc::{MultiRpcService, ServiceEntry};
use tokio::task::JoinHandle;

use crate::owner_socket::{self, OwnerSocketKind};

/// Prefix of the `LISTEN_FDNAMES` entry of a per-OS-user agent-tool socket the supervisor hands
/// over: `agent-tools.<os user>`. The supervisor's contract, spelled again here because the daemon
/// does not link the supervisor; a test pins the two together.
pub const AGENT_TOOLS_FD_NAME_PREFIX: &str = "agent-tools.";

/// Directory under the data dir holding one subdirectory per OS user.
pub const AGENT_TOOL_SOCKETS_RUN_DIR: &str = "run-agent-tools";

/// File name of a user's agent-tool socket inside their directory.
pub const AGENT_TOOL_SOCKET_FILE: &str = "agent.sock";

/// Longest socket path accepted: `sun_path` is 104 bytes on macOS (the smaller of the two; Linux's is
/// 108), NUL included. A macOS temporary directory alone spends ~65 of them, so this takes every
/// byte the platform allows rather than keeping a margin.
const MAX_SOCKET_PATH_BYTES: usize = 103;

/// `<data dir>/run-agent-tools/<os user>/agent.sock`, or why there can be none: a user name that is
/// not one plain path segment, or a path too long for `AF_UNIX`.
pub fn agent_tool_socket_path_for_user(data_dir: &Path, os_user: &str) -> anyhow::Result<PathBuf> {
    let name_is_one_safe_segment = !os_user.is_empty()
        && !os_user.starts_with('.')
        && os_user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !name_is_one_safe_segment {
        bail!("OS user name {os_user:?} cannot name a socket directory");
    }
    let path = data_dir
        .join(AGENT_TOOL_SOCKETS_RUN_DIR)
        .join(os_user)
        .join(AGENT_TOOL_SOCKET_FILE);
    if path.as_os_str().len() > MAX_SOCKET_PATH_BYTES {
        bail!(
            "agent-tool socket path {} is too long for a unix socket ({} > {MAX_SOCKET_PATH_BYTES} \
             bytes); use a shorter data dir",
            path.display(),
            path.as_os_str().len()
        );
    }
    Ok(path)
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
    owner_socket::descriptors_named(
        AGENT_TOOLS_FD_NAME_PREFIX,
        my_pid,
        listen_pid,
        listen_fds,
        listen_fdnames,
    )
}

/// A listener the supervisor created for one OS user, vetted (owner and mode) and ready to serve.
#[derive(Debug)]
pub struct InheritedAgentToolSocket {
    pub os_user: String,
    pub listener: UnixListener,
    pub path: PathBuf,
}

/// The agent-tool sockets this process was handed, according to its own environment.
///
/// Call it once, before anything else reads or clears the `LISTEN_*` variables — the daemon's own
/// listener adoption removes them.
pub fn adopt_from_environment() -> Vec<InheritedAgentToolSocket> {
    let variable = |name: &str| std::env::var(name).ok();
    adopt_descriptors(agent_tool_fds(
        std::process::id(),
        variable("LISTEN_PID").as_deref(),
        variable("LISTEN_FDS").as_deref(),
        variable("LISTEN_FDNAMES").as_deref(),
    ))
}

/// Adopt each `(os user, descriptor)` that is that user's agent-tool socket; close the rest.
pub fn adopt_descriptors(descriptors: Vec<(String, RawFd)>) -> Vec<InheritedAgentToolSocket> {
    descriptors
        .into_iter()
        .filter_map(|(os_user, fd)| {
            let (listener, path) =
                owner_socket::adopt_descriptor(&os_user, fd, OwnerSocketKind::AGENT_TOOL)?;
            Some(InheritedAgentToolSocket {
                os_user,
                listener,
                path,
            })
        })
        .collect()
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

struct BoundServer {
    path: PathBuf,
    /// `(dev, ino)` of the socket this server bound, to notice the file was replaced or removed.
    identity: (u64, u64),
    /// Whether the file is this daemon's to remove — `false` for a socket the supervisor made.
    bound_here: bool,
    task: JoinHandle<()>,
}

impl Drop for BoundServer {
    /// A server outlives nothing: dropping the daemon's sockets closes the listener, so a daemon that
    /// starts over in the same process (or a test that does) finds no live peer on the path.
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// This daemon's agent-tool sockets: the per-OS-user ones it serves (bound lazily, or adopted from
/// the supervisor), or the one an embedded host serves.
pub struct AgentToolSockets {
    placement: AgentToolSocketPlacement,
    /// The daemon's finished roster; empty until [`Self::install_roster`]. Read at every accept, so
    /// a socket bound before the roster exists still serves it once it does.
    roster: Arc<std::sync::Mutex<Vec<ServiceEntry>>>,
    servers: std::sync::Mutex<HashMap<String, BoundServer>>,
    /// Sockets the supervisor created, by OS user: `Some` until first served, `None` after — the
    /// key stays so a user the supervisor serves is never bound by this daemon itself.
    inherited: std::sync::Mutex<HashMap<String, Option<InheritedAgentToolSocket>>>,
}

impl AgentToolSockets {
    /// Sockets placed as `placement`, serving no roster until [`Self::install_roster`].
    pub fn new(placement: AgentToolSocketPlacement) -> Self {
        Self {
            placement,
            roster: Arc::default(),
            servers: std::sync::Mutex::default(),
            inherited: std::sync::Mutex::default(),
        }
    }

    /// How this daemon's sessions are told where to report.
    pub fn placement(&self) -> AgentToolSocketPlacement {
        self.placement
    }

    /// Serve `entries` — the daemon's finished roster — on every socket, from now on. Called once by
    /// `runtime::build`, after the roster exists.
    pub fn install_roster(&self, entries: Vec<ServiceEntry>) {
        *self.roster.lock().expect("agent-tool roster lock") = entries;
    }

    /// Serve these supervisor-created sockets for their users from now on (served on first use);
    /// a user with one is never bound by this daemon itself.
    pub fn adopt_inherited(&self, sockets: Vec<InheritedAgentToolSocket>) {
        let mut inherited = self.inherited.lock().expect("inherited sockets lock");
        for socket in sockets {
            inherited.insert(socket.os_user.clone(), Some(socket));
        }
    }

    /// Stop serving: close every socket this daemon bound and remove its file, as a daemon that
    /// exits leaves none. A socket the supervisor created is not this daemon's to remove — the
    /// supervisor keeps it, and its listener here is closed with the daemon; the daemon the
    /// supervisor starts next is handed it again. A later [`Self::ensure_bound`] binds afresh a
    /// socket this daemon bound.
    pub fn stop_serving(&self) {
        let servers: Vec<BoundServer> = self
            .servers
            .lock()
            .expect("agent-tool servers lock")
            .drain()
            .map(|(_, server)| server)
            .collect();
        for server in servers {
            server.task.abort();
            let still_its_file = std::fs::symlink_metadata(&server.path)
                .is_ok_and(|m| (m.dev(), m.ino()) == server.identity);
            if still_its_file && server.bound_here {
                let _ = std::fs::remove_file(&server.path);
            }
        }
    }

    /// The path of `os_user`'s agent-tool socket, binding and serving it first if this daemon is not
    /// already. Refuses a user it cannot give an owner-only socket to — under the supervisor, a user
    /// not listed under `agent_tool_sockets:` — naming the setting to add.
    pub async fn ensure_bound(&self, os_user: &str, data_dir: &Path) -> anyhow::Result<PathBuf> {
        self.ensure_bound_blocking(os_user, data_dir)
    }

    /// [`Self::ensure_bound`] for a call site that is not `async` — a tool-server resolver, a
    /// synchronous plan. There is nothing to wait for: binding is a few filesystem calls, which is
    /// all `ensure_bound` does too. Must run inside a tokio runtime, which serves the socket.
    ///
    /// Safe on a runtime worker. The locks are `std` ones held for those calls alone — never across
    /// an `.await`, and never while waiting on another task — so a worker can wait here only for a
    /// bounded run of syscalls (`getpwnam`, `mkdir`, `bind`, `chown`, a `connect` to a socket file
    /// with no listener, which is refused at once). Only the first session of a user pays that; later
    /// ones `stat` the bound file. `HostSessionSockets` does the same work on a worker.
    pub fn ensure_bound_blocking(&self, os_user: &str, data_dir: &Path) -> anyhow::Result<PathBuf> {
        if self.placement == AgentToolSocketPlacement::Embedded {
            return embedded_socket(data_dir);
        }
        let mut servers = self.servers.lock().expect("agent-tool servers lock");
        if let Some(server) = servers.get(os_user) {
            let intact = !server.task.is_finished()
                && std::fs::symlink_metadata(&server.path)
                    .map(|m| (m.dev(), m.ino()) == server.identity)
                    .unwrap_or(false);
            if intact {
                return Ok(server.path.clone());
            }
        }
        // A server whose file is gone or replaced listens on nothing anyone can reach — its socket
        // is no longer at the path — so dropping it (which stops it) cannot be mistaken for a live
        // peer by the probe below, which looks at the path alone.
        servers.remove(os_user);
        let inherited = {
            let mut inherited = self.inherited.lock().expect("inherited sockets lock");
            inherited.get_mut(os_user).map(Option::take)
        };
        let (path, listener, bound_here) = match inherited {
            Some(Some(socket)) => {
                socket
                    .listener
                    .set_nonblocking(true)
                    .context("make the inherited socket non-blocking")?;
                (socket.path, socket.listener, false)
            }
            // The supervisor made this socket and only it can make another.
            Some(None) => bail!(
                "the agent-tool socket tddy-supervisor created for {os_user} is gone or no longer \
                 accepting; it can only be recreated by restarting tddy-supervisor"
            ),
            None => {
                let path = agent_tool_socket_path_for_user(data_dir, os_user)?;
                let listener =
                    owner_socket::bind_owner_only(&path, os_user, OwnerSocketKind::AGENT_TOOL)?;
                (path, listener, true)
            }
        };
        let meta = std::fs::symlink_metadata(&path).context("stat the bound socket")?;
        let listener = tokio::net::UnixListener::from_std(listener)
            .context("hand the socket to the runtime")?;
        let task = tokio::spawn(serve(
            listener,
            os_user.to_string(),
            Arc::clone(&self.roster),
        ));
        servers.insert(
            os_user.to_string(),
            BoundServer {
                path: path.clone(),
                identity: (meta.dev(), meta.ino()),
                bound_here,
                task,
            },
        );
        log::info!(
            target: "tddy_daemon_kernel::agent_tool_sockets",
            "agent-tool socket for {os_user} listening at {}",
            path.display()
        );
        Ok(path)
    }
}

/// The one socket an embedded daemon's runtime serves. The runtime binds it at startup, so a
/// session started before that has nothing to be told and is refused rather than pointed at a path
/// nobody serves — whose every report the hooks' fail-quiet contract would swallow.
fn embedded_socket(data_dir: &Path) -> anyhow::Result<PathBuf> {
    let path = crate::agent_tool_socket_path(data_dir);
    match std::os::unix::net::UnixStream::connect(&path) {
        Ok(_) => Ok(path),
        Err(e) => bail!(
            "the embedded host's agent-tool socket {} is not being served: {e}",
            path.display()
        ),
    }
}

/// Accept every connection for the daemon's lifetime; each gets its own endpoint over the roster as
/// it stands then. One connection per caller: a dispatch sharing a connection with a long-lived
/// server stream would interleave with frames it does not read.
async fn serve(
    listener: tokio::net::UnixListener,
    owner: String,
    roster: Arc<std::sync::Mutex<Vec<ServiceEntry>>>,
) {
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon_kernel::agent_tool_sockets",
                    "agent-tool socket for {owner}: accept failed and it is no longer served; one \
                     this daemon bound is bound again by the next session that needs it, one \
                     tddy-supervisor made only once tddy-supervisor restarts: {e}"
                );
                return;
            }
        };
        let entries: Vec<ServiceEntry> = roster
            .lock()
            .expect("agent-tool roster lock")
            .iter()
            .map(|e| ServiceEntry {
                name: e.name,
                service: Arc::clone(&e.service),
            })
            .collect();
        let (reader, writer) = tokio::io::split(stream);
        let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
            reader,
            writer,
            MultiRpcService::new(entries),
            tddy_rpc::RequestTransport::UnixSocket,
        );
        tokio::spawn(endpoint.run());
    }
}

#[cfg(test)]
#[path = "agent_tool_sockets_tests.rs"]
mod tests;
