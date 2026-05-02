//! Process spawner — fork + setuid/setgid to run tddy-* as target OS user.

use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;

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

    let format_quoted = yaml_double_quote_scalar(child_log_format);
    let yaml = format!(
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
    );

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
                if libc::initgroups(pw_name.as_ptr(), gid) != 0 {
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
pub fn spawn_as_user(
    os_user: &str,
    tool_path: &str,
    repo_path: &Path,
    livekit: &LiveKitCreds,
    opts: SpawnOptions<'_>,
    child_log_level: &str,
    child_log_format: &str,
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

    let mut cmd = std::process::Command::new(tool_path);
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
        .arg(&identity);

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
        tool_path,
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
                if libc::initgroups(pw_name.as_ptr(), gid) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::setuid(uid) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let mut child = cmd.spawn()?;
    let pid = child.id();

    log::info!(
        "spawn ok session_id={} pid={} livekit_room={} livekit_server_identity={}",
        session_id,
        pid,
        livekit_room,
        identity
    );

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
pub fn spawn_as_user(
    _os_user: &str,
    _tool_path: &str,
    _repo_path: &Path,
    _livekit: &LiveKitCreds,
    _opts: SpawnOptions<'_>,
    _child_log_level: &str,
    _child_log_format: &str,
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
        let mut c = DaemonConfig {
            livekit: Some(LiveKitConfig {
                url: Some("ws://x".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(livekit_spawn_daemon_instance_id(&c), None);
        c.daemon_instance_id = Some(" west ".into());
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
