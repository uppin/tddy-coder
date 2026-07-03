//! Process spawner — fork + setuid/setgid to run tddy-* as target OS user.

use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use uuid::Uuid;

use tddy_core::{default_log_config, resolve_logger, LogConfig};

use crate::config::DaemonConfig;
use crate::tddy_user_config;

/// Same default line format as `tddy_core` and typical `dev.desktop.yaml` `log.loggers.*.format`.
pub const CHILD_LOG_FORMAT_FALLBACK: &str = "{timestamp} [{level}] [{target}] {message}";

fn level_filter_to_yaml(level: log::LevelFilter) -> &'static str {
    match level {
        log::LevelFilter::Off => "off",
        log::LevelFilter::Error => "error",
        log::LevelFilter::Warn => "warn",
        log::LevelFilter::Info => "info",
        log::LevelFilter::Debug => "debug",
        log::LevelFilter::Trace => "trace",
    }
}

/// Resolve `tool_path` (from `dev.daemon.yaml`'s `allowed_tools`, e.g. `"target/debug/tddy-coder"`)
/// to an absolute path anchored to the **daemon's own toolchain root** — never to the target
/// session's `repo_path`.
///
/// The daemon and the `tddy-coder` it spawns are built from the same checkout/workspace (the
/// same `cargo build`, the same `target/` directory) — which *project* a session happens to
/// operate on must not change which binary gets executed. A `repo_path` is just an arbitrary
/// target codebase (it could be a TODO app); it is not expected to contain a `tddy-coder` build
/// of its own, and resolving `tool_path` against it was the bug (a session against
/// `main_repo_path`s that don't coincidentally contain a `tddy-coder` build would spawn the
/// wrong binary — or nothing at all).
///
/// An already-absolute `tool_path` is returned unchanged (an operator override).
fn resolve_tool_path(tool_path: &str, daemon_toolchain_root: &Path) -> PathBuf {
    resolve_relative_to_daemon_toolchain_root(Path::new(tool_path), daemon_toolchain_root)
}

/// Resolve `tddy_data_dir` (already-parsed `DaemonConfig::tddy_data_dir` / `ConnectionService::tddy_data_dir`,
/// e.g. `tddy_core::output::default_tddy_data_dir()`'s relative debug-build default `"tmp/.tddy"`)
/// to an absolute path anchored to the **daemon's own toolchain root** — never to the target
/// session's `repo_path`.
///
/// Same rationale as [`resolve_tool_path`]: the daemon and the `tddy-coder` child it spawns must
/// agree on where session state (`changeset.yaml`, `.session.yaml`, etc.) lives. The child's own
/// cwd is set to `repo_path` (an arbitrary target codebase), so if the child were left to
/// independently re-derive a relative `tddy_data_dir` default against its own cwd, it would write
/// session state into `repo_path`'s tree instead of the daemon's — and the daemon's `ListSessions`
/// (which scans relative to its own launch dir) would never find it. Passing the daemon's already-
/// resolved value explicitly (via `--tddy-data-dir`) removes the guesswork.
///
/// An already-absolute `tddy_data_dir` is returned unchanged (an operator override).
fn resolve_tddy_data_dir(tddy_data_dir: &Path, daemon_toolchain_root: &Path) -> PathBuf {
    resolve_relative_to_daemon_toolchain_root(tddy_data_dir, daemon_toolchain_root)
}

/// Shared absolute/relative resolution rule for [`resolve_tool_path`] and [`resolve_tddy_data_dir`]:
/// an absolute `path` is returned unchanged (an operator override); a relative `path` is joined
/// onto `daemon_toolchain_root` (the daemon's own process cwd — never the target session's
/// `repo_path`).
fn resolve_relative_to_daemon_toolchain_root(path: &Path, daemon_toolchain_root: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        daemon_toolchain_root.join(path)
    }
}

/// Last `n` non-empty lines of `s`, joined by `\n`. Used to fold a crashed child's stderr into an
/// RPC error message without dumping an arbitrarily long stack trace.
fn tail_lines(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// Escape `s` for use as a YAML double-quoted scalar (child session `log.loggers.default.format`).
fn yaml_double_quote_scalar(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Level + format string for the per-session `tddy-coder` `--config` snippet, matching the daemon's
/// effective `log:` (e.g. from `dev.desktop.yaml`). When the daemon omits `log:`, matches
/// `main.rs` (`default_log_config` / `RUST_LOG`).
pub fn child_log_yaml_tuning(daemon_log: Option<&LogConfig>) -> (String, String) {
    let cfg = daemon_log
        .cloned()
        .unwrap_or_else(|| default_log_config(None, None));
    let level = level_filter_to_yaml(cfg.default.level).to_string();
    let format = resolve_logger(&cfg, None, &cfg.default.logger)
        .and_then(|logger| logger.format.clone())
        .unwrap_or_else(|| CHILD_LOG_FORMAT_FALLBACK.to_string());
    (level, format)
}

/// Extract the `log:` block from a `tddy-coder` config file (e.g. `dev.config.yaml`, referenced by
/// the daemon's `coder_config_path`) and re-emit it as a standalone `--config` document for spawned
/// children. This lets operators own the child's full log routing — loggers, levels, and target
/// policies (e.g. sending `libwebrtc*` / `livekit*` chatter to a separate file at INFO) — from an
/// editable config file rather than daemon-synthesized defaults.
///
/// Returns `None` when no path is configured, the file can't be read, or it has no `log:` section;
/// callers then fall back to [`child_log_yaml_tuning`]-based synthesis. The block is passed through
/// verbatim (as `serde_yaml::Value`) so it doesn't depend on `LogConfig` implementing `Serialize`.
/// File paths inside it resolve against the child's cwd (the session repo root).
pub fn coder_log_config_yaml(coder_config_path: Option<&Path>) -> Option<String> {
    let path = coder_config_path?;
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            log::warn!(
                "coder_config_path {} set but unreadable ({}); falling back to synthesized child log config",
                path.display(),
                e
            );
            return None;
        }
    };
    let doc: serde_yaml::Value = match serde_yaml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "coder_config_path {} is not valid YAML ({}); falling back to synthesized child log config",
                path.display(),
                e
            );
            return None;
        }
    };
    let log = doc.get("log")?.clone();
    let mut map = serde_yaml::Mapping::new();
    map.insert(serde_yaml::Value::from("log"), log);
    match serde_yaml::to_string(&serde_yaml::Value::Mapping(map)) {
        Ok(s) => Some(s),
        Err(e) => {
            log::warn!(
                "failed to re-serialize log: block from {} ({}); falling back to synthesized child log config",
                path.display(),
                e
            );
            None
        }
    }
}

#[cfg(test)]
mod coder_log_config_yaml_tests {
    use super::coder_log_config_yaml;
    use std::io::Write;

    fn write_temp(contents: &str) -> tempfile::TempPath {
        let mut f = tempfile::NamedTempFile::new().expect("temp file");
        f.write_all(contents.as_bytes()).expect("write");
        f.into_temp_path()
    }

    #[test]
    fn none_when_no_path_configured() {
        assert!(coder_log_config_yaml(None).is_none());
    }

    #[test]
    fn none_when_file_has_no_log_section() {
        let path = write_temp("daemon: true\nmouse: true\n");
        assert!(coder_log_config_yaml(Some(path.as_ref())).is_none());
    }

    #[test]
    fn none_when_path_missing() {
        let missing = std::path::Path::new("/no/such/coder-config-xyz.yaml");
        assert!(coder_log_config_yaml(Some(missing)).is_none());
    }

    #[test]
    fn extracts_only_log_block_and_parses_as_child_config() {
        // Given a full tddy-coder config (like dev.config.yaml) with unrelated keys plus a log block
        // that routes libwebrtc* to a separate webrtc logger at INFO.
        let path = write_temp(
            r#"daemon: true
mouse: true
github:
  stub: true
log:
  loggers:
    default:
      output: { file: "tmp/logs/coder" }
    webrtc:
      output: { file: "tmp/logs/coder-webrtc" }
  default:
    level: debug
    logger: default
  policies:
    - selector: { target: "libwebrtc*" }
      level: info
      logger: webrtc
"#,
        );

        // When
        let yaml = coder_log_config_yaml(Some(path.as_ref())).expect("log block extracted");

        // Then — only the log block is emitted (no daemon/github keys leak into the child config)…
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid yaml");
        let map = doc.as_mapping().expect("mapping");
        assert_eq!(
            map.len(),
            1,
            "child --config must contain only the log: block"
        );
        assert!(map.contains_key(serde_yaml::Value::from("log")));

        // …and it round-trips into the real child config type with the webrtc routing intact.
        let cfg: tddy_coder::config::Config =
            serde_yaml::from_str(&yaml).expect("parses as tddy-coder Config");
        let log = cfg.log.expect("log present");
        assert!(log.loggers.contains_key("webrtc"));
        assert_eq!(log.policies.len(), 1);
        assert_eq!(log.policies[0].level, Some(log::LevelFilter::Info));
        assert_eq!(log.policies[0].logger.as_deref(), Some("webrtc"));
    }
}

/// LiveKit credentials to pass to spawned process (url, api_key, api_secret).
/// Optional `common_room` forces all daemon spawns into one LiveKit room (see [`resolve_livekit_room_name`]).
#[derive(Debug, Clone)]
pub struct LiveKitCreds {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    /// When set (non-empty after trim), spawned processes use this room instead of `daemon-{session_id}`.
    pub common_room: Option<String>,
    /// When set, LiveKit server identity is `daemon-{id}-{session_id}` (multi-host).
    pub daemon_instance_id: Option<String>,
}

/// Picks the LiveKit room name for a spawned `tddy-*` process.
///
/// When `common_room` is set (non-empty after trim), every session uses that shared room so all
/// daemon-spawned tools join the same room. Otherwise the room is `daemon-{session_id}`.
pub(crate) fn resolve_livekit_room_name(common_room: Option<&str>, session_id: &str) -> String {
    if let Some(cr) = common_room {
        let t = cr.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    format!("daemon-{}", session_id)
}

/// LiveKit **server identity** string for the spawned `tddy-coder` process (browser / terminal RPC target).
///
/// When `daemon_instance_id` is set (multi-host), the identity must incorporate it so clients can
/// route to the daemon that owns the session. Single-daemon setups use [`None`].
///
/// Expected pattern when `Some(instance_id)` is set: `daemon-{instance_id}-{session_id}`.
pub fn livekit_server_identity_for_session(
    daemon_instance_id: Option<&str>,
    session_id: &str,
) -> String {
    let instance = daemon_instance_id.map(str::trim).filter(|s| !s.is_empty());
    log::debug!(
        "livekit_server_identity_for_session: instance={:?} session_id={}",
        instance,
        session_id
    );
    match instance {
        Some(id) => format!("daemon-{}-{}", id, session_id),
        None => format!("daemon-{}", session_id),
    }
}

/// Paths and open files for child `--config`, stderr, stdout, and the shared app log target.
///
/// Child needs a real stderr so crossterm/terminal APIs work; Stdio::null() can cause SIGSEGV.
/// Stdout is also captured to a file so early panics / print output are visible when debugging hangs.
///
/// Appends to `tmp/logs/coder` (same path as `dev.config.yaml`’s default file logger) so
/// backend invoke lines (e.g. Cursor CLI) appear alongside `tddy_coder::run` startup logs.
/// `rotation.max_rotated: 0` avoids renaming the shared file on each session start.
#[cfg(unix)]
struct ChildProcessLogFiles {
    config_path: PathBuf,
    stderr_path: PathBuf,
    stdout_path: PathBuf,
    app_log_path: PathBuf,
    stderr: File,
    stdout: File,
}

#[cfg(unix)]
fn create_child_log_config_and_streams(
    repo_path: &Path,
    session_id: &str,
    child_log_level: &str,
    child_log_format: &str,
    coder_log_config_yaml: Option<&str>,
) -> anyhow::Result<ChildProcessLogFiles> {
    let child_logs_dir = repo_path.join("tmp").join("logs").join("child");
    std::fs::create_dir_all(&child_logs_dir).map_err(|e| {
        anyhow::anyhow!(
            "failed to create child logs dir {}: {}",
            child_logs_dir.display(),
            e
        )
    })?;

    let log_file = repo_path.join("tmp").join("logs").join("coder");
    if let Some(parent) = log_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("failed to create log dir {}: {}", parent.display(), e))?;
    }
    // Absolute path avoids duplicate FILE_OUTPUTS keys and matches the repo the child uses as cwd.
    let log_file_abs = log_file.canonicalize().unwrap_or_else(|_| log_file.clone());

    let config_path = child_logs_dir.join(format!("{}.yaml", session_id));

    // When the daemon supplies a coder config's `log:` block (via `coder_config_path`), it fully
    // owns the child's log routing — write it verbatim. Otherwise synthesize a minimal `default`
    // logger from the daemon's derived level/format.
    let yaml = if let Some(coder_log_yaml) = coder_log_config_yaml {
        coder_log_yaml.to_string()
    } else {
        let format_quoted = yaml_double_quote_scalar(child_log_format);
        format!(
            r#"log:
  loggers:
    default:
      output: {{ file: "{}" }}
      format: {}
  default:
    level: {}
    logger: default
  rotation:
    max_rotated: 0
"#,
            log_file_abs.display(),
            format_quoted,
            child_log_level
        )
    };

    std::fs::write(&config_path, yaml).map_err(|e| {
        anyhow::anyhow!(
            "failed to write child config {}: {}",
            config_path.display(),
            e
        )
    })?;

    let stderr_path = child_logs_dir.join(format!("{}_stderr", session_id));
    let stderr_file = File::create(&stderr_path).map_err(|e| {
        anyhow::anyhow!(
            "failed to create child stderr {}: {}",
            stderr_path.display(),
            e
        )
    })?;

    let stdout_path = child_logs_dir.join(format!("{}_stdout", session_id));
    let stdout_file = File::create(&stdout_path).map_err(|e| {
        anyhow::anyhow!(
            "failed to create child stdout {}: {}",
            stdout_path.display(),
            e
        )
    })?;

    Ok(ChildProcessLogFiles {
        config_path,
        stderr_path,
        stdout_path,
        app_log_path: log_file_abs,
        stderr: stderr_file,
        stdout: stdout_file,
    })
}

/// Optional flags for [`spawn_as_user`] (session identity, agent, mouse).
#[derive(Debug, Clone, Copy, Default)]
pub struct SpawnOptions<'a> {
    pub resume_session_id: Option<&'a str>,
    /// When starting a new session (no `resume_session_id`), use this id instead of generating one
    /// (e.g. Telegram pre-created `~/.tddy/sessions/<id>`).
    pub new_session_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub agent: Option<&'a str>,
    pub mouse: bool,
    /// Passed to spawned `tddy-coder` as `--recipe` when non-empty (e.g. `bugfix`).
    pub recipe: Option<&'a str>,
    /// Back-reference to the orchestrating PR-stack session. Passed as `--stack-parent <id>`.
    pub stack_parent: Option<&'a str>,
}

/// Merge the daemon process `PATH` with an optional prefix (from the target user's `~/.tddy/config.yaml`).
pub fn merge_spawn_child_path(path_extra: Option<&str>) -> String {
    const FALLBACK: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
    let base = std::env::var("PATH").unwrap_or_else(|_| FALLBACK.to_string());
    let Some(extra) = path_extra.map(str::trim).filter(|s| !s.is_empty()) else {
        return base;
    };
    format!("{}:{}", extra.trim_end_matches(':'), base)
}

/// Default gRPC port when `tddy-coder` omits `--grpc`; probe uses the same bind shape as the child.
const DEFAULT_TDDY_CODER_GRPC_PORT: u16 = 50051;
/// How many successive ports to try after [`DEFAULT_TDDY_CODER_GRPC_PORT`] when the default is busy.
const GRPC_LISTEN_PORT_SEARCH_LEN: u32 = 4096;

/// Probes `0.0.0.0:{port}` (same as `tddy-coder` daemon bind). `Ok(())` means the port was free
/// at probe time; `Err` with [`ErrorKind::AddrInUse`] means another listener holds it.
pub fn verify_tcp_listen_port_free(port: u16) -> std::io::Result<()> {
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = std::net::TcpListener::bind(addr)?;
    drop(listener);
    Ok(())
}

fn allocate_verified_grpc_listen_port() -> std::io::Result<u16> {
    for i in 0..GRPC_LISTEN_PORT_SEARCH_LEN {
        let p = u32::from(DEFAULT_TDDY_CODER_GRPC_PORT).saturating_add(i);
        if p > u32::from(u16::MAX) {
            break;
        }
        let port = p as u16;
        match verify_tcp_listen_port_free(port) {
            Ok(()) => return Ok(port),
            Err(e) if e.kind() == ErrorKind::AddrInUse => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        ErrorKind::AddrNotAvailable,
        "no free TCP port for tddy-coder gRPC in search range",
    ))
}

/// Result of spawning a session.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SpawnResult {
    pub session_id: String,
    pub livekit_room: String,
    pub livekit_server_identity: String,
    pub livekit_url: String,
    pub pid: u32,
    /// gRPC listen port for the spawned `tddy-coder --daemon` child (localhost observer / TddyRemote).
    pub grpc_port: u16,
}

/// Clone a git repository as the given OS user. If `destination` already exists, skips clone.
#[cfg(unix)]
pub fn clone_as_user(os_user: &str, git_url: &str, destination: &Path) -> anyhow::Result<()> {
    use std::os::unix::process::CommandExt;

    let dest_str = destination
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("destination path is not valid UTF-8"))?;

    // Match shell `if test -e "$1"; then exit 0; fi` without fork/pre_exec — avoids NSS hangs
    // in setuid/initgroups when the tree is already present.
    if destination.exists() {
        log::info!(
            "clone_as_user: destination already exists, skipping fork (dest={})",
            destination.display()
        );
        return Ok(());
    }

    log::info!(
        "clone_as_user: will run git clone as os_user={} dest={}",
        os_user,
        destination.display()
    );

    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    let ret = unsafe {
        libc::getpwnam_r(
            std::ffi::CString::new(os_user)
                .map_err(|e| anyhow::anyhow!("invalid username: {}", e))?
                .as_ptr(),
            passwd.as_mut_ptr(),
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if ret != 0 || result.is_null() {
        anyhow::bail!("user '{}' not found", os_user);
    }
    let passwd = unsafe { &*result };
    let uid = passwd.pw_uid;
    let gid = passwd.pw_gid;
    if passwd.pw_dir.is_null() {
        anyhow::bail!("user '{}' has no home directory", os_user);
    }
    if passwd.pw_name.is_null() {
        anyhow::bail!("user '{}' has no passwd pw_name", os_user);
    }
    // initgroups(3) requires a non-NULL user; NULL yields EINVAL on Linux and is not POSIX.
    let pw_name = unsafe { std::ffi::CStr::from_ptr(passwd.pw_name).to_owned() };

    let home_dir = unsafe { std::ffi::CStr::from_ptr(passwd.pw_dir) }
        .to_string_lossy()
        .into_owned();

    let same_user = uid == unsafe { libc::getuid() } && gid == unsafe { libc::getgid() };

    // Run as target user: skip clone if destination exists; else `git clone`.
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c")
        .arg(r#"if test -e "$1"; then exit 0; fi; exec git clone "$2" "$1""#)
        .arg("sh")
        .arg(dest_str)
        .arg(git_url)
        .env("HOME", &home_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if !same_user {
        let home_dir_pre = home_dir.clone();
        unsafe {
            cmd.pre_exec(move || {
                std::env::set_var("HOME", &home_dir_pre);
                if libc::setgid(gid) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::initgroups(pw_name.as_ptr(), gid as _) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::setuid(uid) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    log::debug!(
        "clone_as_user: invoking sh/git same_user={} dest={}",
        same_user,
        destination.display()
    );

    let output = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("git clone: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone failed: {}", stderr.trim());
    }

    Ok(())
}

#[cfg(not(unix))]
pub fn clone_as_user(_os_user: &str, _git_url: &str, _destination: &Path) -> anyhow::Result<()> {
    anyhow::bail!("clone_as_user is only supported on Unix")
}

/// Spawn a tddy-* process as the given OS user.
#[cfg(unix)]
#[allow(clippy::too_many_arguments)] // os_user/tool_path/tddy_data_dir/repo_path/livekit/opts/log level+format; a struct would obscure call sites for a spawn-time argument bundle.
pub fn spawn_as_user(
    os_user: &str,
    tool_path: &str,
    tddy_data_dir: &Path,
    repo_path: &Path,
    livekit: &LiveKitCreds,
    opts: SpawnOptions<'_>,
    child_log_level: &str,
    child_log_format: &str,
    coder_log_config_yaml: Option<&str>,
) -> anyhow::Result<SpawnResult> {
    use std::os::unix::process::CommandExt;

    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    let ret = unsafe {
        libc::getpwnam_r(
            std::ffi::CString::new(os_user)
                .map_err(|e| anyhow::anyhow!("invalid username: {}", e))?
                .as_ptr(),
            passwd.as_mut_ptr(),
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if ret != 0 || result.is_null() {
        anyhow::bail!("user '{}' not found", os_user);
    }
    let passwd = unsafe { &*result };
    let uid = passwd.pw_uid;
    let gid = passwd.pw_gid;
    if passwd.pw_dir.is_null() {
        anyhow::bail!("user '{}' has no home directory", os_user);
    }
    if passwd.pw_name.is_null() {
        anyhow::bail!("user '{}' has no passwd pw_name", os_user);
    }
    let pw_name = unsafe { std::ffi::CStr::from_ptr(passwd.pw_name).to_owned() };

    if opts.resume_session_id.is_some() && opts.new_session_id.is_some() {
        anyhow::bail!("resume_session_id and new_session_id are mutually exclusive");
    }
    let session_id = opts
        .resume_session_id
        .map(String::from)
        .or_else(|| opts.new_session_id.map(String::from))
        .unwrap_or_else(|| Uuid::now_v7().to_string());
    let livekit_room = resolve_livekit_room_name(livekit.common_room.as_deref(), &session_id);
    let instance = livekit
        .daemon_instance_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let identity = livekit_server_identity_for_session(instance, &session_id);

    let home_dir = unsafe { std::ffi::CStr::from_ptr(passwd.pw_dir) }
        .to_string_lossy()
        .into_owned();

    let same_user = uid == unsafe { libc::getuid() } && gid == unsafe { libc::getgid() };

    let logs = create_child_log_config_and_streams(
        repo_path,
        &session_id,
        child_log_level,
        child_log_format,
        coder_log_config_yaml,
    )?;

    let user_cfg = Path::new(&home_dir).join(".tddy").join("config.yaml");
    let path_extra = tddy_user_config::spawn_path_extra_for_home(Path::new(&home_dir));
    let child_path = merge_spawn_child_path(path_extra.as_deref());
    if let Some(ref extra) = path_extra {
        log::info!(
            "spawner: child PATH prepends spawn_path_extra from {}: {}",
            user_cfg.display(),
            extra
        );
    }

    let grpc_port = allocate_verified_grpc_listen_port()
        .map_err(|e| anyhow::anyhow!("allocate gRPC listen port for tddy-coder: {}", e))?;

    // Anchor a relative tool_path to the daemon's own toolchain root (its own process cwd —
    // nothing in this crate ever calls set_current_dir, so this reliably reflects wherever
    // ./web-dev / cargo run -p tddy-daemon was launched from), not to repo_path. repo_path is an
    // arbitrary target codebase for the session to operate on; it is not expected to contain a
    // tddy-coder build of its own.
    let daemon_toolchain_root = std::env::current_dir()
        .map_err(|e| anyhow::anyhow!("resolve daemon's own toolchain root (current_dir): {}", e))?;
    let resolved_tool_path = resolve_tool_path(tool_path, &daemon_toolchain_root);
    let resolved_tddy_data_dir = resolve_tddy_data_dir(tddy_data_dir, &daemon_toolchain_root);

    let mut cmd = std::process::Command::new(&resolved_tool_path);
    cmd.current_dir(repo_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(logs.stdout))
        .stderr(Stdio::from(logs.stderr))
        .env("HOME", &home_dir)
        .env("PATH", &child_path)
        .arg("--daemon")
        .arg("--grpc")
        .arg(grpc_port.to_string())
        .arg("--livekit-url")
        .arg(&livekit.url)
        .arg("--livekit-api-key")
        .arg(&livekit.api_key)
        .arg("--livekit-api-secret")
        .arg(&livekit.api_secret)
        .arg("--livekit-room")
        .arg(&livekit_room)
        .arg("--livekit-identity")
        .arg(&identity)
        .arg("--tddy-data-dir")
        .arg(&resolved_tddy_data_dir);

    if let Some(resume_id) = opts.resume_session_id {
        cmd.arg("--resume-from").arg(resume_id);
    } else {
        cmd.arg("--session-id").arg(&session_id);
    }

    if let Some(pid) = opts.project_id {
        if !pid.is_empty() {
            cmd.arg("--project-id").arg(pid);
        }
    }

    if let Some(a) = opts.agent {
        let a = a.trim();
        if !a.is_empty() {
            cmd.arg("--agent").arg(a);
        }
    }

    if opts.mouse {
        cmd.arg("--mouse");
    }

    if let Some(r) = opts.recipe {
        let r = r.trim();
        if !r.is_empty() {
            log::debug!("spawner: passing --recipe {}", r);
            cmd.arg("--recipe").arg(r);
        }
    }

    if let Some(sp) = opts.stack_parent {
        let sp = sp.trim();
        if !sp.is_empty() {
            log::debug!("spawner: passing --stack-parent {}", sp);
            cmd.arg("--stack-parent").arg(sp);
        }
    }

    cmd.arg("--config").arg(&logs.config_path);

    let cfg_abs = logs
        .config_path
        .canonicalize()
        .unwrap_or_else(|_| logs.config_path.clone());
    let err_abs = logs
        .stderr_path
        .canonicalize()
        .unwrap_or_else(|_| logs.stderr_path.clone());
    let out_abs = logs
        .stdout_path
        .canonicalize()
        .unwrap_or_else(|_| logs.stdout_path.clone());
    let app_abs = logs
        .app_log_path
        .canonicalize()
        .unwrap_or_else(|_| logs.app_log_path.clone());

    log::info!(
        "spawning process os_user={} tool={} repo={} session_id={} grpc_port={} livekit_room={} livekit_identity={} livekit_url={}",
        os_user,
        resolved_tool_path.display(),
        repo_path.display(),
        session_id,
        grpc_port,
        livekit_room,
        identity,
        livekit.url
    );
    log::info!(
        "spawner: child I/O same_user={} stderr={} stdout={} daemon_config={} app_file_log={}",
        same_user,
        err_abs.display(),
        out_abs.display(),
        cfg_abs.display(),
        app_abs.display()
    );
    if !same_user {
        log::info!(
            "spawner: cmd.spawn() blocks until child finishes pre_exec (setgid/initgroups/setuid) and exec; if this hangs, check NSS/LDAP for os_user={} (often stuck in initgroups)",
            os_user
        );
    }

    log::debug!("spawner: about to cmd.spawn() session_id={}", session_id);

    // When spawning as same user, skip pre_exec — avoids fork() which can deadlock in some envs.
    // pre_exec forces the slow fork path; plain spawn may use posix_spawn.
    if !same_user {
        let home_dir_pre = home_dir.clone();
        unsafe {
            cmd.pre_exec(move || {
                std::env::set_var("HOME", &home_dir_pre);
                if libc::setgid(gid) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::initgroups(pw_name.as_ptr(), gid as _) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::setuid(uid) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    log::info!(
        "[spawn] child cmd: {} {}",
        cmd.get_program().to_string_lossy(),
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut child = cmd.spawn()?;
    let pid = child.id();

    log::info!(
        "spawn ok session_id={} pid={} livekit_room={} livekit_server_identity={}",
        session_id,
        pid,
        livekit_room,
        identity
    );

    // Grace period: `cmd.spawn()` only confirms fork+exec succeeded, not that the process is
    // actually alive and serving. A bad CLI arg (e.g. an unknown --recipe) or an early panic
    // exits within milliseconds — without this check that would still return a "successful"
    // SpawnResult with a session_id that never becomes a real session. Poll non-blockingly
    // (`try_wait`, not `wait`) so a healthy long-running child is never delayed beyond this
    // fixed window.
    const STARTUP_GRACE_PERIOD: Duration = Duration::from_millis(500);
    const STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(25);
    let mut waited = Duration::ZERO;
    let early_exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if waited >= STARTUP_GRACE_PERIOD => break None,
            Ok(None) => {
                std::thread::sleep(STARTUP_POLL_INTERVAL);
                waited += STARTUP_POLL_INTERVAL;
            }
            Err(e) => {
                log::warn!(
                    "spawner: try_wait failed during startup grace period session_id={} pid={} err={}",
                    session_id,
                    pid,
                    e
                );
                break None;
            }
        }
    };

    if let Some(status) = early_exit {
        let stderr_tail = std::fs::read_to_string(&logs.stderr_path)
            .map(|s| tail_lines(&s, 20))
            .unwrap_or_default();
        log::warn!(
            "spawner: child exited during startup grace period session_id={} pid={} status={} stderr={}",
            session_id,
            pid,
            status,
            err_abs.display()
        );
        return Err(anyhow::anyhow!(
            "tddy-coder exited immediately after starting ({status}){}",
            if stderr_tail.is_empty() {
                String::new()
            } else {
                format!(": {stderr_tail}")
            }
        ));
    }

    let session_id_exit = session_id.clone();
    std::thread::spawn(move || match child.wait() {
        Ok(status) => log::info!(
            "child exited session_id={} pid={} status={}",
            session_id_exit,
            pid,
            status
        ),
        Err(e) => log::warn!(
            "child wait failed session_id={} pid={} err={}",
            session_id_exit,
            pid,
            e
        ),
    });

    Ok(SpawnResult {
        session_id: session_id.clone(),
        livekit_room,
        livekit_server_identity: identity,
        livekit_url: livekit.url.clone(),
        pid,
        grpc_port,
    })
}

#[cfg(not(unix))]
#[allow(clippy::too_many_arguments)] // Mirrors the #[cfg(unix)] spawn_as_user signature.
pub fn spawn_as_user(
    _os_user: &str,
    _tool_path: &str,
    _tddy_data_dir: &Path,
    _repo_path: &Path,
    _livekit: &LiveKitCreds,
    _opts: SpawnOptions<'_>,
    _child_log_level: &str,
    _child_log_format: &str,
    _coder_log_config_yaml: Option<&str>,
) -> anyhow::Result<SpawnResult> {
    anyhow::bail!("spawn_as_user is only supported on Unix")
}

/// True when sessions join a shared LiveKit room (`livekit.common_room`), so participant identities
/// must be disambiguated per daemon instance (see [`livekit_spawn_daemon_instance_id`]).
fn livekit_common_room_is_set(config: &DaemonConfig) -> bool {
    config
        .livekit
        .as_ref()
        .and_then(|l| l.common_room.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
}

/// Instance id segment for **`tddy-coder` LiveKit identity** (`daemon-{instance}-{session}`).
///
/// When **`livekit.common_room`** is set, every spawned coder joins that shared room; use the same
/// effective id as [`crate::livekit_peer_discovery::local_instance_id_for_config`] so identities
/// never collide with another daemon on the same host and match **ConnectSession** / **ResumeSession**.
///
/// When common room is unset, rooms are per-session (`daemon-{session_id}`) and the legacy rule
/// applies: only honor an explicit `daemon_instance_id` YAML override (no implicit hostname).
pub fn livekit_spawn_daemon_instance_id(config: &DaemonConfig) -> Option<String> {
    if livekit_common_room_is_set(config) {
        Some(crate::livekit_peer_discovery::local_instance_id_for_config(
            config,
        ))
    } else {
        config
            .daemon_instance_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }
}

/// Build LiveKitCreds from daemon config.
pub fn livekit_creds_from_config(config: &DaemonConfig) -> Option<LiveKitCreds> {
    let lk = config.livekit.as_ref()?;
    let url = lk.url.as_ref()?.clone();
    let api_key = lk.api_key.as_ref()?.clone();
    let api_secret = lk.api_secret.as_ref()?.clone();
    Some(LiveKitCreds {
        url: lk.public_url.as_ref().cloned().unwrap_or(url),
        api_key,
        api_secret,
        common_room: lk.common_room.clone(),
        daemon_instance_id: livekit_spawn_daemon_instance_id(config),
    })
}

#[cfg(test)]
mod resolve_livekit_room_name_tests {
    use super::resolve_livekit_room_name;

    #[test]
    fn when_common_room_configured_all_sessions_share_that_room_name() {
        assert_eq!(
            resolve_livekit_room_name(Some("tddy-lobby"), "session-uuid-a"),
            "tddy-lobby"
        );
        assert_eq!(
            resolve_livekit_room_name(Some("tddy-lobby"), "session-uuid-b"),
            "tddy-lobby"
        );
    }

    #[test]
    fn when_common_room_unset_uses_daemon_prefixed_session_room() {
        assert_eq!(resolve_livekit_room_name(None, "abc"), "daemon-abc");
    }

    #[test]
    fn when_common_room_unset_or_whitespace_only_uses_daemon_prefixed_session_room() {
        assert_eq!(resolve_livekit_room_name(Some(""), "abc"), "daemon-abc");
        assert_eq!(resolve_livekit_room_name(Some("   "), "abc"), "daemon-abc");
    }
}

#[cfg(test)]
mod livekit_server_identity_multi_host_tests {
    use super::livekit_server_identity_for_session;

    /// When a daemon instance is selected, identity embeds instance id and session id.
    #[test]
    fn identity_includes_daemon_instance_when_provided() {
        assert_eq!(
            livekit_server_identity_for_session(Some("west-1"), "sid-9"),
            "daemon-west-1-sid-9"
        );
    }

    #[test]
    fn identity_single_daemon_without_instance_uses_session_only() {
        assert_eq!(
            livekit_server_identity_for_session(None, "sid-9"),
            "daemon-sid-9"
        );
    }
}

#[cfg(test)]
mod livekit_spawn_instance_id_tests {
    use super::livekit_spawn_daemon_instance_id;
    use crate::config::{DaemonConfig, LiveKitConfig};

    #[test]
    fn without_common_room_only_yaml_instance_id_opt_in() {
        let base = DaemonConfig {
            livekit: Some(LiveKitConfig {
                url: Some("ws://x".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(livekit_spawn_daemon_instance_id(&base), None);
        let c = DaemonConfig {
            livekit: base.livekit.clone(),
            daemon_instance_id: Some(" west ".into()),
            ..Default::default()
        };
        assert_eq!(
            livekit_spawn_daemon_instance_id(&c).as_deref(),
            Some("west")
        );
    }

    #[test]
    fn with_common_room_uses_local_instance_id_even_without_yaml_override() {
        let c = DaemonConfig {
            livekit: Some(LiveKitConfig {
                url: Some("ws://x".into()),
                common_room: Some("lobby".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let id = livekit_spawn_daemon_instance_id(&c).expect("some");
        assert!(!id.is_empty());
    }
}

#[cfg(test)]
mod child_log_yaml_tuning_tests {
    use super::child_log_yaml_tuning;
    use tddy_core::LogConfig;

    #[test]
    fn uses_daemon_default_level_and_logger_format() {
        let yaml = r#"
loggers:
  default:
    output: stderr
    format: "[TEST] {level} {message}"
default:
  level: warn
  logger: default
"#;
        let cfg: LogConfig = serde_yaml::from_str(yaml).expect("parse");
        let (level, format) = child_log_yaml_tuning(Some(&cfg));
        assert_eq!(level, "warn");
        assert_eq!(format, "[TEST] {level} {message}");
    }

    #[test]
    fn when_daemon_log_missing_matches_default_log_config() {
        let (level_a, fmt_a) = child_log_yaml_tuning(None);
        let (level_b, fmt_b) = child_log_yaml_tuning(None);
        assert_eq!(level_a, level_b);
        assert_eq!(fmt_a, fmt_b);
        assert!(
            matches!(
                level_a.as_str(),
                "off" | "error" | "warn" | "info" | "debug" | "trace"
            ),
            "unexpected level {}",
            level_a
        );
    }
}

#[cfg(test)]
mod grpc_listen_port_tests {
    use std::io::ErrorKind;
    use std::net::TcpListener;

    use super::verify_tcp_listen_port_free;

    #[test]
    fn verify_tcp_listen_port_free_ok_after_listener_dropped() {
        let l = TcpListener::bind("0.0.0.0:0").expect("bind ephemeral");
        let port = l.local_addr().expect("addr").port();
        drop(l);
        verify_tcp_listen_port_free(port).expect("port free after drop");
    }

    #[test]
    fn verify_tcp_listen_port_free_err_addr_in_use() {
        let holder = TcpListener::bind("0.0.0.0:0").expect("bind holder");
        let port = holder.local_addr().expect("addr").port();
        let err = verify_tcp_listen_port_free(port).expect_err("second bind should fail");
        assert_eq!(err.kind(), ErrorKind::AddrInUse);
    }
}

#[cfg(test)]
mod stack_parent_spawn_tests {
    use super::SpawnOptions;

    /// `spawn_options_has_stack_parent_field` — `SpawnOptions` must expose a `stack_parent` field
    /// so that the daemon can forward a parent orchestrator session id to the spawned child
    /// `tddy-coder` process as `--stack-parent <id>`.
    ///
    /// This test will **fail to compile** until `stack_parent: Option<&'a str>` is added to
    /// `SpawnOptions`. That compile failure is the intended red-phase signal for Layer 3.
    #[test]
    fn spawn_options_has_stack_parent_field() {
        let opts = SpawnOptions {
            stack_parent: Some("orch-session-id-42"),
            ..Default::default()
        };
        assert_eq!(
            opts.stack_parent,
            Some("orch-session-id-42"),
            "SpawnOptions::stack_parent must round-trip correctly"
        );
    }
}

#[cfg(all(test, unix))]
mod startup_grace_period_tests {
    use super::{spawn_as_user, LiveKitCreds, SpawnOptions};

    fn current_username() -> String {
        std::env::var("USER").expect("USER env var must be set to run this test")
    }

    fn a_livekit_creds() -> LiveKitCreds {
        LiveKitCreds {
            url: "ws://127.0.0.1:7880".to_string(),
            api_key: "test-key".to_string(),
            api_secret: "test-secret".to_string(),
            common_room: None,
            daemon_instance_id: None,
        }
    }

    /// Returns the path to the `false` binary — `/usr/bin/false` on macOS, `/bin/false` on Linux.
    fn false_bin() -> &'static str {
        if cfg!(target_os = "macos") {
            "/usr/bin/false"
        } else {
            "/bin/false"
        }
    }

    #[test]
    fn spawning_a_binary_that_exits_immediately_returns_an_error_with_its_stderr() {
        // Given — `false` ignores every argument (including the --daemon/--grpc/... flags
        // spawn_as_user appends) and exits 1 immediately, simulating a child that crashes on
        // startup (e.g. an unknown --recipe value) rather than becoming a running session
        let tmp = tempfile::tempdir().unwrap();
        let tddy_data_dir = tempfile::tempdir().unwrap();
        let os_user = current_username();

        // When
        let result = spawn_as_user(
            &os_user,
            false_bin(),
            tddy_data_dir.path(),
            tmp.path(),
            &a_livekit_creds(),
            SpawnOptions::default(),
            "info",
            super::CHILD_LOG_FORMAT_FALLBACK,
            None,
        );

        // Then — a startup crash is reported as an error, not a fake-success SpawnResult
        let err = result.expect_err("a child that exits immediately must not report success");
        assert!(
            err.to_string().contains("exited immediately"),
            "expected the error to explain the child exited during startup, got: {err}"
        );
    }

    /// A script that ignores every argument (spawn_as_user appends `--daemon --grpc ...`) and
    /// just sleeps, standing in for a `tddy-coder` process that starts up successfully.
    fn a_script_that_outlives_the_grace_period(dir: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("fake-long-running-tddy-coder.sh");
        std::fs::write(&path, "#!/bin/sh\nsleep 2\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn spawning_a_process_that_outlives_the_grace_period_returns_ok() {
        // Given — a process that stays alive well past the startup grace period
        let tmp = tempfile::tempdir().unwrap();
        let tddy_data_dir = tempfile::tempdir().unwrap();
        let os_user = current_username();
        let script = a_script_that_outlives_the_grace_period(tmp.path());

        // When
        let result = spawn_as_user(
            &os_user,
            script.to_str().unwrap(),
            tddy_data_dir.path(),
            tmp.path(),
            &a_livekit_creds(),
            SpawnOptions::default(),
            "info",
            super::CHILD_LOG_FORMAT_FALLBACK,
            None,
        );

        // Then — spawn_as_user does not wait for the child to exit; it only guards against an
        // immediate crash, so a long-running child is reported as a successful spawn
        let spawned = result.expect("a long-running child must be reported as a successful spawn");
        assert!(spawned.pid > 0);
    }
}

#[cfg(test)]
mod resolve_tool_path_tests {
    use super::resolve_tool_path;
    use std::path::Path;

    #[test]
    fn an_absolute_tool_path_is_returned_unchanged() {
        // Given — an operator override, already a full path (e.g. "/usr/local/bin/tddy-coder")
        let daemon_toolchain_root = Path::new("/var/tddy/Code/tddy-coder-worktrees/pr-stacking-ui");

        // When
        let resolved = resolve_tool_path("/usr/local/bin/tddy-coder", daemon_toolchain_root);

        // Then — the daemon toolchain root is irrelevant; the absolute path wins as-is
        assert_eq!(resolved, Path::new("/usr/local/bin/tddy-coder"));
    }

    #[test]
    fn a_relative_tool_path_is_joined_with_the_daemon_toolchain_root() {
        // Given — the shape every `allowed_tools` entry in dev.daemon.yaml actually uses
        let daemon_toolchain_root = Path::new("/var/tddy/Code/tddy-coder-worktrees/pr-stacking-ui");

        // When
        let resolved = resolve_tool_path("target/debug/tddy-coder", daemon_toolchain_root);

        // Then
        assert_eq!(
            resolved,
            Path::new("/var/tddy/Code/tddy-coder-worktrees/pr-stacking-ui/target/debug/tddy-coder")
        );
    }

    #[test]
    fn a_relative_tool_path_never_resolves_against_some_other_directory() {
        // Given — two candidate anchors that must never be conflated: the daemon's own
        // toolchain root, and an unrelated directory that happens to be passed around
        // elsewhere in spawn_as_user (a target session's repo_path)
        let daemon_toolchain_root = Path::new("/var/tddy/Code/tddy-coder-worktrees/pr-stacking-ui");
        let unrelated_target_repo = Path::new("/home/dev/some-unrelated-todo-app");

        // When
        let resolved = resolve_tool_path("target/debug/tddy-coder", daemon_toolchain_root);

        // Then — resolution only ever depends on daemon_toolchain_root
        assert!(!resolved.starts_with(unrelated_target_repo));
        assert!(resolved.starts_with(daemon_toolchain_root));
    }
}

#[cfg(all(test, unix))]
mod daemon_toolchain_resolution_tests {
    use super::{spawn_as_user, LiveKitCreds, SpawnOptions};
    use serial_test::serial;
    use std::path::{Path, PathBuf};

    fn current_username() -> String {
        std::env::var("USER").expect("USER env var must be set to run this test")
    }

    fn a_livekit_creds() -> LiveKitCreds {
        LiveKitCreds {
            url: "ws://127.0.0.1:7880".to_string(),
            api_key: "test-key".to_string(),
            api_secret: "test-secret".to_string(),
            common_room: None,
            daemon_instance_id: None,
        }
    }

    /// Writes an executable script at `<dir>/target/debug/tddy-coder`, mirroring the relative
    /// layout every real `dev.daemon.yaml` `allowed_tools` entry points at. Sleeps rather than
    /// exiting immediately so it survives `spawn_as_user`'s startup grace-period check — this
    /// test is about *locating* the binary, not about post-spawn liveness.
    fn a_fake_tddy_coder_build_under(dir: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let script_dir = dir.join("target").join("debug");
        std::fs::create_dir_all(&script_dir).unwrap();
        let script_path = script_dir.join("tddy-coder");
        std::fs::write(&script_path, "#!/bin/sh\nsleep 2\n").unwrap();
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// RAII guard: switches the test process's cwd for the duration of the test (simulating
    /// "wherever `./web-dev` launched the daemon from") and restores the original cwd on drop —
    /// including when the test panics — so a failing assertion never corrupts the cwd for
    /// whichever test `#[serial]` runs next.
    struct CwdGuard(PathBuf);
    impl CwdGuard {
        fn switch_to(dir: &Path) -> Self {
            let original = std::env::current_dir().expect("read current_dir");
            std::env::set_current_dir(dir).expect("switch current_dir for test");
            Self(original)
        }
    }
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    #[test]
    #[serial]
    fn spawn_as_user_locates_a_relative_tool_path_against_the_daemons_own_cwd_not_the_target_repo()
    {
        // Given — the daemon's own toolchain root (simulated here by the process's cwd, matching
        // "wherever ./web-dev launched the daemon from") contains a tddy-coder build; the target
        // session's repo_path is a *completely unrelated* directory that contains no such build
        // at all — like the TODO-app project this recipe is meant to work against
        let daemon_toolchain_root = tempfile::tempdir().unwrap();
        a_fake_tddy_coder_build_under(daemon_toolchain_root.path());
        let target_repo = tempfile::tempdir().unwrap();
        let tddy_data_dir = tempfile::tempdir().unwrap();
        let _cwd_guard = CwdGuard::switch_to(daemon_toolchain_root.path());

        let os_user = current_username();

        // When — spawn_as_user receives the *relative* tool_path every real `allowed_tools`
        // entry actually uses ("target/debug/tddy-coder"), and a repo_path pointing at the
        // unrelated target project
        let result = spawn_as_user(
            &os_user,
            "target/debug/tddy-coder",
            tddy_data_dir.path(),
            target_repo.path(),
            &a_livekit_creds(),
            SpawnOptions::default(),
            "info",
            super::CHILD_LOG_FORMAT_FALLBACK,
            None,
        );

        // Then — it finds and runs the daemon's own toolchain build, even though target_repo
        // has no tddy-coder build of its own; project selection must never change which binary
        // gets executed
        result.expect(
            "spawn_as_user must locate a relative tool_path against the daemon's own toolchain \
             root, not against the target session's repo_path",
        );
    }
}

#[cfg(test)]
mod resolve_tddy_data_dir_tests {
    use super::resolve_tddy_data_dir;
    use std::path::Path;

    #[test]
    fn an_absolute_tddy_data_dir_is_returned_unchanged() {
        // Given — an operator override via dev.daemon.yaml's `tddy_data_dir`, already absolute
        let daemon_toolchain_root = Path::new("/var/tddy/Code/tddy-coder-worktrees/pr-stacking-ui");

        // When
        let resolved = resolve_tddy_data_dir(Path::new("/var/tddy/.tddy"), daemon_toolchain_root);

        // Then — the daemon toolchain root is irrelevant; the absolute path wins as-is
        assert_eq!(resolved, Path::new("/var/tddy/.tddy"));
    }

    #[test]
    fn a_relative_tddy_data_dir_is_joined_with_the_daemon_toolchain_root() {
        // Given — the shape `default_tddy_data_dir()` actually returns for a debug build
        let daemon_toolchain_root = Path::new("/var/tddy/Code/tddy-coder-worktrees/pr-stacking-ui");

        // When
        let resolved = resolve_tddy_data_dir(Path::new("tmp/.tddy"), daemon_toolchain_root);

        // Then
        assert_eq!(
            resolved,
            Path::new("/var/tddy/Code/tddy-coder-worktrees/pr-stacking-ui/tmp/.tddy")
        );
    }

    #[test]
    fn a_relative_tddy_data_dir_never_resolves_against_some_other_directory() {
        // Given — two candidate anchors that must never be conflated: the daemon's own
        // toolchain root, and an unrelated directory that happens to be passed around
        // elsewhere in spawn_as_user (a target session's repo_path)
        let daemon_toolchain_root = Path::new("/var/tddy/Code/tddy-coder-worktrees/pr-stacking-ui");
        let unrelated_target_repo = Path::new("/home/dev/some-unrelated-todo-app");

        // When
        let resolved = resolve_tddy_data_dir(Path::new("tmp/.tddy"), daemon_toolchain_root);

        // Then — resolution only ever depends on daemon_toolchain_root, never on repo_path
        assert!(!resolved.starts_with(unrelated_target_repo));
        assert!(resolved.starts_with(daemon_toolchain_root));
    }
}

#[cfg(all(test, unix))]
mod daemon_data_dir_passthrough_tests {
    use super::{spawn_as_user, LiveKitCreds, SpawnOptions};
    use serial_test::serial;
    use std::path::{Path, PathBuf};

    fn current_username() -> String {
        std::env::var("USER").expect("USER env var must be set to run this test")
    }

    fn a_livekit_creds() -> LiveKitCreds {
        LiveKitCreds {
            url: "ws://127.0.0.1:7880".to_string(),
            api_key: "test-key".to_string(),
            api_secret: "test-secret".to_string(),
            common_room: None,
            daemon_instance_id: None,
        }
    }

    /// Writes an executable script at `<dir>/target/debug/tddy-coder` that records the argv it
    /// was actually invoked with (one argument per line) to `argv_file`, then sleeps rather than
    /// exiting immediately so it survives `spawn_as_user`'s startup grace-period check — these
    /// tests are about what argv the child *received*, not about post-spawn liveness.
    fn a_fake_tddy_coder_build_that_records_its_argv(dir: &Path, argv_file: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let script_dir = dir.join("target").join("debug");
        std::fs::create_dir_all(&script_dir).unwrap();
        let script_path = script_dir.join("tddy-coder");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{}\"\nsleep 2\n",
            argv_file.display()
        );
        std::fs::write(&script_path, script).unwrap();
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Reads back the argv recorded by [`a_fake_tddy_coder_build_that_records_its_argv`] and
    /// returns the value immediately following `flag` (e.g. `--tddy-data-dir`), if present.
    fn recorded_argv_value_after(argv_file: &Path, flag: &str) -> Option<String> {
        let contents = std::fs::read_to_string(argv_file).expect("read recorded argv file");
        let lines: Vec<&str> = contents.lines().collect();
        lines
            .iter()
            .position(|l| *l == flag)
            .and_then(|i| lines.get(i + 1))
            .map(|s| s.to_string())
    }

    /// RAII guard: switches the test process's cwd for the duration of the test (simulating
    /// "wherever `./web-dev` launched the daemon from") and restores the original cwd on drop —
    /// including when the test panics — so a failing assertion never corrupts the cwd for
    /// whichever test `#[serial]` runs next.
    struct CwdGuard(PathBuf);
    impl CwdGuard {
        fn switch_to(dir: &Path) -> Self {
            let original = std::env::current_dir().expect("read current_dir");
            std::env::set_current_dir(dir).expect("switch current_dir for test");
            Self(original)
        }
    }
    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    #[test]
    #[serial]
    fn spawn_as_user_passes_the_daemons_tddy_data_dir_to_the_child_as_an_absolute_path_argument() {
        // Given — an absolute tddy_data_dir (an operator override, or the daemon's already-
        // resolved home) that is distinct from both the daemon's own toolchain root and the
        // target session's repo_path
        let daemon_toolchain_root = tempfile::tempdir().unwrap();
        let target_repo = tempfile::tempdir().unwrap();
        let tddy_data_dir = tempfile::tempdir().unwrap();
        let argv_file = target_repo.path().join("captured-argv.txt");
        a_fake_tddy_coder_build_that_records_its_argv(daemon_toolchain_root.path(), &argv_file);
        let _cwd_guard = CwdGuard::switch_to(daemon_toolchain_root.path());

        let os_user = current_username();

        // When
        let result = spawn_as_user(
            &os_user,
            "target/debug/tddy-coder",
            tddy_data_dir.path(),
            target_repo.path(),
            &a_livekit_creds(),
            SpawnOptions::default(),
            "info",
            super::CHILD_LOG_FORMAT_FALLBACK,
            None,
        );
        result.expect("spawn_as_user must succeed");

        // Then — the child receives the daemon's own tddy_data_dir verbatim, regardless of
        // repo_path; it must never fall back to deriving session storage from its own cwd
        let received = recorded_argv_value_after(&argv_file, "--tddy-data-dir")
            .expect("child argv must include --tddy-data-dir");
        assert_eq!(received, tddy_data_dir.path().to_str().unwrap());
    }

    #[test]
    #[serial]
    fn spawn_as_user_resolves_a_relative_tddy_data_dir_against_the_daemons_own_cwd_not_the_target_repo(
    ) {
        // Given — the shape `default_tddy_data_dir()` actually returns for a debug build
        // ("tmp/.tddy", relative) when no operator override is configured; the target session's
        // repo_path is a completely unrelated directory the child's cwd gets set to
        let daemon_toolchain_root = tempfile::tempdir().unwrap();
        let target_repo = tempfile::tempdir().unwrap();
        let argv_file = target_repo.path().join("captured-argv.txt");
        a_fake_tddy_coder_build_that_records_its_argv(daemon_toolchain_root.path(), &argv_file);
        let _cwd_guard = CwdGuard::switch_to(daemon_toolchain_root.path());

        let os_user = current_username();

        // When
        let result = spawn_as_user(
            &os_user,
            "target/debug/tddy-coder",
            Path::new("tmp/.tddy"),
            target_repo.path(),
            &a_livekit_creds(),
            SpawnOptions::default(),
            "info",
            super::CHILD_LOG_FORMAT_FALLBACK,
            None,
        );
        result.expect("spawn_as_user must succeed");

        // Then — resolved against the daemon's own cwd, never against repo_path (which would
        // silently split session storage between the daemon and the child it spawned).
        // Canonicalize the expected side too: production resolves the daemon's cwd via
        // `std::env::current_dir()`, which returns the OS-canonicalized path (e.g. macOS
        // resolves `/tmp` to `/private/tmp`), while `daemon_toolchain_root.path()` here is the
        // raw, uncanonicalized `tempfile::TempDir` path — both name the same directory.
        let received = recorded_argv_value_after(&argv_file, "--tddy-data-dir")
            .expect("child argv must include --tddy-data-dir");
        let expected = daemon_toolchain_root
            .path()
            .canonicalize()
            .expect("canonicalize daemon toolchain root")
            .join("tmp/.tddy");
        assert_eq!(received, expected.to_str().unwrap());
    }
}
