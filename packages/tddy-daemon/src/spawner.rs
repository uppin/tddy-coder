//! Process spawner — fork + setuid/setgid to run tddy-* as target OS user.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use uuid::Uuid;

use crate::config::DaemonConfig;

/// LiveKit credentials to pass to spawned process (url, api_key, api_secret).
/// Room and identity are generated per session.
#[derive(Debug, Clone)]
pub struct LiveKitCreds {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
}

/// Create child log config and stderr file. Returns (config_path, stderr_file).
/// Child needs a real stderr so crossterm/terminal APIs work; Stdio::null() can cause SIGSEGV.
fn create_child_log_config_and_stderr(
    repo_path: &Path,
    session_id: &str,
) -> anyhow::Result<(PathBuf, File)> {
    let child_logs_dir = repo_path.join("tmp").join("logs").join("child");
    std::fs::create_dir_all(&child_logs_dir).map_err(|e| {
        anyhow::anyhow!(
            "failed to create child logs dir {}: {}",
            child_logs_dir.display(),
            e
        )
    })?;

    let log_file = child_logs_dir.join(session_id);
    let config_path = child_logs_dir.join(format!("{}.yaml", session_id));

    let yaml = format!(
        r#"log:
  loggers:
    default:
      output: {{ file: "{}" }}
      format: "{{timestamp}} [{{level}}] [{{target}}] {{message}}"
  default:
    level: debug
    logger: default
"#,
        log_file.display()
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

    Ok((config_path, stderr_file))
}

/// Result of spawning a session.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SpawnResult {
    pub session_id: String,
    pub livekit_room: String,
    pub livekit_server_identity: String,
    pub livekit_url: String,
    pub pid: u32,
}

/// Spawn a tddy-* process as the given OS user.
#[cfg(unix)]
pub fn spawn_as_user(
    os_user: &str,
    tool_path: &str,
    repo_path: &Path,
    livekit: &LiveKitCreds,
    resume_session_id: Option<&str>,
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

    let session_id = resume_session_id
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let livekit_room = format!("daemon-{}", session_id);
    // Each session has distinct server identity for LiveKit participant targeting
    let identity = format!("daemon-{}", session_id);

    let home_dir = unsafe { std::ffi::CStr::from_ptr(passwd.pw_dir) }
        .to_string_lossy()
        .into_owned();

    let (config_path, stderr_file) = create_child_log_config_and_stderr(repo_path, &session_id)?;

    let mut cmd = std::process::Command::new(tool_path);
    cmd.current_dir(repo_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file))
        .env("HOME", &home_dir)
        .arg("--daemon")
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

    if let Some(resume_id) = resume_session_id {
        cmd.arg("--resume-from").arg(resume_id);
    } else {
        cmd.arg("--session-id").arg(&session_id);
    }

    cmd.arg("--config").arg(&config_path);

    log::info!(
        "spawning process os_user={} tool={} repo={} session_id={} livekit_room={} livekit_identity={} livekit_url={}",
        os_user,
        tool_path,
        repo_path.display(),
        session_id,
        livekit_room,
        identity,
        livekit.url
    );

    log::debug!("spawner: about to cmd.spawn() session_id={}", session_id);

    // When spawning as same user, skip pre_exec — avoids fork() which can deadlock in some envs.
    // pre_exec forces the slow fork path; plain spawn may use posix_spawn.
    let same_user = uid == unsafe { libc::getuid() } && gid == unsafe { libc::getgid() };
    if !same_user {
        let home_dir_pre = home_dir.clone();
        unsafe {
            cmd.pre_exec(move || {
                std::env::set_var("HOME", &home_dir_pre);
                if libc::setgid(gid) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::initgroups(std::ptr::null(), gid) != 0 {
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
    })
}

#[cfg(not(unix))]
pub fn spawn_as_user(
    _os_user: &str,
    _tool_path: &str,
    _repo_path: &Path,
    _livekit: &LiveKitCreds,
    _resume_session_id: Option<&str>,
) -> anyhow::Result<SpawnResult> {
    anyhow::bail!("spawn_as_user is only supported on Unix")
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
    })
}
