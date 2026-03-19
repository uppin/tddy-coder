//! Process spawner — fork + setuid/setgid to run tddy-* as target OS user.

use std::path::Path;
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

/// Result of spawning a session.
#[derive(Debug)]
pub struct SpawnResult {
    pub session_id: String,
    pub livekit_room: String,
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

    let session_id = resume_session_id
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let livekit_room = format!("daemon-{}", session_id);
    let identity = format!("daemon-{}", session_id);

    let mut cmd = std::process::Command::new(tool_path);
    cmd.current_dir(repo_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd.arg("--daemon")
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

    unsafe {
        cmd.pre_exec(move || {
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

    let child = cmd.spawn()?;
    let pid = child.id();

    Ok(SpawnResult {
        session_id,
        livekit_room,
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
