//! Resolve OS user to their sessions directory path.

/// Where a user's things live, derived in [`tddy_daemon_kernel::user_paths`].
///
/// These four are the resolvers a subsystem crate reaches; `username_for_uid` below stayed because
/// nothing that left this crate names it. Re-exported so every caller here keeps its
/// `crate::user_sessions_path::…` path. (`project_path_under_home_from_user_relative` left with the
/// project handlers for `tddy-daemon-rpc`, its only caller.)
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
