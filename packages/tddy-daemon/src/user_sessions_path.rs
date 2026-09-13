//! Resolve OS user to their sessions directory path.

/// Where a user's things live, derived in [`tddy_daemon_kernel::user_paths`].
///
/// These four are the resolvers a subsystem crate reaches; `username_for_uid` and
/// `project_path_under_home_from_user_relative` below stayed because nothing that left this crate
/// names them. Re-exported so every caller here keeps its `crate::user_sessions_path::…` path.
pub use tddy_daemon_kernel::user_paths::{
    home_dir_for_user, projects_path_for_user, repos_base_for_user, sessions_base_for_user,
};

use std::path::{Path, PathBuf};

/// OS username for a uid (from passwd, via the reentrant `getpwuid_r`). Injected into the local
/// peer-trust path so a SO_PEERCRED peer uid can be matched against a configured `users[]` entry.
#[cfg(unix)]
pub fn username_for_uid(uid: u32) -> Option<String> {
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    let ret = unsafe {
        libc::getpwuid_r(
            uid as libc::uid_t,
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
    let name = unsafe { std::ffi::CStr::from_ptr(passwd.pw_name) }
        .to_string_lossy()
        .into_owned();
    Some(name)
}

#[cfg(not(unix))]
pub fn username_for_uid(_uid: u32) -> Option<String> {
    None
}

/// Data root (parent of `sessions/`) for a `tddy-coder` child that runs with the same config.
#[cfg(unix)]
pub fn tddy_data_root_matching_child(os_user: &str, config_dir: Option<&Path>) -> Option<PathBuf> {
    sessions_base_for_user(os_user, config_dir)
}

#[cfg(not(unix))]
pub fn tddy_data_root_matching_child(
    _os_user: &str,
    _config_dir: Option<&Path>,
) -> Option<PathBuf> {
    None
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
