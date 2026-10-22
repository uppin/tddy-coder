use std::collections::BTreeMap;
use std::path::PathBuf;

fn default_relay_idle_timeout_secs() -> u64 {
    1800
}

/// Configuration for relay daemon mode (`--relay` / `relay:` YAML section).
///
/// In relay mode the daemon forwards RPCs to a remote peer via LiveKit, does not require a
/// `web_bundle_path`, and shuts down automatically after `idle_timeout_secs` of inactivity.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelayConfig {
    #[serde(default = "default_relay_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            idle_timeout_secs: default_relay_idle_timeout_secs(),
        }
    }
}

/// Local Unix-domain socket transport (`local:` YAML section). The daemon serves its
/// `ConnectionService` over this socket with SO_PEERCRED peer-trust auth, so same-host clients
/// (e.g. tddy-sandbox-app) can mint a session token without an OAuth round-trip.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalConfig {
    /// Path the daemon binds the local socket at. When unset, resolved at bind time to
    /// `${XDG_RUNTIME_DIR:-/run}/tddy-daemon.sock` (see [`DaemonConfig::local_socket_path`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<PathBuf>,
}

/// Which peers this daemon will propagate credentials to.
///
/// The section is optional, and its absence means **this daemon syncs credentials with nobody** —
/// not "with everyone in the room". Room membership carries no attestation that a participant runs
/// `tddy-daemon` (`docs/ft/daemon/livekit-peer-discovery.md` § *Trust model*), and handing a
/// person's GitHub credential to anything holding the room's LiveKit credentials is the property
/// `#keyring` exists to remove. A desktop install with no fleet is the common case for leaving it
/// unset, and it must work.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeyringConfig {
    /// The shared secret that authorises a peer to hold this deployment's credentials.
    ///
    /// **One of two required checks, never sufficient alone.** It answers *may that daemon hold my
    /// credentials* — a per-deployment authorisation decision with no cryptographic answer. The
    /// other check is `#keyring` 1/9's Ed25519 signature, which answers *did that daemon send
    /// this*; a peer passing only one of the two receives nothing and is journaled as refused.
    ///
    /// The secret itself never travels: a peer advertises an HMAC of a fresh challenge under it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_secret: Option<String>,
}

/// Git behavior for daemon-side operations that contact a remote (fetching integration bases).
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct GitConfig {
    /// Overrides `GIT_SSH_COMMAND` for the daemon's `git fetch` calls only — it is not exported to
    /// the process environment or to spawned children. Point git at an ssh binary that can
    /// authenticate non-interactively; e.g. on macOS the system ssh has Keychain support that the
    /// Nix-provided ssh lacks: `/usr/bin/ssh -o BatchMode=yes -o ConnectTimeout=10`. When unset, git
    /// inherits the ambient environment. Regardless of this value, daemon remote fetches run with
    /// stdin closed and `GIT_TERMINAL_PROMPT=0`, so a missing key/passphrase fails fast instead of
    /// hanging the daemon on a prompt it can never answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_command: Option<String>,
}

/// Linux rootless cgroups sandbox delegation (`sandbox_cgroup:` YAML section). All fields optional
/// so the backend derives defaults at runtime (delegated base from `/proc/self/cgroup`, controllers
/// `memory cpu pids`, supervisor leaf `supervisor`) — nothing is hardcoded in the crate. Consulted
/// only by the Linux cgroups backend; ignored on macOS / QEMU.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxCgroupConfig {
    /// Explicit delegated cgroup base directory; skips `/proc/self/cgroup` derivation when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_path: Option<PathBuf>,
    /// cgroup v2 unified mount root (default `/sys/fs/cgroup`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_root: Option<PathBuf>,
    /// Controllers enabled in the base's `cgroup.subtree_control`; empty means `[memory, cpu, pids]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controllers: Vec<String>,
    /// Leaf cgroup the daemon relocates its own process into (default `supervisor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supervisor_leaf: Option<String>,
    /// Default `memory.max` (bytes) applied when a plan carries no explicit limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_max: Option<u64>,
    /// Default `cpu.max` applied when a plan carries no explicit limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_max: Option<String>,
    /// Default `pids.max` applied when a plan carries no explicit limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pids_max: Option<u64>,
}

/// The warm code-intelligence index this daemon manages (`index_daemon:` YAML section).
///
/// The section is optional and only overrides the defaults: every daemon manages an index daemon
/// lazily, started by the first request that needs it. A `tddy-index-daemon` that cannot be started is an error on the request that needed it — not a
/// quiet fall back to a cold rust-analyzer, which would turn a misconfiguration into six minutes
/// of unexplained latency per request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct IndexDaemonConfig {
    /// The binary to run. When unset it is resolved at spawn time, from the daemon's own
    /// executable's directory or from `PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<PathBuf>,
    /// The AF_UNIX socket the index daemon is told to bind and this daemon dials. When unset,
    /// resolved to `${XDG_RUNTIME_DIR:-/run}/tddy-index-daemon.sock` — see
    /// [`IndexDaemonConfig::resolved_socket_path`], which mirrors [`DaemonConfig::local_socket_path`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<PathBuf>,
    /// How long a freshly started index daemon has to bind that socket. A budget on *binding*
    /// only: indexing is not waited for here, and a process that dies in the meantime is reported
    /// at once rather than at this deadline.
    #[serde(default = "default_index_daemon_ready_timeout_secs")]
    pub ready_timeout_secs: u64,
    /// How long the index daemon may sit unused before the reaper stops it. Generous by default:
    /// the warm index it holds costs minutes to rebuild.
    #[serde(default = "default_index_daemon_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    /// The rust-analyzer the index daemon runs, passed to it as `--rust-analyzer`. When unset the
    /// index daemon runs `rust-analyzer` from its own `PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rust_analyzer_path: Option<PathBuf>,
    /// Environment variables set on the index daemon, and so inherited by the rust-analyzer it
    /// starts and the `cargo` / `rustc` that one runs to load a workspace — the toolchain a host
    /// launched outside a developer shell does not otherwise have. Added to the daemon's own
    /// environment, overriding a variable of the same name.
    // TODO: one environment for every workspace root; a per-project toolchain (a checkout's own
    // nix/direnv shell) is not expressible here yet.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// 60s. Binding a socket is immediate; this covers a cold start of the process around it.
fn default_index_daemon_ready_timeout_secs() -> u64 {
    60
}

/// 30 minutes. The index behind this process costs six to ten minutes to rebuild on this
/// workspace, so reaping it eagerly would cost more than holding it.
fn default_index_daemon_idle_timeout_secs() -> u64 {
    30 * 60
}

impl Default for IndexDaemonConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            socket_path: None,
            ready_timeout_secs: default_index_daemon_ready_timeout_secs(),
            idle_timeout_secs: default_index_daemon_idle_timeout_secs(),
            rust_analyzer_path: None,
            env: BTreeMap::new(),
        }
    }
}

impl IndexDaemonConfig {
    /// The socket path this daemon tells the index daemon to bind, resolving the default when the
    /// section names none.
    pub fn resolved_socket_path(&self) -> PathBuf {
        if let Some(path) = &self.socket_path {
            return path.clone();
        }
        // `/run` exists only on Linux hosts; elsewhere (Tddy Desktop on macOS) the temp dir is the
        // one place an unprivileged process can bind.
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let run = PathBuf::from("/run");
                if run.is_dir() {
                    run
                } else {
                    std::env::temp_dir()
                }
            });
        runtime_dir.join("tddy-index-daemon.sock")
    }
}

/// Project-home sessions (`#project-home` 6/7): how long one may sit idle — its last turn ended
/// and no input arrived — before the daemon stops it. It resumes, same conversation, on next open.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectHomeConfig {
    /// Idle seconds before a project-home session is stopped; `0` disables auto-stop.
    #[serde(default = "default_project_home_idle_stop_secs")]
    pub idle_stop_secs: u64,
}

/// 10 minutes: long enough to read a reply and answer it, short enough not to hold an agent
/// process per project all day.
fn default_project_home_idle_stop_secs() -> u64 {
    10 * 60
}

impl Default for ProjectHomeConfig {
    fn default() -> Self {
        Self {
            idle_stop_secs: default_project_home_idle_stop_secs(),
        }
    }
}
