use super::DaemonConfig;

use std::path::{Path, PathBuf};

fn default_claude_cli_binary_path() -> String {
    "claude".to_string()
}

fn default_claude_cli_fullscreen() -> bool {
    true
}

/// How long an orchestrated claude-cli session's `PermissionRequest` hook waits for its
/// orchestrator's answer, in seconds, when `claude_cli.answer_wait_secs` is not set
/// (`#home-inbox` 6/8).
pub const DEFAULT_ANSWER_WAIT_SECS: u64 = 120;

fn default_claude_cli_answer_wait_secs() -> u64 {
    DEFAULT_ANSWER_WAIT_SECS
}

fn default_cursor_cli_binary_path() -> String {
    "agent".to_string()
}

/// Environment variable that overrides the resolved `claude` binary path (highest priority).
/// The escape hatch for when host auto-resolution picks the wrong binary.
pub const CLAUDE_BINARY_ENV: &str = "TDDY_CLAUDE_BINARY";

/// Environment variable Claude Code reads to force its fullscreen renderer — the only renderer that
/// enables the alternate screen and mouse tracking, so the only one the web terminal can scroll.
pub const CLAUDE_FULLSCREEN_ENV: &str = "CLAUDE_CODE_NO_FLICKER";

/// A wrapper-shim `bin` directory whose `claude` merely re-execs the real binary from `$PATH`
/// (e.g. Superset's `~/.superset/bin`). Such a shim can't resolve inside the jail — its `PATH` is
/// only `/usr/bin:/bin` — so host resolution must skip these dirs. See the `claude-sandbox` script.
pub(crate) fn is_wrapper_shim_dir(dir: &Path) -> bool {
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
    find_real_harness_on_host("claude")
}

/// Whether the `claude` binary is taken from an explicit override or must be auto-resolved.
pub(crate) enum ClaudeBinaryChoice {
    /// Use this value as-is (an env override or an explicit config path).
    Explicit(String),
    /// No explicit path given — resolve the real `claude` from the host.
    Auto,
}

/// Decide from the env override and configured value alone (no filesystem access, so it is
/// unit-testable): a non-empty env override wins; otherwise a configured value naming a path
/// (contains `/`) is explicit; a bare name means "auto-resolve".
pub(crate) fn choose_claude_binary(
    env_override: Option<&str>,
    configured: &str,
) -> ClaudeBinaryChoice {
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

/// A configured harness binary taken as named when it is already a path (contains `/`); a bare
/// name resolved on the host through `find`, falling back to the name itself when nothing is found
/// (the spawner's error then names it). Pure apart from `find`, so the precedence is unit-testable.
pub fn resolve_harness_binary_name(
    configured: String,
    find: impl Fn(&str) -> Option<PathBuf>,
) -> String {
    if configured.contains('/') {
        return configured;
    }
    find(&configured)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or(configured)
}

/// Claude Code CLI session configuration. Loaded from daemon YAML under `claude_cli:`.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tddy_tools_path: Option<String>,
    /// HTTP base URL the per-worktree hook command uses to call `ReportSessionStatus`.
    /// When absent, defaults to `http://127.0.0.1:{web_port}` derived from the listen config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_url: Option<String>,
    /// Persistent jail `$HOME` reused across sandboxed claude-cli sessions so auth (refreshed OAuth
    /// tokens), session history, and settings survive between sessions. Absent → default
    /// `$HOME/.tddy/sandbox-claude-home`. Override with the `TDDY_SANDBOX_CLAUDE_HOME` env var. A
    /// single daemon-wide home — see [`resolve_claude_home_dir`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_home_dir: Option<PathBuf>,
    /// Force Claude's fullscreen renderer on every claude-cli launch (default `true`) — see
    /// [`DaemonConfig::claude_cli_renderer_env`]. `false` leaves the renderer to Claude.
    #[serde(default = "default_claude_cli_fullscreen")]
    pub fullscreen: bool,
    /// How long an orchestrated session's `PermissionRequest` hook waits for its orchestrator's
    /// answer before Claude shows its own prompt, in seconds (default
    /// [`DEFAULT_ANSWER_WAIT_SECS`]). Claude's own timeout for that hook is this plus ten
    /// (`#home-inbox` 6/8).
    #[serde(default = "default_claude_cli_answer_wait_secs")]
    pub answer_wait_secs: u64,
}

/// One further CLI harness's configuration, under `cli_harnesses.<session type>:`.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct CliHarnessConfig {
    /// The binary to run; the harness's default (`opencode`, `muse`) when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<String>,
}

impl DaemonConfig {
    /// `claude_cli.answer_wait_secs`, or [`DEFAULT_ANSWER_WAIT_SECS`] when the `claude_cli:` section
    /// is absent (`#home-inbox` 6/8).
    #[must_use]
    pub fn claude_cli_answer_wait_secs(&self) -> u64 {
        // TODO(permission-answer): implement — read `claude_cli.answer_wait_secs`.
        DEFAULT_ANSWER_WAIT_SECS
    }

    /// The environment every claude-cli launch adds so Claude runs the renderer `claude_cli.fullscreen`
    /// asks for: [`CLAUDE_FULLSCREEN_ENV`] when on — also when the `claude_cli:` section is absent —
    /// and nothing when off. Forcing it also overrides Claude's own fall back to the classic renderer
    /// after a failed fullscreen launch.
    #[must_use]
    pub fn claude_cli_renderer_env(&self) -> Vec<(String, String)> {
        let fullscreen = self
            .claude_cli
            .as_ref()
            .map_or_else(default_claude_cli_fullscreen, |c| c.fullscreen);
        if fullscreen {
            vec![(CLAUDE_FULLSCREEN_ENV.to_string(), "1".to_string())]
        } else {
            Vec::new()
        }
    }

    /// The binary a CLI harness runs: `binary_env` when set and non-empty, else
    /// `cli_harnesses.<session_type>.binary_path`, else `default_binary`.
    #[must_use]
    pub fn cli_harness_binary(
        &self,
        session_type: &str,
        default_binary: &str,
        binary_env: &str,
    ) -> String {
        // Blank values count as unset, as they do for `TDDY_CLAUDE_BINARY` and `TDDY_CURSOR_AGENT`.
        let from_env = std::env::var(binary_env).ok();
        let from_env = from_env
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let from_config = self
            .cli_harnesses
            .get(session_type)
            .and_then(|harness| harness.binary_path.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        from_env
            .or(from_config)
            .unwrap_or(default_binary)
            .to_string()
    }
}

/// Cursor Agent CLI session configuration. Loaded from daemon YAML under `cursor_cli:`.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct CursorCliConfig {
    /// Path to the Cursor Agent CLI binary (default `"agent"`).
    #[serde(default = "default_cursor_cli_binary_path")]
    pub binary_path: String,
    /// Absolute path to `tddy-tools` for per-worktree hook commands. Falls back to claude_cli or
    /// sibling-of-daemon resolution when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tddy_tools_path: Option<String>,
    /// HTTP base URL for `ReportSessionStatus`. Falls back to claude_cli or web_port default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_url: Option<String>,
    /// Persistent jail `$HOME` for sandboxed cursor-cli sessions (default: `$HOME/.tddy/sandbox-cursor-home`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_home_dir: Option<PathBuf>,
}

/// Environment variable that overrides the resolved cursor `agent` binary path (highest priority).
pub const CURSOR_BINARY_ENV: &str = "TDDY_CURSOR_AGENT";

/// Locate a harness binary as an absolute path, injectable: prefer `<home>/.local/bin/<name>` (the
/// common user-local install), then scan `path_dirs` skipping wrapper-shim dirs. `None` if none is
/// found. The home and search dirs are parameters (not read from env) so the resolution logic is
/// unit-testable. `find_real_claude_on_host` and the cursor resolver delegate to this with their
/// own binary names.
pub(crate) fn find_real_harness_in(
    name: &str,
    home: Option<&Path>,
    path_dirs: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(home) = home {
        let candidate = home.join(".local/bin").join(name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    path_dirs
        .iter()
        .filter(|d| !d.as_os_str().is_empty() && !is_wrapper_shim_dir(d))
        .map(|d| d.join(name))
        .find(|c| is_executable_file(c))
}

/// Locate a registered CLI harness's binary (e.g. `muse`, `opencode`) on the host as an absolute
/// path: prefer `~/.local/bin/<name>`, then scan `$PATH` skipping wrapper-shim dirs. `None` if none
/// is found. This is the resolution a bare harness name needs on a systemd daemon, whose own `PATH`
/// is minimal and never holds the user-local install dirs.
pub fn find_real_harness_on_host(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    find_real_harness_in(name, home.as_deref(), &path_dirs)
}

/// Whether the cursor `agent` binary is taken from an explicit override or must be auto-resolved.
/// Mirrors [`ClaudeBinaryChoice`].
pub(crate) enum CursorBinaryChoice {
    /// Use this value as-is (an env override or an explicit config path).
    Explicit(String),
    /// No explicit path given — resolve the real `agent` from the host.
    Auto,
}

/// Decide from the env override and configured value alone (no filesystem access, so it is
/// unit-testable): a non-empty env override wins; otherwise a configured value naming a path
/// (contains `/`) is explicit; a bare name means "auto-resolve". Mirrors [`choose_claude_binary`].
pub(crate) fn choose_cursor_binary(
    env_override: Option<&str>,
    configured: &str,
) -> CursorBinaryChoice {
    if let Some(env) = env_override.map(str::trim).filter(|s| !s.is_empty()) {
        return CursorBinaryChoice::Explicit(env.to_string());
    }
    if configured.contains('/') {
        return CursorBinaryChoice::Explicit(configured.to_string());
    }
    CursorBinaryChoice::Auto
}

/// Resolve the Cursor Agent CLI binary the runner / model probe will exec, as an absolute path when
/// possible. Mirrors [`resolve_claude_binary_path`] so the impersonated child never needs a PATH
/// lookup (which would fail "binary not found: agent" once the install dir is off the child's PATH).
///
/// Resolution order (first match wins):
/// 1. `TDDY_CURSOR_AGENT` env var, when set and non-empty ([`CURSOR_BINARY_ENV`]).
/// 2. `cursor_cli.binary_path` in the daemon config, when it names a path (contains `/`).
/// 3. Auto-resolution of the real host `agent`: `~/.local/bin/agent`, then `$PATH` (skipping
///    wrapper-shim dirs).
/// 4. Fallback: the configured value as-is (bare name, default `"agent"`).
pub fn resolve_cursor_binary_path(config: &DaemonConfig) -> String {
    let configured = config
        .cursor_cli
        .as_ref()
        .map(|c| c.binary_path.as_str())
        .unwrap_or("agent");
    let env_override = std::env::var(CURSOR_BINARY_ENV).ok();
    match choose_cursor_binary(env_override.as_deref(), configured) {
        CursorBinaryChoice::Explicit(p) => canonicalize_if_path(&p),
        CursorBinaryChoice::Auto => match find_real_harness_on_host("agent") {
            Some(real) => canonicalize_if_path(&real.to_string_lossy()),
            None => configured.to_string(),
        },
    }
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
