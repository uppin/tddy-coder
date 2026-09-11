//! Which shell a `StartTerminalSession` terminal runs.
//!
//! Moved out of `tddy-daemon`'s `pty_runtime.rs` by `#unbundle` node 6: it is a passwd lookup with
//! no daemon state behind it, and it answers a question this crate's
//! [`StartTerminalSession`](crate::proto::terminal_session::TerminalSessionService::start_terminal_session)
//! handler asks — so the host's [`TerminalRoster`](crate::service::TerminalRoster) is handed a
//! resolved shell path and only has to spawn it.
//!
//! The rest of `pty_runtime.rs` stayed behind: `PtyRuntime`/`PtySpawnSpec` exist to front-load a
//! `setpriv` privilege drop through `tddy_daemon_kernel::privilege_drop`, and depending on that
//! crate from here would put the whole LiveKit SDK inside `tddy-tools`'
//! `--no-default-features` in-jail build, which carries none today.

/// The shell a login terminal for `os_user` runs: their passwd `pw_shell`, else the serving
/// process's `$SHELL`, else `/bin/bash`.
///
/// The passwd entry is preferred over `$SHELL` because a daemon started by systemd or nix has a
/// `$SHELL` of its own that is not the target user's interactive shell — a terminal opened with it
/// would come up in the wrong shell with none of the user's rc files.
#[must_use]
pub fn login_shell_for(os_user: &str) -> String {
    login_shell_for_os_user(os_user)
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "/bin/bash".to_string())
}

/// The login shell (`pw_shell`) of `os_user` from the passwd database, or `None` when the entry is
/// missing, has no shell, or has one that refuses logins (`/usr/sbin/nologin`, `/bin/false`).
#[cfg(unix)]
#[must_use]
pub fn login_shell_for_os_user(os_user: &str) -> Option<String> {
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    let name = std::ffi::CString::new(os_user).ok()?;
    let ret = unsafe {
        libc::getpwnam_r(
            name.as_ptr(),
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
    if passwd.pw_shell.is_null() {
        return None;
    }
    let shell = unsafe { std::ffi::CStr::from_ptr(passwd.pw_shell) }
        .to_string_lossy()
        .into_owned();
    if shell.is_empty() || shell.ends_with("/nologin") || shell.ends_with("/false") {
        None
    } else {
        Some(shell)
    }
}

#[cfg(not(unix))]
#[must_use]
pub fn login_shell_for_os_user(_os_user: &str) -> Option<String> {
    None
}
