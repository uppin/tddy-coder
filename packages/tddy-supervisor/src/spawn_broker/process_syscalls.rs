use std::ffi::CStr;

/// The answer every step below gives on a host whose kernel does not have the facility it needs.
///
/// The supervisor is a Linux program: systemd starts it, it joins cgroup scopes, and it builds a
/// jail out of namespaces and bind mounts. None of that exists on Darwin, and the sandboxing that
/// does — Seatbelt, in `tddy-sandbox-darwin` — is a different mechanism reached a different way. What
/// the two platforms share is everything this file decides *before* the fork: the step ordering, the
/// credentials it refuses, the environment it builds. Compiling there keeps those under test on a
/// developer's machine.
///
/// So a step that cannot be performed says so and the spawn fails. It is not a reduced jail: a
/// session the supervisor cannot confine is one it does not start.
#[cfg(not(target_os = "linux"))]
pub(crate) fn only_on_linux(facility: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!(
            "{facility} is a Linux facility, and this host is not Linux; the supervisor will not \
             spawn a session it cannot confine"
        ),
    )
}

/// Ask the kernel to signal us when the supervisor dies, and check it has not died already.
///
/// Called once per [`PreExecStep::SetParentDeathSignal`], which a plan contains once per operation
/// that clears the signal plus one: an arming is not sticky, and `commit_creds()` drops it on any
/// change of effective ids or capability set.
///
/// # Safety
///
/// Runs after `fork`, so it may only use async-signal-safe syscalls. `prctl` and `getppid` read and
/// write nothing but this process's own state.
#[cfg(target_os = "linux")]
unsafe fn set_parent_death_signal(supervisor_pid: libc::pid_t) -> std::io::Result<()> {
    // A supervisor that is killed rather than asked to stop should not leave its daemon and
    // sessions behind: without this they would be reparented to init and run on unsupervised.
    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM as libc::c_ulong) != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // The supervisor may have died in the window between `fork` and the line above, in which case
    // the signal it asks for will never arrive. Checking closes that race.
    if libc::getppid() != supervisor_pid {
        libc::_exit(ORPHANED_EXIT_STATUS);
    }
    Ok(())
}

/// `PR_SET_PDEATHSIG` has no counterpart outside Linux — see [`only_on_linux`]. Refused rather than
/// skipped: it is the first step of every plan precisely because a child that cannot be tied to its
/// supervisor's lifetime is one that can outlive it.
#[cfg(not(target_os = "linux"))]
pub(crate) unsafe fn set_parent_death_signal(supervisor_pid: libc::pid_t) -> std::io::Result<()> {
    let _ = supervisor_pid;
    Err(only_on_linux("the parent-death signal"))
}

/// Become the leader of a new session, and so of a process group of our own.
///
/// `setsid` rather than `setpgid(0, 0)`: a supervised process has no business keeping the
/// supervisor's controlling terminal, and a fresh child is never already a group leader, which is
/// the only way `setsid` can fail.
///
/// # Safety
///
/// Runs after `fork`. `setsid` only changes this process's session and group.
pub(crate) unsafe fn lead_own_process_group() -> std::io::Result<()> {
    if libc::setsid() < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// How many groups `setgroups(2)` is being handed: `size_t` in glibc's header, `c_int` in Darwin's.
/// The count is the length of a group list resolved from the passwd database, so neither type can be
/// narrowed by the cast.
#[cfg(target_os = "linux")]
type GroupCount = libc::size_t;

#[cfg(not(target_os = "linux"))]
type GroupCount = libc::c_int;

/// `setgroups` → `setgid` → `setuid`. Root is surrendered here and never comes back.
///
/// `setgroups(2)` rather than `initgroups(3)`: the list was enumerated before the fork precisely so
/// this is a bare syscall — see [`TargetUser::groups`]. It goes first because dropping the group set
/// needs `CAP_SETGID`, which `setuid` is about to take away.
///
/// # Safety
///
/// Runs after `fork`. `groups` must remain valid for the call. All three change only this process's
/// own credentials, and the order is the security contract: the group set must be replaced while the
/// process still has the privilege to replace it.
pub(crate) unsafe fn drop_privilege(
    uid: libc::uid_t,
    gid: libc::gid_t,
    groups: &[libc::gid_t],
) -> std::io::Result<()> {
    if libc::setgroups(groups.len() as GroupCount, groups.as_ptr()) != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if libc::setgid(gid) != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if libc::setuid(uid) != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// # Safety
///
/// Runs after `fork`. `path` must remain valid for the call; `chdir` changes only this process's cwd.
pub(crate) unsafe fn change_directory(path: &CStr) -> std::io::Result<()> {
    if libc::chdir(path.as_ptr()) != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Write our own pid into an `cgroup.procs`, allocation-free.
///
/// `O_CREAT` is not an accommodation: on cgroupfs the file always exists and the flag is inert,
/// while a delegated base that is a plain directory (a `base_override` pointing outside
/// `/sys/fs/cgroup`) has no file to open yet.
///
/// # Safety
///
/// `procs_path` must remain valid for the duration of the call. Runs after `fork`, so it may only
/// use async-signal-safe syscalls.
pub(super) unsafe fn join_cgroup_scope(procs_path: &CStr) -> std::io::Result<()> {
    // 10 digits covers every u32 pid, plus the newline the kernel expects.
    let mut buffer = [0u8; 11];
    let mut start = buffer.len() - 1;
    buffer[start] = b'\n';
    let mut pid = libc::getpid() as u32;
    loop {
        start -= 1;
        buffer[start] = b'0' + (pid % 10) as u8;
        pid /= 10;
        if pid == 0 {
            break;
        }
    }
    let digits = &buffer[start..];

    let fd = libc::open(
        procs_path.as_ptr(),
        libc::O_WRONLY | libc::O_APPEND | libc::O_CREAT | libc::O_CLOEXEC,
        0o644 as libc::c_int,
    );
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let written = libc::write(fd, digits.as_ptr().cast(), digits.len());
    libc::close(fd);
    if written < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
