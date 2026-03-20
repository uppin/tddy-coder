//! Resolve OS user to their sessions directory path.

use std::path::PathBuf;

/// Resolve the sessions base path for an OS user (~user/.tddy/sessions).
#[cfg(unix)]
pub fn sessions_base_for_user(os_user: &str) -> Option<PathBuf> {
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    let uid = unsafe {
        libc::getpwnam_r(
            std::ffi::CString::new(os_user).ok()?.as_ptr(),
            passwd.as_mut_ptr(),
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            &mut result,
        )
    };
    if uid != 0 || result.is_null() {
        return None;
    }
    let passwd = unsafe { &*result };
    let home = unsafe { std::ffi::CStr::from_ptr(passwd.pw_dir) }.to_string_lossy();
    Some(PathBuf::from(home.as_ref()).join(".tddy").join("sessions"))
}

#[cfg(not(unix))]
pub fn sessions_base_for_user(_os_user: &str) -> Option<PathBuf> {
    None
}
