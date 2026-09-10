//! Becoming another OS user: resolving them, deciding whether privileges must be dropped, and the
//! environment a child impersonating them runs with.
//!
//! Lifted out of `pty_runtime.rs` by `#unbundle` node 1 — five symbols of a 400-line module whose
//! other half is the PTY spawn machinery (`PtyRuntime`, `PtySpawnSpec`, the task registry) that no
//! moving subsystem touches. `ssh_agent` needs [`resolve_pty_os_user`]; `remote_git_service` needs
//! all five, because the git pipe relay impersonates an OS user without a PTY and must resolve the
//! same ids the PTY path does rather than deriving its own and drifting from it.
//!
//! `pty_runtime.rs` re-exports every name, so no caller in `tddy-daemon` changed.

/// Environment overrides applied to a PTY child impersonating a target OS user.
///
/// Sets `HOME` to the target user's home directory (so per-user config/credentials resolve there)
/// and prepends the user's configured `PATH` extra ahead of the daemon's `PATH` (so a user-local
/// install such as `~/.local/bin/claude` is found). Mirrors the env `tddy_spawn::spawner::spawn_as_user`
/// applies to non-interactive spawns.
pub fn pty_user_env_overrides(
    home_dir: &std::path::Path,
    path_extra: Option<&str>,
) -> Vec<(String, String)> {
    vec![
        ("HOME".to_string(), home_dir.to_string_lossy().into_owned()),
        (
            "PATH".to_string(),
            crate::spawn_as_user::merge_spawn_child_path(path_extra),
        ),
    ]
}

/// Whether spawning as the target user requires dropping privileges: true unless the target
/// uid+gid already match the daemon's current identity (dev / single-user, where no setuid is
/// needed).
pub fn pty_requires_privilege_drop(
    target_uid: u32,
    target_gid: u32,
    current_uid: u32,
    current_gid: u32,
) -> bool {
    !(target_uid == current_uid && target_gid == current_gid)
}

/// Wrap `argv` so it execs behind `setpriv`, dropping to the target user's uid/gid with
/// initialized supplementary groups. `setpriv` preserves the environment, so the HOME/PATH
/// overrides applied to the command are kept.
pub fn wrap_argv_for_privilege_drop(argv: &[String], uid: u32, gid: u32) -> Vec<String> {
    let mut wrapped = vec![
        "setpriv".to_string(),
        "--reuid".to_string(),
        uid.to_string(),
        "--regid".to_string(),
        gid.to_string(),
        "--init-groups".to_string(),
        "--".to_string(),
    ];
    wrapped.extend_from_slice(argv);
    wrapped
}

/// The uid/gid/home of a target OS user, resolved from the passwd database.
#[cfg(unix)]
pub struct ResolvedPtyUser {
    pub uid: u32,
    pub gid: u32,
    pub home_dir: String,
}

/// Resolve `os_user` to its uid/gid/home via `getpwnam_r`. Mirrors the passwd lookup in
/// `tddy_spawn::spawner::spawn_as_user`.
///
/// Public so non-PTY spawns that impersonate an OS user (the git pipe relay in
/// `tddy_worktree_service::remote_git_service`) resolve the same ids from the same place, rather than deriving
/// their own and drifting from the PTY path.
#[cfg(unix)]
pub fn resolve_pty_os_user(os_user: &str) -> Result<ResolvedPtyUser, String> {
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    let name = std::ffi::CString::new(os_user).map_err(|e| format!("invalid username: {e}"))?;
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
        return Err(format!("user '{os_user}' not found"));
    }
    let passwd = unsafe { &*result };
    if passwd.pw_dir.is_null() {
        return Err(format!("user '{os_user}' has no home directory"));
    }
    let home_dir = unsafe { std::ffi::CStr::from_ptr(passwd.pw_dir) }
        .to_string_lossy()
        .into_owned();
    Ok(ResolvedPtyUser {
        uid: passwd.pw_uid,
        gid: passwd.pw_gid,
        home_dir,
    })
}
