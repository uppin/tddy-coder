//! Resolve OS user to their sessions directory path.

use std::path::PathBuf;

use tddy_core::output::TDDY_SESSIONS_DIR_ENV;

/// When set to a non-empty path, [`projects_path_for_user`] returns this directory (where
/// `projects.yaml` lives) instead of `~/.tddy/projects`.
///
/// **Intended for integration tests only** (e.g. isolated `projects.yaml` without touching a real
/// home directory). Leave unset in production. In CI, do **not** export this globally across unrelated
/// test jobs: it affects every `projects_path_for_user` call in the same process. Prefer setting
/// it only around suites that need it (see `multi_host_acceptance` restore pattern) or use
/// `#[serial]` where the env is mutated.
pub const TDDY_PROJECTS_DIR_ENV: &str = "TDDY_PROJECTS_DIR";

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

/// Resolve the sessions base path for an OS user (~user/.tddy).
///
/// Callers (e.g. `list_sessions_in_dir`) append `SESSIONS_SUBDIR` ("sessions") to reach
/// the actual session directories at `~/.tddy/sessions/{session_id}/`.
#[cfg(unix)]
pub fn sessions_base_for_user(os_user: &str) -> Option<PathBuf> {
    Some(home_dir_for_user(os_user)?.join(".tddy"))
}

/// Data root (parent of `sessions/`) matching [`tddy_core::output::tddy_data_dir_path`] for a
/// `tddy-coder` child that inherits this process environment and runs with `HOME` set to that user.
///
/// If `TDDY_SESSIONS_DIR` is set (same as the child sees), Telegram and other daemon code must use
/// this root when writing `~/.tddy/sessions/...` artifacts; otherwise the child reads an empty or
/// different `changeset.yaml` than the one the daemon wrote under `$HOME/.tddy`.
#[cfg(unix)]
pub fn tddy_data_root_matching_child(os_user: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var(TDDY_SESSIONS_DIR_ENV) {
        let t = p.trim();
        if !t.is_empty() {
            return Some(PathBuf::from(t));
        }
    }
    sessions_base_for_user(os_user)
}

#[cfg(not(unix))]
pub fn sessions_base_for_user(_os_user: &str) -> Option<PathBuf> {
    None
}

#[cfg(not(unix))]
pub fn tddy_data_root_matching_child(_os_user: &str) -> Option<PathBuf> {
    None
}

/// Directory containing `projects.yaml` (~user/.tddy/projects/), unless [`TDDY_PROJECTS_DIR_ENV`] is set.
#[cfg(unix)]
pub fn projects_path_for_user(os_user: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var(TDDY_PROJECTS_DIR_ENV) {
        let t = p.trim();
        if !t.is_empty() {
            return Some(PathBuf::from(t));
        }
    }
    Some(home_dir_for_user(os_user)?.join(".tddy").join("projects"))
}

#[cfg(not(unix))]
pub fn projects_path_for_user(_os_user: &str) -> Option<PathBuf> {
    if let Ok(p) = std::env::var(TDDY_PROJECTS_DIR_ENV) {
        let t = p.trim();
        if !t.is_empty() {
            return Some(PathBuf::from(t));
        }
    }
    None
}

/// Base directory for cloned repos (~user/{repos_base_path}/).
#[cfg(unix)]
pub fn repos_base_for_user(os_user: &str, repos_base_path: &str) -> Option<PathBuf> {
    Some(home_dir_for_user(os_user)?.join(repos_base_path))
}

/// Resolve a path under the user's home from a user-relative string (for project clone destination).
///
/// Accepts e.g. `Code/my-app` or `~/Code/my-app`. Rejects absolute paths, `..` segments, and empty
/// paths after normalization.
#[cfg(unix)]
pub fn project_path_under_home_from_user_relative(
    os_user: &str,
    user_relative_path: &str,
) -> Result<PathBuf, String> {
    let home =
        home_dir_for_user(os_user).ok_or_else(|| "could not resolve home directory".to_string())?;
    let mut s = user_relative_path.trim();
    if s.is_empty() {
        return Err("path is empty".to_string());
    }
    if s.starts_with("~/") {
        s = &s[2..];
    } else if s == "~" {
        s = "";
    }
    if s.starts_with('/') {
        return Err("path must be relative to home, not an absolute path".to_string());
    }
    if s.is_empty() {
        return Err("path is empty".to_string());
    }
    let mut dest = home.clone();
    for part in s.split('/').filter(|p| !p.is_empty()) {
        if part == "." {
            continue;
        }
        if part == ".." {
            return Err("invalid path component".to_string());
        }
        if part.contains('\0') {
            return Err("invalid path".to_string());
        }
        dest.push(part);
    }
    if !dest.starts_with(&home) {
        return Err("path escapes home directory".to_string());
    }
    Ok(dest)
}

#[cfg(not(unix))]
pub fn repos_base_for_user(_os_user: &str, _repos_base_path: &str) -> Option<PathBuf> {
    None
}

#[cfg(not(unix))]
pub fn project_path_under_home_from_user_relative(
    _os_user: &str,
    _user_relative_path: &str,
) -> Result<PathBuf, String> {
    Err("unsupported platform".to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn project_path_under_home_accepts_simple_relative() {
        let u = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
        let home = home_dir_for_user(&u).expect("home");
        let got = project_path_under_home_from_user_relative(&u, "Code/foo").unwrap();
        assert_eq!(got, home.join("Code").join("foo"));
    }

    #[test]
    fn project_path_under_home_accepts_tilde_prefix() {
        let u = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
        let home = home_dir_for_user(&u).expect("home");
        let got = project_path_under_home_from_user_relative(&u, "~/Code/foo").unwrap();
        assert_eq!(got, home.join("Code").join("foo"));
    }

    #[test]
    fn project_path_under_home_rejects_dotdot() {
        let u = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
        assert!(project_path_under_home_from_user_relative(&u, "a/../b").is_err());
    }

    #[test]
    fn project_path_under_home_rejects_absolute() {
        let u = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
        assert!(project_path_under_home_from_user_relative(&u, "/etc/passwd").is_err());
    }
}
