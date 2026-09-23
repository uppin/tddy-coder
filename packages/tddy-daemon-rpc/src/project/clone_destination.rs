//! Where a project clone lands when the caller names a path under their home.
//!
//! Moved from `tddy-session-lifecycle`'s `user_sessions_path` with the project handlers, its only
//! caller.

use std::path::PathBuf;

#[cfg(unix)]
use tddy_daemon_kernel::user_paths::home_dir_for_user;

/// Resolve a path under the user's home from a user-relative string (for project clone destination).
///
/// Accepts e.g. `Code/my-app` or `~/Code/my-app`. Rejects absolute paths, `..` segments, and empty
/// paths after normalization.
#[cfg(unix)]
pub(crate) fn project_path_under_home_from_user_relative(
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
pub(crate) fn project_path_under_home_from_user_relative(
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
