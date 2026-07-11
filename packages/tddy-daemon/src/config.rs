//! Daemon configuration — users, tools, LiveKit, GitHub, etc.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_core::LogConfig;

fn default_spawn_mouse() -> bool {
    true
}

fn default_spawn_worker_request_timeout_secs() -> u64 {
    300
}

fn default_common_room_set_metadata_timeout_secs() -> u64 {
    60
}

fn default_codex_oauth_loopback_proxy_eligible() -> bool {
    true
}

fn default_relay_idle_timeout_secs() -> u64 {
    1800
}

/// Configuration for relay daemon mode (`--relay` / `relay:` YAML section).
///
/// In relay mode the daemon forwards RPCs to a remote peer via LiveKit, does not require a
/// `web_bundle_path`, and shuts down automatically after `idle_timeout_secs` of inactivity.
#[derive(Debug, Clone, serde::Deserialize)]
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
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalConfig {
    /// Path the daemon binds the local socket at. When unset, resolved at bind time to
    /// `${XDG_RUNTIME_DIR:-/run}/tddy-daemon.sock` (see [`DaemonConfig::local_socket_path`]).
    #[serde(default)]
    pub socket_path: Option<PathBuf>,
}

/// Git behavior for daemon-side operations that contact a remote (fetching integration bases).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitConfig {
    /// Overrides `GIT_SSH_COMMAND` for the daemon's `git fetch` calls only — it is not exported to
    /// the process environment or to spawned children. Point git at an ssh binary that can
    /// authenticate non-interactively; e.g. on macOS the system ssh has Keychain support that the
    /// Nix-provided ssh lacks: `/usr/bin/ssh -o BatchMode=yes -o ConnectTimeout=10`. When unset, git
    /// inherits the ambient environment. Regardless of this value, daemon remote fetches run with
    /// stdin closed and `GIT_TERMINAL_PROMPT=0`, so a missing key/passphrase fails fast instead of
    /// hanging the daemon on a prompt it can never answer.
    #[serde(default)]
    pub ssh_command: Option<String>,
}

/// Linux rootless cgroups sandbox delegation (`sandbox_cgroup:` YAML section). All fields optional
/// so the backend derives defaults at runtime (delegated base from `/proc/self/cgroup`, controllers
/// `memory cpu pids`, supervisor leaf `supervisor`) — nothing is hardcoded in the crate. Consulted
/// only by the Linux cgroups backend; ignored on macOS / QEMU.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxCgroupConfig {
    /// Explicit delegated cgroup base directory; skips `/proc/self/cgroup` derivation when set.
    #[serde(default)]
    pub base_path: Option<PathBuf>,
    /// cgroup v2 unified mount root (default `/sys/fs/cgroup`).
    #[serde(default)]
    pub mount_root: Option<PathBuf>,
    /// Controllers enabled in the base's `cgroup.subtree_control`; empty means `[memory, cpu, pids]`.
    #[serde(default)]
    pub controllers: Vec<String>,
    /// Leaf cgroup the daemon relocates its own process into (default `supervisor`).
    #[serde(default)]
    pub supervisor_leaf: Option<String>,
    /// Default `memory.max` (bytes) applied when a plan carries no explicit limits.
    #[serde(default)]
    pub memory_max: Option<u64>,
    /// Default `cpu.max` applied when a plan carries no explicit limits.
    #[serde(default)]
    pub cpu_max: Option<String>,
    /// Default `pids.max` applied when a plan carries no explicit limits.
    #[serde(default)]
    pub pids_max: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonConfig {
    #[serde(default)]
    pub listen: ListenConfig,
    #[serde(default)]
    pub web_bundle_path: Option<PathBuf>,
    #[serde(default)]
    pub livekit: Option<LiveKitConfig>,
    #[serde(default)]
    pub github: Option<GitHubConfig>,
    #[serde(default)]
    pub auth_storage: Option<PathBuf>,
    #[serde(default)]
    pub log: Option<LogConfig>,
    /// Path to a `tddy-coder` config file (e.g. `dev.config.yaml`) whose `log:` block is passed
    /// verbatim to spawned children as their `--config`, so operators own child log routing (e.g.
    /// routing `libwebrtc*` / `livekit*` chatter to a separate file at INFO). Relative to the
    /// daemon's cwd. When unset, the spawner synthesizes a minimal log config from the daemon `log:`.
    #[serde(default)]
    pub coder_config_path: Option<PathBuf>,
    #[serde(default)]
    pub users: Vec<UserMapping>,
    #[serde(default)]
    pub allowed_tools: Vec<AllowedTool>,
    /// Allowed coding backends / agents (`tddy-coder --agent` values), with optional UI labels.
    #[serde(default)]
    pub allowed_agents: Vec<AllowedAgent>,
    /// Relative to each OS user's home directory (e.g. `repos` → `~/repos/`).
    #[serde(default)]
    pub repos_base_path: Option<String>,
    /// When true (default), spawned `tddy-*` processes receive `--mouse` (browser / touch terminals).
    #[serde(default = "default_spawn_mouse")]
    pub spawn_mouse: bool,
    /// Max seconds for clone/spawn work (`SpawnClient` round-trip or direct `spawn_as_user` /
    /// `clone_as_user`). Minimum effective value is 1.
    #[serde(default = "default_spawn_worker_request_timeout_secs")]
    pub spawn_worker_request_timeout_secs: u64,
    /// Stable id for this daemon in a shared LiveKit room (multi-host). When set, spawned tools
    /// and ConnectSession use `daemon-{instance_id}-{session_id}` as LiveKit server identity.
    /// Overridable at startup via the `TDDY_DAEMON_INSTANCE_ID` env var (see `apply_env_overrides`
    /// in `main.rs`); falls back to the machine short hostname when neither is set.
    #[serde(default)]
    pub daemon_instance_id: Option<String>,
    /// When true, append `-<unix_ms>` once per process to the resolved instance id (YAML override
    /// or hostname default) so multiple local daemons avoid LiveKit `DuplicateIdentity` in
    /// `common_room`. Intended for desktop dev / overlapping CLI embed + standalone runs.
    #[serde(default)]
    pub daemon_instance_id_append_startup_timestamp: bool,
    /// When true (default), this daemon may bind the Codex OAuth loopback TCP port (e.g. 127.0.0.1:1455)
    /// and relay browser callbacks to session hosts via LiveKit. Set false when another process
    /// already uses that port or this instance must not act as the operator-side OAuth proxy.
    #[serde(default = "default_codex_oauth_loopback_proxy_eligible")]
    pub codex_oauth_loopback_proxy_eligible: bool,
    /// Optional Telegram bot notifications (see `telegram_notifier` module).
    #[serde(default)]
    pub telegram: Option<TelegramConfig>,
    /// Claude Code CLI session configuration (spawning interactive `claude` processes in PTYs).
    #[serde(default)]
    pub claude_cli: Option<ClaudeCliConfig>,
    /// Cursor Agent CLI session configuration (spawning interactive `agent` processes in PTYs).
    #[serde(default)]
    pub cursor_cli: Option<CursorCliConfig>,
    /// When set, this daemon runs in relay mode: no web bundle, idle-timeout auto-shutdown,
    /// forwards RPCs to a remote peer via LiveKit.
    #[serde(default)]
    pub relay: Option<RelayConfig>,
    /// Override the tddy home data directory. When absent, the profile default is used
    /// (`tmp/.tddy` in debug builds, `$HOME/.tddy` in release builds).
    #[serde(default)]
    pub tddy_data_dir: Option<PathBuf>,
    /// Screen-sharing bridge binary configuration (VNC + RDP paths).
    #[serde(default)]
    pub screen_sharing: Option<ScreenSharingConfig>,
    /// Git behavior for daemon-side remote operations (see `GitConfig`).
    #[serde(default)]
    pub git: Option<GitConfig>,
    /// Linux rootless cgroups sandbox delegation (see `SandboxCgroupConfig`). None = runtime defaults.
    #[serde(default)]
    pub sandbox_cgroup: Option<SandboxCgroupConfig>,
    /// Local Unix-domain socket transport (see `LocalConfig`). Absent = defaults (socket path
    /// resolved at bind time).
    #[serde(default)]
    pub local: LocalConfig,

    /// Browser DEBUG mask exposed to tddy-web via `GET /api/config` (`debug` field). A `debug`-package
    /// namespace mask (e.g. `tddy:term:*`, or `tddy:term:write,tddy:term:resize`) that enables scoped
    /// `[tddy]` diagnostics in the browser. Mainly for `./web-dev` to debug terminal garbling /
    /// misalignment. The browser invalidates any local override when this value changes. None = off.
    #[serde(default)]
    pub debug: Option<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            listen: ListenConfig::default(),
            web_bundle_path: None,
            livekit: None,
            github: None,
            auth_storage: None,
            log: None,
            coder_config_path: None,
            users: Vec::new(),
            allowed_tools: Vec::new(),
            allowed_agents: Vec::new(),
            repos_base_path: None,
            spawn_mouse: true,
            spawn_worker_request_timeout_secs: default_spawn_worker_request_timeout_secs(),
            daemon_instance_id: None,
            daemon_instance_id_append_startup_timestamp: false,
            codex_oauth_loopback_proxy_eligible: default_codex_oauth_loopback_proxy_eligible(),
            telegram: None,
            claude_cli: None,
            cursor_cli: None,
            relay: None,
            tddy_data_dir: None,
            screen_sharing: None,
            git: None,
            sandbox_cgroup: None,
            local: LocalConfig::default(),
            debug: None,
        }
    }
}

impl DaemonConfig {
    /// Map the optional `sandbox_cgroup:` section onto the cgroups backend's [`CgroupConfig`]. An
    /// absent section yields defaults (empty `CgroupConfig`), so the backend derives everything at
    /// runtime. Only the delegation fields map here; the default-limit overrides feed the plan's
    /// `ResourceLimits` fallback separately.
    pub fn sandbox_cgroup_config(&self) -> tddy_sandbox::CgroupConfig {
        match &self.sandbox_cgroup {
            Some(sc) => tddy_sandbox::CgroupConfig {
                base_override: sc.base_path.clone(),
                mount_root: sc.mount_root.clone(),
                controllers: sc.controllers.clone(),
                supervisor_leaf: sc.supervisor_leaf.clone(),
            },
            None => tddy_sandbox::CgroupConfig::default(),
        }
    }
}

/// Telegram Bot API integration (teloxide). Loaded from daemon YAML under `telegram:`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    pub bot_token: String,
    #[serde(default)]
    pub chat_ids: Vec<i64>,
}

/// Screen-sharing bridge binary configuration. Loaded from daemon YAML under `screen_sharing:`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenSharingConfig {
    /// Path to the `tddy-vnc` bridge binary.
    /// Empty string (default) → resolved at runtime: current_exe sibling, then PATH.
    #[serde(default)]
    pub vnc_binary_path: String,
    /// Path to the `tddy-rdp` bridge binary.
    /// Empty string (default) → resolved at runtime: current_exe sibling, then PATH.
    #[serde(default)]
    pub rdp_binary_path: String,
}

/// Resolve the actual path to the `tddy-vnc` binary.
///
/// Resolution order:
/// 1. Explicit `vnc_binary_path` in `screen_sharing` config (if non-empty).
/// 2. Sibling of the current executable (same directory).
/// 3. Fallback to `"tddy-vnc"` (PATH lookup).
pub fn resolve_vnc_binary_path(config: &DaemonConfig) -> String {
    let explicit = config
        .screen_sharing
        .as_ref()
        .map(|c| c.vnc_binary_path.as_str())
        .unwrap_or("");
    if !explicit.is_empty() {
        return explicit.to_string();
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("tddy-vnc")))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "tddy-vnc".to_string())
}

/// Resolve the actual path to the `tddy-rdp` binary.
///
/// Resolution order:
/// 1. Explicit `rdp_binary_path` in `screen_sharing` config (if non-empty).
/// 2. Sibling of the current executable (same directory).
/// 3. Fallback to `"tddy-rdp"` (PATH lookup).
pub fn resolve_rdp_binary_path(config: &DaemonConfig) -> String {
    let explicit = config
        .screen_sharing
        .as_ref()
        .map(|c| c.rdp_binary_path.as_str())
        .unwrap_or("");
    if !explicit.is_empty() {
        return explicit.to_string();
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("tddy-rdp")))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "tddy-rdp".to_string())
}

fn default_claude_cli_binary_path() -> String {
    "claude".to_string()
}

fn default_cursor_cli_binary_path() -> String {
    "agent".to_string()
}

/// Environment variable that overrides the resolved `claude` binary path (highest priority).
/// The escape hatch for when host auto-resolution picks the wrong binary.
pub const CLAUDE_BINARY_ENV: &str = "TDDY_CLAUDE_BINARY";

/// A wrapper-shim `bin` directory whose `claude` merely re-execs the real binary from `$PATH`
/// (e.g. Superset's `~/.superset/bin`). Such a shim can't resolve inside the jail — its `PATH` is
/// only `/usr/bin:/bin` — so host resolution must skip these dirs. See the `claude-sandbox` script.
fn is_wrapper_shim_dir(dir: &Path) -> bool {
    let ends_with_bin = dir.file_name().is_some_and(|n| n == "bin");
    ends_with_bin
        && dir.components().any(|c| match c {
            std::path::Component::Normal(n) => n.to_string_lossy().starts_with(".superset"),
            _ => false,
        })
}

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(p: &Path) -> bool {
    p.is_file()
}

/// Locate the real host `claude` as an absolute path: prefer `~/.local/bin/claude` (the canonical
/// Claude Code install), then scan `$PATH` skipping wrapper-shim dirs. `None` if none is found.
fn find_real_claude_on_host() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home).join(".local/bin/claude");
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter(|d| !d.as_os_str().is_empty() && !is_wrapper_shim_dir(d))
        .map(|d| d.join("claude"))
        .find(|c| is_executable_file(c))
}

/// Whether the `claude` binary is taken from an explicit override or must be auto-resolved.
enum ClaudeBinaryChoice {
    /// Use this value as-is (an env override or an explicit config path).
    Explicit(String),
    /// No explicit path given — resolve the real `claude` from the host.
    Auto,
}

/// Decide from the env override and configured value alone (no filesystem access, so it is
/// unit-testable): a non-empty env override wins; otherwise a configured value naming a path
/// (contains `/`) is explicit; a bare name means "auto-resolve".
fn choose_claude_binary(env_override: Option<&str>, configured: &str) -> ClaudeBinaryChoice {
    if let Some(env) = env_override.map(str::trim).filter(|s| !s.is_empty()) {
        return ClaudeBinaryChoice::Explicit(env.to_string());
    }
    if configured.contains('/') {
        return ClaudeBinaryChoice::Explicit(configured.to_string());
    }
    ClaudeBinaryChoice::Auto
}

/// Canonicalize a value that names a path (contains `/`); return a bare name unchanged. The SBPL
/// allow-list is built from canonical (symlink-resolved) parent dirs, so a symlinked spelling would
/// be denied at exec time.
fn canonicalize_if_path(p: &str) -> String {
    if p.contains('/') {
        std::fs::canonicalize(p)
            .map(|c| c.to_string_lossy().into_owned())
            .unwrap_or_else(|_| p.to_string())
    } else {
        p.to_string()
    }
}

/// Resolve the `claude` binary the sandbox runner will exec, as an absolute path when possible.
///
/// Resolution order (first match wins):
/// 1. `TDDY_CLAUDE_BINARY` env var, when set and non-empty ([`CLAUDE_BINARY_ENV`]).
/// 2. `claude_cli.binary_path` in the daemon config, when it names a path (contains `/`).
/// 3. Auto-resolution of the real host `claude`: `~/.local/bin/claude`, then `$PATH` (skipping
///    wrapper-shim dirs like Superset's `~/.superset/bin`, which can't resolve inside the jail).
/// 4. Fallback: the configured value as-is (bare name, default `"claude"`).
///
/// A bare name must never reach the runner in normal operation: `binary_exec_reads` would take its
/// empty parent (`Path::parent` of `"claude"` is `Some("")`) and emit `(subpath "")`, which macOS
/// `sandbox-exec` rejects. Steps 1–3 produce an absolute path; step 4 is a last resort.
pub fn resolve_claude_binary_path(config: &DaemonConfig) -> String {
    let configured = config
        .claude_cli
        .as_ref()
        .map(|c| c.binary_path.as_str())
        .unwrap_or("claude");
    let env_override = std::env::var(CLAUDE_BINARY_ENV).ok();
    match choose_claude_binary(env_override.as_deref(), configured) {
        ClaudeBinaryChoice::Explicit(p) => canonicalize_if_path(&p),
        ClaudeBinaryChoice::Auto => match find_real_claude_on_host() {
            Some(real) => canonicalize_if_path(&real.to_string_lossy()),
            None => configured.to_string(),
        },
    }
}

/// Claude Code CLI session configuration. Loaded from daemon YAML under `claude_cli:`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeCliConfig {
    /// Path to the `claude` binary. A path (contains `/`) is used verbatim; a bare name (default
    /// `"claude"`) triggers host auto-resolution — see [`resolve_claude_binary_path`]. Override
    /// with the `TDDY_CLAUDE_BINARY` env var when resolution picks the wrong binary.
    #[serde(default = "default_claude_cli_binary_path")]
    pub binary_path: String,
    /// Absolute path to the `tddy-tools` binary for per-worktree hook commands.
    /// When absent, the daemon resolves it from `current_exe` sibling or falls back to
    /// `"tddy-tools"` (PATH lookup).
    #[serde(default)]
    pub tddy_tools_path: Option<String>,
    /// HTTP base URL the per-worktree hook command uses to call `ReportSessionStatus`.
    /// When absent, defaults to `http://127.0.0.1:{web_port}` derived from the listen config.
    #[serde(default)]
    pub daemon_url: Option<String>,
    /// Persistent jail `$HOME` reused across sandboxed claude-cli sessions so auth (refreshed OAuth
    /// tokens), session history, and settings survive between sessions. Absent → default
    /// `$HOME/.tddy/sandbox-claude-home`. Override with the `TDDY_SANDBOX_CLAUDE_HOME` env var. A
    /// single daemon-wide home — see [`resolve_claude_home_dir`].
    #[serde(default)]
    pub claude_home_dir: Option<PathBuf>,
}

/// Cursor Agent CLI session configuration. Loaded from daemon YAML under `cursor_cli:`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CursorCliConfig {
    /// Path to the Cursor Agent CLI binary (default `"agent"`).
    #[serde(default = "default_cursor_cli_binary_path")]
    pub binary_path: String,
    /// Absolute path to `tddy-tools` for per-worktree hook commands. Falls back to claude_cli or
    /// sibling-of-daemon resolution when absent.
    #[serde(default)]
    pub tddy_tools_path: Option<String>,
    /// HTTP base URL for `ReportSessionStatus`. Falls back to claude_cli or web_port default.
    #[serde(default)]
    pub daemon_url: Option<String>,
    /// Persistent jail `$HOME` for sandboxed cursor-cli sessions (default: `$HOME/.tddy/sandbox-cursor-home`).
    #[serde(default)]
    pub cursor_home_dir: Option<PathBuf>,
}

/// Resolve the Cursor Agent CLI binary path from config and env.
///
/// Resolution: `cursor_cli.binary_path` → `TDDY_CURSOR_AGENT` env → `"agent"`.
pub fn resolve_cursor_binary_path(config: &DaemonConfig) -> String {
    if let Ok(env) = std::env::var("TDDY_CURSOR_AGENT") {
        if !env.trim().is_empty() {
            return env;
        }
    }
    config
        .cursor_cli
        .as_ref()
        .map(|c| c.binary_path.as_str())
        .unwrap_or("agent")
        .to_string()
}

/// Resolve tddy-tools path for cursor-cli hooks (cursor config → claude config → default).
pub fn resolve_cursor_cli_tddy_tools_path(config: &DaemonConfig) -> Option<String> {
    config
        .cursor_cli
        .as_ref()
        .and_then(|c| c.tddy_tools_path.clone())
        .or_else(|| {
            config
                .claude_cli
                .as_ref()
                .and_then(|c| c.tddy_tools_path.clone())
        })
}

/// Resolve daemon URL for cursor-cli hooks (cursor config → claude config).
pub fn resolve_cursor_cli_daemon_url(config: &DaemonConfig) -> Option<String> {
    config
        .cursor_cli
        .as_ref()
        .and_then(|c| c.daemon_url.clone())
        .or_else(|| {
            config
                .claude_cli
                .as_ref()
                .and_then(|c| c.daemon_url.clone())
        })
}

/// Environment variable overriding the persistent sandbox cursor `$HOME`.
pub const CURSOR_HOME_ENV: &str = "TDDY_SANDBOX_CURSOR_HOME";

/// Resolve the single daemon-wide persistent jail `$HOME` for sandboxed cursor-cli sessions.
pub fn resolve_cursor_home_dir(config: &DaemonConfig) -> PathBuf {
    if let Some(env) = std::env::var_os(CURSOR_HOME_ENV).filter(|v| !v.is_empty()) {
        return PathBuf::from(env);
    }
    if let Some(dir) = config
        .cursor_cli
        .as_ref()
        .and_then(|c| c.cursor_home_dir.as_ref())
    {
        return dir.clone();
    }
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".tddy").join("sandbox-cursor-home")
}

/// Environment variable overriding the persistent sandbox claude `$HOME`.
pub const CLAUDE_HOME_ENV: &str = "TDDY_SANDBOX_CLAUDE_HOME";

/// Resolve the single daemon-wide persistent jail `$HOME` for sandboxed claude-cli sessions.
///
/// Resolution order (first match wins):
/// 1. `TDDY_SANDBOX_CLAUDE_HOME` env var (if set and non-empty).
/// 2. `claude_cli.claude_home_dir` in the daemon config.
/// 3. Default `$HOME/.tddy/sandbox-claude-home`.
///
/// The dir is reused across sessions and mounted read-write into the jail, so credentials/history
/// persist. It is deliberately separate from the daemon user's real `~/.claude`.
pub fn resolve_claude_home_dir(config: &DaemonConfig) -> PathBuf {
    if let Some(env) = std::env::var_os(CLAUDE_HOME_ENV).filter(|v| !v.is_empty()) {
        return PathBuf::from(env);
    }
    if let Some(dir) = config
        .claude_cli
        .as_ref()
        .and_then(|c| c.claude_home_dir.as_ref())
    {
        return dir.clone();
    }
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".tddy").join("sandbox-claude-home")
}

/// Environment variable naming the shared sandbox config file explicitly (highest priority).
pub const SANDBOX_CONFIG_ENV: &str = "TDDY_SANDBOX_CONFIG";

/// Basename (no extension) of the shared claude-sandbox config file. The per-OS variant inserts the
/// OS token: `claude-sandbox.<os>.yaml`; the generic form is `claude-sandbox.yaml`.
pub const SANDBOX_CONFIG_BASENAME: &str = "claude-sandbox";

/// OS token used in per-OS config filenames (e.g. `claude-sandbox.darwin.yaml`).
///
/// Uses the repo's convention — `"darwin"` for macOS (matching the `tddy-sandbox-darwin` crate),
/// `"linux"` for Linux — rather than Rust's `std::env::consts::OS` (`"macos"`). A per-OS file is
/// host-neutral: the same `claude-sandbox.darwin.yaml` works on any macOS host.
pub fn sandbox_config_os_token() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "darwin"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        std::env::consts::OS
    }
}

/// Resolve which shared claude-sandbox config file the daemon should load, searching `base_dir`.
///
/// Order (first existing wins):
/// 1. `TDDY_SANDBOX_CONFIG` env — an explicit path, used as-is when it exists.
/// 2. Per-OS `<base>/claude-sandbox.<os>.yaml` (e.g. `claude-sandbox.darwin.yaml`) — committed per
///    platform, host-neutral within that OS.
/// 3. Generic `<base>/claude-sandbox.yaml`.
///
/// `None` → no file found; the daemon falls back to built-in defaults (zero-config still works).
pub fn resolve_sandbox_config_path(base_dir: &Path) -> Option<PathBuf> {
    resolve_sandbox_config_path_with(
        base_dir,
        sandbox_config_os_token(),
        std::env::var(SANDBOX_CONFIG_ENV).ok().as_deref(),
        |p| p.is_file(),
    )
}

/// Pure core of [`resolve_sandbox_config_path`] — env override, OS token, and an existence probe
/// are all injected so the precedence is unit-testable without touching the real filesystem/env.
fn resolve_sandbox_config_path_with(
    base_dir: &Path,
    os_token: &str,
    env_override: Option<&str>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(env) = env_override.map(str::trim).filter(|s| !s.is_empty()) {
        let p = PathBuf::from(env);
        if exists(&p) {
            return Some(p);
        }
    }
    let per_os = base_dir.join(format!("{SANDBOX_CONFIG_BASENAME}.{os_token}.yaml"));
    if exists(&per_os) {
        return Some(per_os);
    }
    let generic = base_dir.join(format!("{SANDBOX_CONFIG_BASENAME}.yaml"));
    if exists(&generic) {
        return Some(generic);
    }
    None
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListenConfig {
    #[serde(default)]
    pub web_port: Option<u16>,
    #[serde(default)]
    pub web_host: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveKitConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_secret: Option<String>,
    #[serde(default)]
    pub public_url: Option<String>,
    /// Shared LiveKit room for presence (browser + tddy-* tools). Exposed to web as `common_room` in `/api/config`.
    #[serde(default)]
    pub common_room: Option<String>,
    /// Total wall-clock time the daemon spends retrying `set_metadata` for the common-room advertisement
    /// in one publish round. The LiveKit Rust SDK uses a fixed **5 s** timeout **per attempt**; this value
    /// caps how long we keep trying before treating the round as failed (minimum effective **1** second).
    #[serde(default = "default_common_room_set_metadata_timeout_secs")]
    pub common_room_set_metadata_timeout_secs: u64,
}

impl Default for LiveKitConfig {
    fn default() -> Self {
        Self {
            url: None,
            api_key: None,
            api_secret: None,
            public_url: None,
            common_room: None,
            common_room_set_metadata_timeout_secs: default_common_room_set_metadata_timeout_secs(),
        }
    }
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHubConfig {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub stub: Option<bool>,
    #[serde(default)]
    pub stub_codes: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserMapping {
    pub github_user: String,
    pub os_user: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedTool {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedAgent {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
}

impl DaemonConfig {
    /// Default subdirectory under home for cloned project repos when `repos_base_path` is unset.
    pub fn repos_base_path_or_default(&self) -> &str {
        self.repos_base_path.as_deref().unwrap_or("repos")
    }

    /// Load config from a YAML file.
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read config {}: {}", path.display(), e))?;
        let config: Self = serde_yaml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("failed to parse config {}: {}", path.display(), e))?;
        Ok(config)
    }

    /// Resolve OS user for a GitHub user. Returns None if not mapped.
    pub fn os_user_for_github(&self, github_user: &str) -> Option<&str> {
        self.users
            .iter()
            .find(|u| u.github_user == github_user)
            .map(|u| u.os_user.as_str())
    }

    /// Resolve the local caller's identity from its peer uid (SO_PEERCRED), for the local peer-trust
    /// auth path used by same-host clients (e.g. tddy-sandbox-app). `uid_to_username` maps a uid to
    /// an OS username — injected so this is host-independent and unit-testable — and the result is
    /// matched against a configured `users[]` entry by `os_user`. Returns None when the uid has no
    /// username or no matching user mapping.
    pub fn local_identity_for_uid(
        &self,
        uid: u32,
        uid_to_username: impl Fn(u32) -> Option<String>,
    ) -> Option<&UserMapping> {
        let username = uid_to_username(uid)?;
        self.users.iter().find(|u| u.os_user == username)
    }

    /// The GitHub login for a local caller's peer uid (SO_PEERCRED), used to mint its access token
    /// in the local peer-trust auth path. Thin wrapper over [`Self::local_identity_for_uid`].
    pub fn local_token_login_for_uid(
        &self,
        uid: u32,
        uid_to_username: impl Fn(u32) -> Option<String>,
    ) -> Option<String> {
        self.local_identity_for_uid(uid, uid_to_username)
            .map(|m| m.github_user.clone())
    }

    /// Resolve the path the local Unix-domain socket binds at. Uses the explicit `local.socket_path`
    /// when configured; otherwise `${XDG_RUNTIME_DIR}/tddy-daemon.sock`, falling back to
    /// `/run/tddy-daemon.sock` when `XDG_RUNTIME_DIR` is unset.
    pub fn local_socket_path(&self) -> PathBuf {
        if let Some(path) = &self.local.socket_path {
            return path.clone();
        }
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/run"));
        runtime_dir.join("tddy-daemon.sock")
    }

    /// List allowed tools with path and label.
    pub fn allowed_tools(&self) -> &[AllowedTool] {
        &self.allowed_tools
    }

    /// The tool path to spawn when nothing more specific was requested — the first configured
    /// `allowed_tools` entry (e.g. `"target/debug/tddy-coder"` in dev), falling back to a bare
    /// `"tddy-coder"` only when no tools are configured at all. Single source of truth: callers
    /// (`StartSession`'s Telegram workflow spawn path, `ResumeSession`'s no-recorded-tool
    /// fallback) must not each hardcode their own default, or they can silently drift apart.
    pub fn default_tool_path(&self) -> String {
        self.allowed_tools
            .first()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| "tddy-coder".to_string())
    }

    /// Allowed agent ids (`StartSession.agent` / `tddy-coder --agent`) and display labels.
    pub fn allowed_agents(&self) -> &[AllowedAgent] {
        &self.allowed_agents
    }

    /// Wall-clock limit for blocking clone/spawn operations (spawn worker or in-process).
    pub fn spawn_worker_request_timeout(&self) -> Duration {
        let secs = self.spawn_worker_request_timeout_secs.max(1);
        Duration::from_secs(secs)
    }

    /// Wall-clock budget for one common-room daemon-advertisement `set_metadata` round (the LiveKit SDK
    /// still uses **5 s per attempt**; we retry until this budget elapses or the publish succeeds).
    pub fn common_room_set_metadata_attempt_budget(&self) -> Duration {
        let secs = self
            .livekit
            .as_ref()
            .map(|l| l.common_room_set_metadata_timeout_secs)
            .unwrap_or_else(default_common_room_set_metadata_timeout_secs)
            .max(1);
        Duration::from_secs(secs)
    }

    /// Merge Telegram settings from process environment (after YAML load).
    ///
    /// Variables:
    /// - `TDDY_TELEGRAM_BOT_TOKEN` — Bot API token; when set, assigns the token. If there was no
    ///   `telegram:` block in YAML, a new block is created.
    /// - `TDDY_TELEGRAM_CHAT_IDS` — Comma-separated chat ids (e.g. `-1001234567890,123456`).
    /// - `TDDY_TELEGRAM_ENABLED` — `true`/`false`/`1`/`0`/`yes`/`no`/`on`/`off` (case-insensitive).
    ///
    /// When a new `telegram` block is created solely because `TDDY_TELEGRAM_BOT_TOKEN` is set,
    /// `enabled` defaults to `true` unless `TDDY_TELEGRAM_ENABLED` is set. When merging into an
    /// existing YAML `telegram` block, `enabled` is not changed by the token alone (set
    /// `TDDY_TELEGRAM_ENABLED` explicitly).
    pub fn apply_telegram_env_overrides(&mut self) {
        let bot_token = non_empty_env("TDDY_TELEGRAM_BOT_TOKEN");
        let chat_ids_csv = non_empty_env("TDDY_TELEGRAM_CHAT_IDS");
        let enabled = non_empty_env("TDDY_TELEGRAM_ENABLED");
        merge_telegram_env(
            self,
            bot_token.as_deref(),
            chat_ids_csv.as_deref(),
            enabled.as_deref(),
        );
    }

    /// Validate that this config is suitable for relay mode.
    ///
    /// Relay mode requires the `relay:` section to be present. It does not require
    /// `web_bundle_path` — relay daemons do not serve static files.
    pub fn validate_for_relay(&self) -> anyhow::Result<()> {
        if self.relay.is_some() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("relay section is required in relay mode"))
        }
    }

    /// Override [`Self::codex_oauth_loopback_proxy_eligible`] from `TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE`
    /// (`true`/`false`/`1`/`0`/`yes`/`no`/`on`/`off`, case-insensitive). Call after YAML load.
    pub fn apply_oauth_loopback_proxy_env_override(&mut self) {
        if let Some(s) = non_empty_env("TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE") {
            if let Some(b) = parse_env_bool(&s) {
                self.codex_oauth_loopback_proxy_eligible = b;
            } else {
                log::warn!(
                    target: "tddy_daemon::config",
                    "TDDY_CODEX_OAUTH_LOOPBACK_PROXY_ELIGIBLE: expected true/false/1/0/yes/no/on/off, ignoring"
                );
            }
        }
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(s)
        }
    })
}

fn parse_chat_ids_csv(s: &str) -> Result<Vec<i64>, ()> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let n: i64 = p.parse().map_err(|_| ())?;
        out.push(n);
    }
    Ok(out)
}

fn parse_env_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn merge_telegram_env(
    config: &mut DaemonConfig,
    bot_token: Option<&str>,
    chat_ids_csv: Option<&str>,
    enabled: Option<&str>,
) {
    if bot_token.is_none() && chat_ids_csv.is_none() && enabled.is_none() {
        return;
    }

    let mut created_from_env_token = false;
    if config.telegram.is_none() {
        if bot_token.is_none() {
            if chat_ids_csv.is_some() || enabled.is_some() {
                log::warn!(
                    target: "tddy_daemon::config",
                    "telegram: set TDDY_TELEGRAM_BOT_TOKEN or add a `telegram:` block to the config file before using TDDY_TELEGRAM_CHAT_IDS / TDDY_TELEGRAM_ENABLED"
                );
            }
            return;
        }
        created_from_env_token = true;
        config.telegram = Some(TelegramConfig {
            enabled: false,
            bot_token: String::new(),
            chat_ids: Vec::new(),
        });
    }

    let tg = config.telegram.as_mut().expect("telegram just ensured");

    if let Some(t) = bot_token {
        tg.bot_token = t.trim().to_string();
    }

    if let Some(csv) = chat_ids_csv {
        match parse_chat_ids_csv(csv) {
            Ok(ids) if !ids.is_empty() => tg.chat_ids = ids,
            Ok(_) => {}
            Err(()) => log::warn!(
                target: "tddy_daemon::config",
                "TDDY_TELEGRAM_CHAT_IDS: expected comma-separated integers, ignoring"
            ),
        }
    }

    if let Some(e) = enabled {
        match parse_env_bool(e) {
            Some(b) => tg.enabled = b,
            None => log::warn!(
                target: "tddy_daemon::config",
                "TDDY_TELEGRAM_ENABLED: expected true/false/1/0/yes/no/on/off, ignoring"
            ),
        }
    } else if bot_token.is_some() && created_from_env_token {
        tg.enabled = true;
    }
}

/// The single source of truth for "what tool should a session run when nothing more specific was
/// requested" — used by `StartSession`/`ResumeSession`/Telegram workflow spawn alike, so they
/// can never independently drift on the default (see `resume_session`'s prior bug: it
/// hardcoded a bare `"tddy-coder"` fallback that only resolves when the binary happens to be on
/// `PATH`, unlike this method's `allowed_tools`-driven default, e.g. `"target/debug/tddy-coder"`
/// in dev — a session resumed after its metadata never recorded an explicit `tool` value failed
/// to spawn with "No such file or directory").
#[cfg(test)]
mod default_tool_path_tests {
    use super::*;

    #[test]
    fn default_tool_path_uses_the_first_configured_allowed_tool() {
        // Given — dev.daemon.yaml's real shape: a debug-build tool path configured first
        let config = DaemonConfig {
            allowed_tools: vec![AllowedTool {
                path: "target/debug/tddy-coder".to_string(),
                label: Some("tddy-coder (debug)".to_string()),
            }],
            ..Default::default()
        };

        // When
        let default_path = config.default_tool_path();

        // Then
        assert_eq!(default_path, "target/debug/tddy-coder");
    }

    #[test]
    fn default_tool_path_falls_back_to_a_bare_binary_name_when_no_tools_are_configured() {
        // Given — no allowed_tools configured at all
        let config = DaemonConfig {
            allowed_tools: vec![],
            ..Default::default()
        };

        // When
        let default_path = config.default_tool_path();

        // Then
        assert_eq!(default_path, "tddy-coder");
    }
}

#[cfg(test)]
mod telegram_env_tests {
    use super::*;

    #[test]
    fn token_from_env_creates_telegram_and_enables_by_default() {
        let mut c = DaemonConfig::default();
        merge_telegram_env(&mut c, Some("tok"), None, None);
        let tg = c.telegram.as_ref().expect("telegram");
        assert_eq!(tg.bot_token, "tok");
        assert!(tg.enabled);
        assert!(tg.chat_ids.is_empty());
    }

    #[test]
    fn token_override_on_existing_yaml_does_not_auto_enable() {
        let mut c = DaemonConfig {
            telegram: Some(TelegramConfig {
                enabled: false,
                bot_token: "old".to_string(),
                chat_ids: vec![1],
            }),
            ..Default::default()
        };
        merge_telegram_env(&mut c, Some("new"), None, None);
        let tg = c.telegram.as_ref().expect("telegram");
        assert_eq!(tg.bot_token, "new");
        assert!(!tg.enabled);
        assert_eq!(tg.chat_ids, vec![1]);
    }

    #[test]
    fn chat_ids_from_env_merge_into_existing_block() {
        let mut c = DaemonConfig {
            telegram: Some(TelegramConfig {
                enabled: true,
                bot_token: "t".to_string(),
                chat_ids: vec![1],
            }),
            ..Default::default()
        };
        merge_telegram_env(&mut c, None, Some("-100, 42"), None);
        let tg = c.telegram.as_ref().expect("telegram");
        assert_eq!(tg.chat_ids, vec![-100, 42]);
    }

    #[test]
    fn enabled_false_disables_even_when_created_from_env_token() {
        let mut c = DaemonConfig::default();
        merge_telegram_env(&mut c, Some("tok"), None, Some("false"));
        let tg = c.telegram.as_ref().expect("telegram");
        assert_eq!(tg.bot_token, "tok");
        assert!(!tg.enabled);
    }
}

#[cfg(test)]
mod spawn_timeout_tests {
    use super::*;

    #[test]
    fn default_spawn_worker_request_timeout_is_300_seconds() {
        let c = DaemonConfig::default();
        assert_eq!(c.spawn_worker_request_timeout_secs, 300);
        assert_eq!(c.spawn_worker_request_timeout().as_secs(), 300);
    }

    #[test]
    fn spawn_worker_request_timeout_clamps_zero_to_one_second() {
        let c = DaemonConfig {
            spawn_worker_request_timeout_secs: 0,
            ..Default::default()
        };
        assert_eq!(c.spawn_worker_request_timeout().as_secs(), 1);
    }

    #[test]
    fn common_room_set_metadata_budget_defaults_to_60_seconds() {
        let c = DaemonConfig::default();
        assert_eq!(c.common_room_set_metadata_attempt_budget().as_secs(), 60);
        let c = DaemonConfig {
            livekit: Some(LiveKitConfig::default()),
            ..Default::default()
        };
        assert_eq!(
            c.livekit
                .as_ref()
                .unwrap()
                .common_room_set_metadata_timeout_secs,
            60
        );
        assert_eq!(c.common_room_set_metadata_attempt_budget().as_secs(), 60);
    }

    #[test]
    fn common_room_set_metadata_budget_clamps_zero_to_one_second() {
        let c = DaemonConfig {
            livekit: Some(LiveKitConfig {
                common_room_set_metadata_timeout_secs: 0,
                ..LiveKitConfig::default()
            }),
            ..Default::default()
        };
        assert_eq!(c.common_room_set_metadata_attempt_budget().as_secs(), 1);
    }

    #[test]
    fn codex_oauth_loopback_proxy_eligible_defaults_true() {
        let c = DaemonConfig::default();
        assert!(c.codex_oauth_loopback_proxy_eligible);
    }
}

#[cfg(test)]
mod claude_cli_config_tests {
    use super::*;

    /// When `claude_cli:` is omitted from YAML entirely, `DaemonConfig::claude_cli` is `None`.
    #[test]
    fn claude_cli_absent_when_not_in_yaml() {
        let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(
            c.claude_cli.is_none(),
            "claude_cli must be absent when not configured"
        );
    }

    /// `tddy_tools_path` and `daemon_url` both default to `None` when the section is present
    /// but the new fields are not specified.
    #[test]
    fn claude_cli_config_defaults_new_paths_to_none() {
        let yaml = "
users:
  - github_user: u
    os_user: u
claude_cli:
  binary_path: /usr/local/bin/claude
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        let cli = c.claude_cli.as_ref().expect("claude_cli must be present");
        assert_eq!(
            cli.tddy_tools_path, None,
            "tddy_tools_path must default to None"
        );
        assert_eq!(cli.daemon_url, None, "daemon_url must default to None");
    }

    /// When both new fields are explicit, they round-trip correctly through YAML parsing.
    #[test]
    fn claude_cli_config_parses_explicit_tddy_tools_path_and_daemon_url() {
        let yaml = "
users:
  - github_user: u
    os_user: u
claude_cli:
  binary_path: claude
  tddy_tools_path: /usr/local/bin/tddy-tools
  daemon_url: http://127.0.0.1:9000
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        let cli = c.claude_cli.as_ref().expect("claude_cli must be present");
        assert_eq!(
            cli.tddy_tools_path.as_deref(),
            Some("/usr/local/bin/tddy-tools")
        );
        assert_eq!(cli.daemon_url.as_deref(), Some("http://127.0.0.1:9000"));
    }

    /// A non-empty env override always wins, even over an explicit config path.
    #[test]
    fn env_override_wins_over_configured_path() {
        assert!(matches!(
            choose_claude_binary(Some("/opt/claude"), "/usr/local/bin/claude"),
            ClaudeBinaryChoice::Explicit(p) if p == "/opt/claude"
        ));
    }

    /// A blank/whitespace env override is ignored (falls through to config/auto).
    #[test]
    fn blank_env_override_is_ignored() {
        assert!(matches!(
            choose_claude_binary(Some("   "), "claude"),
            ClaudeBinaryChoice::Auto
        ));
    }

    /// A configured value naming a path is explicit; a bare name auto-resolves from the host.
    #[test]
    fn configured_path_is_explicit_but_bare_name_auto_resolves() {
        assert!(matches!(
            choose_claude_binary(None, "/usr/local/bin/claude"),
            ClaudeBinaryChoice::Explicit(_)
        ));
        assert!(matches!(
            choose_claude_binary(None, "claude"),
            ClaudeBinaryChoice::Auto
        ));
    }

    /// `claude_home_dir` round-trips from YAML, and a configured value is what the resolver returns
    /// (when the `TDDY_SANDBOX_CLAUDE_HOME` env override is not set, the normal case in tests).
    #[test]
    fn claude_home_dir_parses_and_resolves_from_config() {
        let yaml = "
users:
  - github_user: u
    os_user: u
claude_cli:
  binary_path: claude
  claude_home_dir: /custom/claude-home
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(
            c.claude_cli.as_ref().unwrap().claude_home_dir.as_deref(),
            Some(Path::new("/custom/claude-home"))
        );
        if std::env::var_os(CLAUDE_HOME_ENV).is_none() {
            assert_eq!(
                resolve_claude_home_dir(&c),
                PathBuf::from("/custom/claude-home"),
                "a configured claude_home_dir must be returned when the env override is unset"
            );
        }
    }

    /// With no `claude_cli` section the resolver falls back to `$HOME/.tddy/sandbox-claude-home`.
    #[test]
    fn claude_home_dir_defaults_under_tddy_when_unconfigured() {
        if std::env::var_os(CLAUDE_HOME_ENV).is_some() {
            return; // env override in effect; default path not exercised
        }
        let c = DaemonConfig::default();
        let resolved = resolve_claude_home_dir(&c);
        assert!(
            resolved.ends_with(".tddy/sandbox-claude-home"),
            "default home must be under ~/.tddy: {}",
            resolved.display()
        );
    }

    /// The per-OS config file wins over the generic one when both are present.
    #[test]
    fn per_os_sandbox_config_takes_precedence_over_generic() {
        let base = Path::new("/cfg");
        // Both present → per-OS wins.
        let got = resolve_sandbox_config_path_with(base, "darwin", None, |p| {
            p == Path::new("/cfg/claude-sandbox.darwin.yaml")
                || p == Path::new("/cfg/claude-sandbox.yaml")
        });
        assert_eq!(got, Some(PathBuf::from("/cfg/claude-sandbox.darwin.yaml")));
        // Only generic present → generic.
        let got = resolve_sandbox_config_path_with(base, "darwin", None, |p| {
            p == Path::new("/cfg/claude-sandbox.yaml")
        });
        assert_eq!(got, Some(PathBuf::from("/cfg/claude-sandbox.yaml")));
    }

    /// The env override wins when it points at an existing file; a missing env target is skipped in
    /// favour of the per-OS/generic search. Absent everything → `None` (defaults).
    #[test]
    fn sandbox_config_env_override_and_absence() {
        let base = Path::new("/cfg");
        // Env points at an existing file → used as-is.
        let got =
            resolve_sandbox_config_path_with(base, "darwin", Some("/explicit/cfg.yaml"), |p| {
                p == Path::new("/explicit/cfg.yaml")
            });
        assert_eq!(got, Some(PathBuf::from("/explicit/cfg.yaml")));
        // Env set but missing → falls through to the per-OS file.
        let got =
            resolve_sandbox_config_path_with(base, "darwin", Some("/explicit/missing.yaml"), |p| {
                p == Path::new("/cfg/claude-sandbox.darwin.yaml")
            });
        assert_eq!(got, Some(PathBuf::from("/cfg/claude-sandbox.darwin.yaml")));
        // Nothing exists → None.
        let got = resolve_sandbox_config_path_with(base, "darwin", None, |_| false);
        assert_eq!(got, None);
    }

    /// On macOS the OS token is the repo's `"darwin"` (not Rust's `"macos"`).
    #[test]
    #[cfg(target_os = "macos")]
    fn os_token_is_darwin_on_macos() {
        assert_eq!(sandbox_config_os_token(), "darwin");
    }

    /// Wrapper-shim `bin` dirs (Superset) are recognised so PATH resolution skips them; real
    /// install dirs are not.
    #[test]
    fn recognises_wrapper_shim_dirs() {
        assert!(is_wrapper_shim_dir(Path::new("/Users/x/.superset/bin")));
        assert!(is_wrapper_shim_dir(Path::new(
            "/Users/x/.superset-worktrees/proj/bin"
        )));
        assert!(!is_wrapper_shim_dir(Path::new("/Users/x/.local/bin")));
        assert!(!is_wrapper_shim_dir(Path::new("/usr/bin")));
        // Only the `bin` leaf under a `.superset*` component counts.
        assert!(!is_wrapper_shim_dir(Path::new("/Users/x/.superset/lib")));
    }
}

#[cfg(test)]
mod web_debug_mask_tests {
    use super::*;

    #[test]
    fn debug_mask_defaults_to_none() {
        let c = DaemonConfig::default();
        assert!(c.debug.is_none());
    }

    #[test]
    fn debug_mask_absent_in_yaml_is_none() {
        let yaml = "
users:
  - github_user: u
    os_user: u
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(c.debug.is_none());
    }

    #[test]
    fn debug_mask_parses_from_yaml() {
        let yaml = "
debug: \"tddy:term:*\"
users:
  - github_user: u
    os_user: u
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(c.debug.as_deref(), Some("tddy:term:*"));
    }
}

#[cfg(test)]
mod git_config_tests {
    use super::*;

    #[test]
    fn git_ssh_command_absent_is_none() {
        let yaml = "
users:
  - github_user: u
    os_user: u
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(c.git.is_none(), "git section must be absent by default");
    }

    #[test]
    fn git_ssh_command_parses_from_yaml() {
        let yaml = "
git:
  ssh_command: \"/usr/bin/ssh -o BatchMode=yes -o ConnectTimeout=10\"
users:
  - github_user: u
    os_user: u
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        let git = c.git.as_ref().expect("git section must be present");
        assert_eq!(
            git.ssh_command.as_deref(),
            Some("/usr/bin/ssh -o BatchMode=yes -o ConnectTimeout=10")
        );
    }

    #[test]
    fn git_section_present_but_ssh_command_absent_is_none() {
        let yaml = "
git: {}
users:
  - github_user: u
    os_user: u
";
        let c: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");
        let git = c.git.as_ref().expect("git section must be present");
        assert!(git.ssh_command.is_none());
    }

    #[test]
    fn maps_daemon_sandbox_cgroup_config_onto_the_plan_field() {
        // Given — a daemon config with an explicit sandbox_cgroup section
        let yaml = "
sandbox_cgroup:
  base_path: /custom/delegated/base
  controllers: [memory, pids]
  supervisor_leaf: worker
users:
  - github_user: u
    os_user: u
";
        let config: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");

        // When
        let cgroup = config.sandbox_cgroup_config();

        // Then
        assert_eq!(
            cgroup.base_override,
            Some(std::path::PathBuf::from("/custom/delegated/base"))
        );
        assert_eq!(
            cgroup.controllers,
            vec!["memory".to_string(), "pids".to_string()]
        );
        assert_eq!(cgroup.supervisor_leaf, Some("worker".to_string()));
    }

    #[test]
    fn resolves_local_identity_from_a_mapped_peer_uid() {
        // Given — github user octocat maps to os_user dev1, and peer uid 1002 resolves to "dev1"
        let yaml = "
users:
  - github_user: octocat
    os_user: dev1
";
        let config: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");

        // When
        let identity =
            config.local_identity_for_uid(1002, |uid| (uid == 1002).then(|| "dev1".to_string()));

        // Then
        let mapping = identity.expect("uid 1002 must map to a configured user");
        assert_eq!(mapping.github_user, "octocat");
        assert_eq!(mapping.os_user, "dev1");
    }

    #[test]
    fn rejects_local_identity_for_an_unmapped_peer_uid() {
        // Given — a config with one user; the peer uid resolves to a username in no mapping
        let yaml = "
users:
  - github_user: octocat
    os_user: dev1
";
        let config: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");

        // When
        let identity = config.local_identity_for_uid(4242, |_| Some("stranger".to_string()));

        // Then
        assert!(identity.is_none());
    }

    #[test]
    fn mints_a_login_for_a_mapped_peer_uid() {
        // Given — octocat maps to os_user dev1; peer uid 1002 resolves to "dev1"
        let yaml = "
users:
  - github_user: octocat
    os_user: dev1
";
        let config: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");

        // When
        let login =
            config.local_token_login_for_uid(1002, |uid| (uid == 1002).then(|| "dev1".to_string()));

        // Then
        assert_eq!(login.as_deref(), Some("octocat"));
    }

    #[test]
    fn no_login_for_an_unmapped_peer_uid() {
        // Given
        let yaml = "
users:
  - github_user: octocat
    os_user: dev1
";
        let config: DaemonConfig = serde_yaml::from_str(yaml).expect("parse");

        // When — the peer uid resolves to a username in no mapping
        let login = config.local_token_login_for_uid(4242, |_| Some("stranger".to_string()));

        // Then
        assert!(login.is_none());
    }
}
