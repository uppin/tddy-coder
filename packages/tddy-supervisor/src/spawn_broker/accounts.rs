use crate::config::{ROOT_GID, ROOT_UID};
use std::ffi::{CStr, CString};
use std::path::PathBuf;

use crate::spawn_broker::GroupListId;

/// A target OS user, fully resolved before the fork so the child never has to consult NSS.
#[derive(Debug, Clone)]
pub struct TargetUser {
    pub uid: u32,
    pub gid: u32,
    /// `pw_name`. Kept as a C string because it becomes `USER`/`LOGNAME` in a child that cannot
    /// allocate.
    pub name: CString,
    pub home: PathBuf,
    /// The account's supplementary groups, resolved here so the child calls nothing but
    /// `setgroups(2)`.
    ///
    /// This is what `initgroups(3)` would have computed, and computing it *before* the fork is not a
    /// micro-optimisation. `initgroups` calls `getgrouplist`, which allocates and enters NSS: for
    /// `files` it opens a `FILE*`, and for `sssd`/`ldap` it does socket I/O or a `dlopen`. None of that
    /// is async-signal-safe, and the fork happens from one thread of a multi-threaded process, so a
    /// malloc-arena or NSS lock held by any other thread at fork time is held forever in the child.
    /// `packages/tddy-daemon/src/spawner.rs:909` records this crate's own encounter with it — spawns
    /// "often stuck in initgroups" against LDAP. Worse here: [`ForkBroker`](super::fork_broker::ForkBroker) has exactly one thread, so
    /// one wedged child would stop every later session spawn *and* every managed-service restart,
    /// including the daemon's, while the supervisor stayed up and kept answering `ListServices`.
    pub groups: Vec<libc::gid_t>,
}

/// Look up an OS user in the host's passwd database.
pub fn resolve_target_user(name: &str) -> anyhow::Result<TargetUser> {
    let c_name = CString::new(name)
        .map_err(|_| anyhow::anyhow!("user name `{name}` contains a nul byte"))?;

    let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
    // Generous, because a user in a large directory service can have a long entry; getpwnam_r
    // reports ERANGE rather than truncating, so a too-small buffer would just be a lookup failure.
    let mut buffer = vec![0u8; 16 * 1024];
    let mut found = std::ptr::null_mut();
    // SAFETY: every pointer is to a live local, and `buffer.len()` describes `buffer` exactly.
    let code = unsafe {
        libc::getpwnam_r(
            c_name.as_ptr(),
            entry.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut found,
        )
    };
    if code != 0 || found.is_null() {
        anyhow::bail!("no OS user `{name}` on this host");
    }
    // SAFETY: `found` is non-null, so getpwnam_r filled `entry` and pointed `found` at it.
    let entry = unsafe { &*found };
    if entry.pw_dir.is_null() || entry.pw_name.is_null() {
        anyhow::bail!("OS user `{name}` has an incomplete passwd entry");
    }
    // Before anything else is read out of the entry: what the account is *called* is not what it
    // resolves to, and the ids are the part that decides how much privilege the child gets.
    refuse_root_credentials(name, entry.pw_uid, entry.pw_gid)?;
    // SAFETY: both pointers were just checked non-null and point into `buffer`, which is alive.
    let (pw_name, home) = unsafe {
        (
            CStr::from_ptr(entry.pw_name).to_owned(),
            PathBuf::from(CStr::from_ptr(entry.pw_dir).to_string_lossy().into_owned()),
        )
    };

    let groups = resolve_supplementary_groups(&pw_name, entry.pw_gid)?;

    Ok(TargetUser {
        uid: entry.pw_uid,
        gid: entry.pw_gid,
        name: pw_name,
        home,
        groups,
    })
}

/// The group list `initgroups(3)` would install for an account: its primary group plus every group
/// that lists it as a member.
///
/// Called before the fork, from a process that may safely allocate and talk to NSS. See
/// [`TargetUser::groups`] for why the child may not.
pub(super) fn resolve_supplementary_groups(
    name: &CStr,
    gid: u32,
) -> anyhow::Result<Vec<libc::gid_t>> {
    // Most accounts are in a handful of groups; the retry below covers the ones that are not.
    let mut groups: Vec<libc::gid_t> = vec![0; 32];
    loop {
        let mut count = groups.len() as libc::c_int;
        // SAFETY: `name` is a live C string, and `count` describes `groups` exactly. On overflow
        // `getgrouplist` returns -1 and writes the required length into `count` without writing past
        // the buffer it was given. The cast is between the two spellings of a 32-bit group id — see
        // [`GroupListId`] — so the buffer it writes into is the size it was told.
        let found = unsafe {
            libc::getgrouplist(
                name.as_ptr(),
                gid as GroupListId,
                groups.as_mut_ptr().cast::<GroupListId>(),
                &mut count,
            )
        };
        if found >= 0 {
            // Truncated to `count`, not to the return value: glibc returns the number of groups it
            // found, Darwin's libc returns 0 for "they fit", and both write that number into
            // `ngroups`. Reading the return value would hand the child an empty group list on one of
            // the two — the quiet downgrade the bail below exists to refuse, arrived at by another
            // route.
            groups.truncate(count as usize);
            return Ok(groups);
        }
        if count as usize <= groups.len() {
            // -1 without asking for more room is a failure to enumerate, not a small buffer. Refusing
            // is the only safe answer: a short group list is a silently *different* set of
            // permissions for the session, which is exactly the kind of quiet downgrade this
            // boundary exists to prevent.
            anyhow::bail!(
                "could not resolve the groups of OS user `{}`",
                name.to_string_lossy()
            );
        }
        groups.resize(count as usize, 0);
    }
}

/// Refuse credentials that resolve to root, whatever the account they belong to is called.
///
/// `SupervisorConfig::load` rejects the *name* `root`, and that is a different check: an account
/// aliased to uid 0 under another name (`toor`, or any second passwd entry sharing the id) passes
/// validation, and by the time the child runs there is nothing left to catch it —
/// [`privilege_to_drop`] compares effective ids, sees the supervisor already *is* uid 0, and reports
/// nothing to drop, so the child execs as real root. `Authorizer::from_service_uids` would then admit
/// uid 0 as well, contradicting the promise in `authz.rs` that root is not special-cased in.
///
/// gid 0 is refused for the same reason as uid 0: membership of the root group is read access to
/// most of what the supervisor exists to keep away from a session.
pub fn refuse_root_credentials(account: &str, uid: u32, gid: u32) -> anyhow::Result<()> {
    if uid == ROOT_UID {
        anyhow::bail!(
            "account `{account}` resolves to uid {ROOT_UID}; the supervisor is the only privileged \
             process on the host"
        );
    }
    if gid == ROOT_GID {
        anyhow::bail!(
            "account `{account}` resolves to gid {ROOT_GID}; the supervisor is the only privileged \
             process on the host"
        );
    }
    Ok(())
}

/// Look up a group in the host's group database, for a service that declares one explicitly
/// instead of taking its account's primary group.
pub fn resolve_group_gid(name: &str) -> anyhow::Result<u32> {
    let c_name = CString::new(name)
        .map_err(|_| anyhow::anyhow!("group name `{name}` contains a nul byte"))?;

    let mut entry = std::mem::MaybeUninit::<libc::group>::uninit();
    let mut buffer = vec![0u8; 16 * 1024];
    let mut found = std::ptr::null_mut();
    // SAFETY: every pointer is to a live local, and `buffer.len()` describes `buffer` exactly.
    let code = unsafe {
        libc::getgrnam_r(
            c_name.as_ptr(),
            entry.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut found,
        )
    };
    if code != 0 || found.is_null() {
        anyhow::bail!("no OS group `{name}` on this host");
    }
    // SAFETY: `found` is non-null, so getgrnam_r filled `entry` and pointed `found` at it.
    Ok(unsafe { (*found).gr_gid })
}
