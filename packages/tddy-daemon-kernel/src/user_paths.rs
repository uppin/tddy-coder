//! Where a user's things live: their home, the data root under it, their projects and repos, and
//! the per-user settings that add to a spawned child's `PATH`.
//!
//! Lifted by `#unbundle` node 1 out of `user_sessions_path.rs` and `tddy_user_config.rs` — the
//! four path resolvers and the one settings read that families E, F, G and H reach, and nothing
//! else. `username_for_uid` (the SO_PEERCRED peer-trust path) and
//! `project_path_under_home_from_user_relative` (project cloning) stayed in the daemon: no moving
//! module names either.
//!
//! Both origin modules re-export every name here, so no caller in `tddy-daemon` changed.

use std::path::{Path, PathBuf};

/// Home directory for an OS user (from passwd).
#[cfg(unix)]
pub fn home_dir_for_user(os_user: &str) -> Option<PathBuf> {
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    let ret = unsafe {
        libc::getpwnam_r(
            std::ffi::CString::new(os_user).ok()?.as_ptr(),
            passwd.as_mut_ptr(),
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if ret != 0 || result.is_null() {
        return None;
    }
    let passwd = unsafe { &*result };
    let home = unsafe { std::ffi::CStr::from_ptr(passwd.pw_dir) }.to_string_lossy();
    Some(PathBuf::from(home.as_ref()))
}

#[cfg(not(unix))]
pub fn home_dir_for_user(_os_user: &str) -> Option<PathBuf> {
    None
}

/// Resolve the sessions base path for an OS user.
///
/// If `config_dir` is `Some`, it is used directly (config is the single source of truth).
/// Otherwise falls back to the profile default (`tmp/.tddy` in debug, `$HOME/.tddy` in release).
///
/// Callers (e.g. `list_sessions_in_dir`) append `SESSIONS_SUBDIR` ("sessions") to reach
/// the actual session directories at `~/.tddy/sessions/{session_id}/`.
#[cfg(unix)]
pub fn sessions_base_for_user(os_user: &str, config_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(d) = config_dir {
        return Some(d.to_path_buf());
    }
    tddy_core::output::default_tddy_data_dir()
        .or_else(|| home_dir_for_user(os_user).map(|h| h.join(".tddy")))
}

#[cfg(not(unix))]
pub fn sessions_base_for_user(_os_user: &str, _config_dir: Option<&Path>) -> Option<PathBuf> {
    None
}

/// Directory containing `projects.yaml` (`{tddy_data_dir}/projects/`).
///
/// When `config_dir` is `Some`, it is used as the data root.
/// Otherwise falls back to the profile default or `$HOME/.tddy`.
#[cfg(unix)]
pub fn projects_path_for_user(os_user: &str, config_dir: Option<&Path>) -> Option<PathBuf> {
    let base = sessions_base_for_user(os_user, config_dir)?;
    Some(base.join("projects"))
}

#[cfg(not(unix))]
pub fn projects_path_for_user(_os_user: &str, _config_dir: Option<&Path>) -> Option<PathBuf> {
    None
}

/// Base directory for cloned repos (~user/{repos_base_path}/).
#[cfg(unix)]
pub fn repos_base_for_user(os_user: &str, repos_base_path: &str) -> Option<PathBuf> {
    Some(home_dir_for_user(os_user)?.join(repos_base_path))
}

#[cfg(not(unix))]
pub fn repos_base_for_user(_os_user: &str, _repos_base_path: &str) -> Option<PathBuf> {
    None
}

// ---------------------------------------------------------------------------------------------
// Per–OS-user settings read from `{home}/.tddy/config.yaml` — the home of the user a child runs as.
// ---------------------------------------------------------------------------------------------

/// YAML schema for `{home}/.tddy/config.yaml`.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TddyUserHomeConfig {
    /// Colon-separated directories prepended to `PATH` for spawned `tddy-coder` (e.g. Cursor `agent`).
    #[serde(default)]
    pub spawn_path_extra: Option<String>,
}

/// Load `~/.tddy/config.yaml` under `home`. Missing file returns `None`; parse errors are logged.
pub fn load_tddy_user_config(home: &Path) -> Option<TddyUserHomeConfig> {
    let path = home.join(".tddy").join("config.yaml");
    if !path.is_file() {
        return None;
    }
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("tddy user config: read {}: {}", path.display(), e);
            return None;
        }
    };
    match serde_yaml::from_str::<TddyUserHomeConfig>(&contents) {
        Ok(c) => Some(c),
        Err(e) => {
            log::warn!("tddy user config: parse {}: {}", path.display(), e);
            None
        }
    }
}

/// `spawn_path_extra` from the target user's `~/.tddy/config.yaml`, if set and non-empty.
pub fn spawn_path_extra_for_home(home: &Path) -> Option<String> {
    load_tddy_user_config(home).and_then(|c| {
        c.spawn_path_extra
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}
