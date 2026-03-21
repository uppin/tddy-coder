//! Resolve OS user to their sessions directory path.

use std::path::PathBuf;

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

/// Resolve the sessions base path for an OS user (~user/.tddy/sessions).
#[cfg(unix)]
pub fn sessions_base_for_user(os_user: &str) -> Option<PathBuf> {
    Some(home_dir_for_user(os_user)?.join(".tddy").join("sessions"))
}

#[cfg(not(unix))]
pub fn sessions_base_for_user(_os_user: &str) -> Option<PathBuf> {
    None
}

/// Directory containing `projects.yaml` (~user/.tddy/projects/).
#[cfg(unix)]
pub fn projects_path_for_user(os_user: &str) -> Option<PathBuf> {
    Some(home_dir_for_user(os_user)?.join(".tddy").join("projects"))
}

#[cfg(not(unix))]
pub fn projects_path_for_user(_os_user: &str) -> Option<PathBuf> {
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
