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

/// Everything the child needs to become `os_user`, resolved in the parent **before** `fork`.
///
/// A child between `fork` and `exec` may only make async-signal-safe calls, so nothing here may be
/// looked up there: the passwd entry and the supplementary group list (`getgrouplist` — the NSS walk
/// `initgroups(3)` would do after the fork) are settled up front.
#[cfg(unix)]
#[derive(Debug, Clone)]
pub struct TargetAccount {
    /// The account's login name, as the caller asked for it.
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    /// Supplementary groups, primary group included, for `setgroups(2)` in the child.
    pub groups: Vec<libc::gid_t>,
    /// The account's home directory, for the child's `HOME` and working directory.
    pub home: String,
}

#[cfg(unix)]
impl TargetAccount {
    /// True when the child would run as the identity this process already has by effective ids, so
    /// no privilege change is needed and the `setgroups`/`setgid`/`setuid` steps are skipped.
    pub fn is_this_process(&self) -> bool {
        // SAFETY: `geteuid` and `getegid` take no arguments and cannot fail.
        self.uid == unsafe { libc::geteuid() } && self.gid == unsafe { libc::getegid() }
    }
}

/// Resolve `os_user` to a [`TargetAccount`] — passwd entry and supplementary groups — in the parent.
#[cfg(unix)]
pub fn resolve_target_account(os_user: &str) -> Result<TargetAccount, String> {
    let name = std::ffi::CString::new(os_user).map_err(|e| format!("invalid username: {e}"))?;
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    // SAFETY: `name` is a live C string, `passwd` and `buf` outlive the call, and `buf.len()` is
    // the size of `buf`; `getpwnam_r` writes only into them and `result`.
    let ret = unsafe {
        libc::getpwnam_r(
            name.as_ptr(),
            passwd.as_mut_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            &mut result,
        )
    };
    if ret != 0 || result.is_null() {
        return Err(format!("user '{os_user}' not found"));
    }
    // SAFETY: `getpwnam_r` reported success, so `result` points at the initialised entry, whose
    // strings live in `buf` until this function returns.
    let entry = unsafe { &*result };
    if entry.pw_dir.is_null() {
        return Err(format!("user '{os_user}' has no home directory"));
    }
    // SAFETY: `pw_dir` was checked non-null above and is a NUL-terminated string inside `buf`.
    let home = unsafe { std::ffi::CStr::from_ptr(entry.pw_dir) }
        .to_string_lossy()
        .into_owned();
    let groups =
        supplementary_groups(&name, entry.pw_gid).map_err(|e| format!("user '{os_user}': {e}"))?;
    Ok(TargetAccount {
        name: os_user.to_string(),
        uid: entry.pw_uid,
        gid: entry.pw_gid,
        groups,
        home,
    })
}

/// A group id as `getgrouplist(3)` takes it: `gid_t` in glibc's header, `c_int` in Darwin's. Both
/// are 32 bits wide, so this is the two platforms spelling one argument differently rather than
/// passing different things.
#[cfg(all(unix, target_os = "linux"))]
type GroupListId = libc::gid_t;
#[cfg(all(unix, not(target_os = "linux")))]
type GroupListId = libc::c_int;

/// The group list `initgroups(3)` would install for an account: its primary group plus every group
/// that lists it as a member. Reads NSS, so it runs in the parent before the fork.
///
/// The same algorithm as `tddy_supervisor`'s resolver, which this crate cannot depend on. The retry
/// covers accounts in more groups than the first buffer holds, and a failure to enumerate is an
/// error: a short list would be a silently different set of permissions for the child.
#[cfg(unix)]
fn supplementary_groups(name: &std::ffi::CStr, gid: u32) -> Result<Vec<libc::gid_t>, String> {
    let mut groups: Vec<libc::gid_t> = vec![0; 32];
    loop {
        let mut count = groups.len() as libc::c_int;
        // SAFETY: `name` is a live C string and `count` describes `groups` exactly. On overflow
        // `getgrouplist` returns -1 and writes the required length into `count` without writing
        // past the buffer it was given.
        let found = unsafe {
            libc::getgrouplist(
                name.as_ptr(),
                gid as GroupListId,
                groups.as_mut_ptr().cast::<GroupListId>(),
                &mut count,
            )
        };
        if found >= 0 {
            // Truncated to `count`, not to the return value: glibc returns the number of groups
            // found, Darwin returns 0 for "they fit", and both write the number into `count`.
            groups.truncate(count as usize);
            return Ok(groups);
        }
        if count as usize <= groups.len() {
            return Err("could not resolve its supplementary groups".to_string());
        }
        groups.resize(count as usize, 0);
    }
}

/// Who this process is, by **effective** ids — the identity `setuid` and friends check.
#[cfg(unix)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveIdentity {
    pub uid: u32,
    pub gid: u32,
    /// The login name for `uid`, or the uid in decimal when it has none.
    pub name: String,
}

#[cfg(unix)]
impl EffectiveIdentity {
    /// This process's effective identity.
    pub fn current() -> EffectiveIdentity {
        // SAFETY: `geteuid` and `getegid` take no arguments and cannot fail.
        let uid = unsafe { libc::geteuid() };
        let gid = unsafe { libc::getegid() };
        EffectiveIdentity {
            uid,
            gid,
            name: login_name_of(uid).unwrap_or_else(|| uid.to_string()),
        }
    }
}

/// The login name the passwd database holds for `uid`, if it holds one.
#[cfg(unix)]
fn login_name_of(uid: u32) -> Option<String> {
    let mut passwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut buf = vec![0u8; 16384];
    let mut result = std::ptr::null_mut();
    // SAFETY: `passwd` and `buf` outlive the call and `buf.len()` is the size of `buf`;
    // `getpwuid_r` writes only into them and `result`.
    let ret = unsafe {
        libc::getpwuid_r(
            uid,
            passwd.as_mut_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            &mut result,
        )
    };
    if ret != 0 || result.is_null() {
        return None;
    }
    // SAFETY: `getpwuid_r` reported success, so `result` points at the initialised entry.
    let entry = unsafe { &*result };
    if entry.pw_name.is_null() {
        return None;
    }
    Some(
        // SAFETY: `pw_name` was checked non-null above and is a NUL-terminated string inside `buf`.
        unsafe { std::ffi::CStr::from_ptr(entry.pw_name) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// What a child must do to become its target account.
#[cfg(unix)]
#[derive(Debug, Clone)]
pub enum PrivilegeChange {
    /// The target is this process's own effective identity: the child changes nothing.
    None,
    /// The child sets groups, gid and uid to the target's, in that order.
    Drop(TargetAccount),
}

/// A child was asked to run as another user by a process that cannot switch users.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CannotBecomeUser {
    /// The identity this process runs as.
    pub running_as: String,
    /// The account the child was to run as.
    pub requested: String,
}

impl std::fmt::Display for CannotBecomeUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "this process runs as '{}' and cannot start a child as '{}': only root can switch users. \
             On a host run by tddy-supervisor, a spawn as another user is brokered by the supervisor",
            self.running_as, self.requested
        )
    }
}

impl std::error::Error for CannotBecomeUser {}

/// Decide, before any fork, what a child must do to become `target` — or refuse, when this process
/// is not root and `target` is somebody else. Reads this process's effective identity.
#[cfg(unix)]
pub fn plan_privilege_change(target: TargetAccount) -> Result<PrivilegeChange, CannotBecomeUser> {
    plan_privilege_change_from(target, &EffectiveIdentity::current())
}

/// [`plan_privilege_change`] with the current identity passed in, so the decision is testable on a
/// host where the test is not root.
#[cfg(unix)]
pub fn plan_privilege_change_from(
    target: TargetAccount,
    current: &EffectiveIdentity,
) -> Result<PrivilegeChange, CannotBecomeUser> {
    if target.uid == current.uid && target.gid == current.gid {
        return Ok(PrivilegeChange::None);
    }
    // Only root may set another uid or gid; a non-root process with a matching uid but another gid
    // could not `setgid` either, so it is refused here rather than failing inside the child.
    if current.uid == ROOT_UID {
        return Ok(PrivilegeChange::Drop(target));
    }
    Err(CannotBecomeUser {
        running_as: current.name.clone(),
        requested: target.name,
    })
}

#[cfg(unix)]
const ROOT_UID: u32 = 0;
