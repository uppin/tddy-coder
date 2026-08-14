//! Fork/exec with a privilege drop — the one place in the stack that turns root into somebody else.
//!
//! ## Ordering is the security contract
//!
//! Everything the child does between `fork` and `exec` happens in this order, and the order is
//! load-bearing:
//!
//! 1. **tie the child's lifetime to the supervisor's** — a supervisor that is killed rather than
//!    asked to stop must not leave the daemon and every session running unsupervised on the host.
//!    Asked for first, and then *again* after every later step that clears it: `commit_creds()` sets
//!    `pdeath_signal` back to 0 on any change of effective ids or capability set, so the privilege
//!    drop in step 4 and the `unshare(CLONE_NEWUSER)` in step 6 each disarm it. Only the last arming
//!    survives into the `exec`, which is why the plan re-arms rather than arms.
//! 2. **lead its own process group** — the daemon signals sessions with `kill(-pid, …)`, so a child
//!    that stayed in the supervisor's group would make a signal aimed at one session either miss
//!    entirely or hit the supervisor and every service under it.
//! 3. **join the cgroup scope** — needs root, because the scope is owned by root. Doing it here
//!    rather than from the parent means there is no window in which the child runs outside its
//!    limits.
//! 4. **`setgroups` → `setgid` → `setuid`** — root is surrendered here and never comes back. The
//!    group list is enumerated *before* the fork, so this is three bare syscalls; `initgroups(3)`
//!    would allocate and enter NSS, neither of which is safe after a fork.
//! 5. **`chdir` into the working directory** — deliberately *after* the drop, so a caller-supplied
//!    directory is traversed with the target user's authority and not with root's.
//! 6. **namespace and mount setup** — must stay after step 4. A user namespace created by root maps
//!    the child to *real* uid 0 against the host; created by the unprivileged target it maps to the
//!    target, which is what today's rootless jail already does. The maps are therefore built from
//!    the plan's target rather than from `geteuid()`, which before the fork is still root — see
//!    [`single_id_map`]. A jail always takes a mount namespace, because that is where its mounts
//!    happen; it takes a network namespace only when it asked to be isolated from the host's network,
//!    because one it did not ask for is an *empty* network rather than a shared one.
//!
//! That order is *data*, not statements: [`pre_exec_plan`] builds it as a [`Vec<PreExecStep>`] a
//! test can assert on any host, and the child does nothing but walk the very same plan, compiled
//! into an allocation-free form ([`CompiledStep`]) before the fork. There is no second copy of the
//! ordering to drift out of step with the first.
//!
//! ## Forking from a multi-threaded process
//!
//! `fork` copies only the calling thread, so a lock another thread held at fork time stays locked
//! forever in the child. Two things keep that safe here: every fork happens on [`ForkBroker`]'s one
//! dedicated thread, never on a runtime worker; and the `pre_exec` closure below allocates nothing
//! and takes no lock — every string it needs is a [`CString`] built before the fork, and the pids
//! it writes are formatted into buffers that already exist.

use std::collections::BTreeMap;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use crate::config::{ROOT_GID, ROOT_UID};
use crate::socket::SD_LISTEN_FDS_START;

extern "C" {
    /// POSIX's environment array, which `execvp` reads at exec time. Writing it is how the child
    /// installs the environment the supervisor built for it — see [`ChildEnvironment`].
    static mut environ: *const *const libc::c_char;
}

/// What a child exits with when it discovers, between `fork` and `exec`, that the supervisor that
/// forked it is already gone.
///
/// Follows [`set_parent_death_signal`] onto Linux: it is the exit the race check there takes, and
/// there is no such check where the kernel cannot arm the signal in the first place.
#[cfg(target_os = "linux")]
const ORPHANED_EXIT_STATUS: libc::c_int = 125;

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
    /// "often stuck in initgroups" against LDAP. Worse here: [`ForkBroker`] has exactly one thread, so
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

/// A group id as `getgrouplist(3)` takes it: `gid_t` in glibc's header, `c_int` in Darwin's. Both
/// are 32 bits wide, so this is the two platforms spelling one argument differently rather than
/// passing different things.
#[cfg(target_os = "linux")]
type GroupListId = libc::gid_t;
#[cfg(not(target_os = "linux"))]
type GroupListId = libc::c_int;

/// The group list `initgroups(3)` would install for an account: its primary group plus every group
/// that lists it as a member.
///
/// Called before the fork, from a process that may safely allocate and talk to NSS. See
/// [`TargetUser::groups`] for why the child may not.
fn resolve_supplementary_groups(name: &CStr, gid: u32) -> anyhow::Result<Vec<libc::gid_t>> {
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

/// One fork/exec the supervisor performs on someone's behalf.
#[derive(Debug, Clone)]
pub struct SpawnPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub working_dir: PathBuf,
    pub target: TargetUser,
    /// `cgroup.procs` of the scope the child joins while it is still root. `None` leaves the child
    /// in the supervisor's own cgroup.
    pub scope_procs: Option<PathBuf>,
    /// Whether the child shares the supervisor's stdout/stderr. Managed services do, so their
    /// output reaches the journal; sessions do not, because their output belongs to whoever asked
    /// for them.
    pub inherit_output: bool,
    /// What the child's environment is built on top of, before `env` is applied.
    pub environment: EnvironmentBase,
    /// Jail to build around the child. `None` spawns a plain process in the host's namespaces.
    pub sandbox: Option<SandboxJail>,
}

/// Where a child's environment starts from.
///
/// The supervisor's own environment is not a neutral starting point. On a real host it is whatever the
/// unit's `Environment=` and `EnvironmentFile=` put there, which is where operators keep credentials,
/// and the supervisor is uid 0 when it reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentBase {
    /// The supervisor's own environment, verbatim.
    ///
    /// For a *declared managed service* only. Root wrote that declaration, and an operator who sets
    /// `RUST_LOG` on the supervisor unit expects it to reach the daemon exactly as it does today.
    Inherited,
    /// `PATH`, the account's own `HOME`/`USER`/`LOGNAME`, and the host's locale. Nothing else.
    ///
    /// For a session, which runs as somebody else on a caller's behalf and has no claim on anything
    /// the supervisor was started with.
    Minimal,
}

/// `PATH` for a session, whose caller does not get to choose one.
///
/// Fixed rather than taken from the supervisor's own environment, so what a session resolves a
/// command to does not depend on how the unit happened to be started. A host whose binaries live
/// somewhere else grants `PATH` through `SpawnPolicy::allowed_env_keys`, which puts the decision in
/// the root-owned policy file where the rest of them are.
const SESSION_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

/// Variables a minimal environment still takes from the supervisor: they decide how text is rendered
/// and what the clock reads, not what code runs.
const FORWARDED_VARIABLES: [&str; 2] = ["LANG", "TZ"];

/// The jail the supervisor builds around a sandbox session, once it has dropped to the target user.
///
/// Every path here has already been resolved against `SpawnPolicy::allowed_mount_roots`; the child
/// never sees a source the policy did not permit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SandboxJail {
    /// Bind mounts to establish inside the jail, in declaration order.
    pub mounts: Vec<JailMount>,
    /// Whether the jail gets its own network namespace with only loopback up.
    pub isolate_network: bool,
}

/// One bind mount inside a jail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JailMount {
    pub source: PathBuf,
    pub target: PathBuf,
    pub readonly: bool,
}

/// One step the child performs between `fork` and `exec`, in the order it must happen.
///
/// This exists as data rather than as straight-line code because the *ordering* is the security
/// contract, not an implementation detail: the privilege drop has to come after the operations that
/// need root and before every operation that must not have it. Ordering expressed as a value can be
/// asserted by a test on any host; ordering expressed as statements inside `pre_exec` cannot be
/// asserted at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreExecStep {
    /// Ask the kernel to signal this child if the supervisor dies, so a killed supervisor does not
    /// leave sessions reparented to init.
    ///
    /// Appears more than once in a plan, and has to: `commit_creds()` sets `pdeath_signal` back to 0
    /// on any change of effective ids or capability set, so both [`Self::DropPrivilege`] and the
    /// `unshare(CLONE_NEWUSER)` in [`Self::EnterUserNamespace`] silently disarm it. Each is therefore
    /// followed by another arming, and the last one is what the child carries through the `exec`.
    SetParentDeathSignal,
    /// `setsid`, making the child the leader of its own session and process group.
    ///
    /// Callers address a session with `kill(-pid, …)`. A child left in the supervisor's group turns
    /// that into either a miss or a signal to the supervisor and everything it manages.
    LeadOwnProcessGroup,
    /// Write our pid into a scope's `cgroup.procs`. Needs root: the scope is root-owned.
    JoinCgroupScope { procs: PathBuf },
    /// `setgroups` → `setgid` → `setuid`. Root is surrendered here and never comes back.
    DropPrivilege { uid: u32, gid: u32 },
    /// `unshare(CLONE_NEWUSER)`, the uid/gid maps and `setgroups=deny`.
    EnterUserNamespace,
    /// `unshare(CLONE_NEWNS)`. Every jail takes one: the private remount and the bind mounts below
    /// have nowhere else to happen.
    EnterMountNamespace,
    /// `unshare(CLONE_NEWNET)`. Only for a jail that asked to be isolated from the host's network —
    /// a namespace nobody asked for is an *empty* network, not a shared one.
    EnterNetworkNamespace,
    /// `mount(/, MS_REC | MS_PRIVATE)`, so later mounts do not propagate to the host.
    MakeRootMountPrivate,
    /// One bind mount inside the jail.
    BindMount {
        source: PathBuf,
        target: PathBuf,
        readonly: bool,
    },
    /// Bring `lo` up inside the new network namespace.
    BringLoopbackUp,
    /// `chdir` into the working directory.
    ChangeDirectory { path: PathBuf },
}

/// The ordered steps a child performs between `fork` and `exec`. Pure — builds no OS state.
///
/// This is the only place the order is decided. [`PreExecSteps`] executes exactly this sequence,
/// so what a test asserts here is what a child does.
pub fn pre_exec_plan(plan: &SpawnPlan) -> Vec<PreExecStep> {
    let mut steps = vec![
        // First, because every later step can fail, and a child that dies before asking for the
        // parent-death signal is a child that could outlive its supervisor.
        PreExecStep::SetParentDeathSignal,
        PreExecStep::LeadOwnProcessGroup,
    ];

    // While still root: the scope directory is root-owned, and joining it here rather than from the
    // parent leaves no window in which the child runs outside its limits.
    if let Some(procs) = &plan.scope_procs {
        steps.push(PreExecStep::JoinCgroupScope {
            procs: procs.clone(),
        });
    }

    if let Some((uid, gid)) = privilege_to_drop(&plan.target) {
        steps.push(PreExecStep::DropPrivilege { uid, gid });
        // The kernel clears `pdeath_signal` on every change of effective credentials, so the arming
        // above is now gone — see [`PreExecStep::SetParentDeathSignal`].
        steps.push(PreExecStep::SetParentDeathSignal);
    }

    // After the drop, so a caller-supplied directory is traversed with the target user's authority
    // and not with root's.
    steps.push(PreExecStep::ChangeDirectory {
        path: plan.working_dir.clone(),
    });

    if let Some(jail) = &plan.sandbox {
        // Every namespace is created after the drop, so it is owned by the unprivileged target user
        // and its uid map is the same rootless map the daemon builds today.
        steps.push(PreExecStep::EnterUserNamespace);
        // The `unshare` above hands the child a full capability set inside its new namespace, which is
        // another change of credentials, and so clears the signal again.
        steps.push(PreExecStep::SetParentDeathSignal);
        // Whatever the jail asked for, the mount namespace is unconditional: the private remount and
        // the bind mounts below have nowhere else to happen. The *network* namespace is not, because
        // a jail that asked to share the host's network and got one of its own would get an empty
        // network with its loopback down — worse than either answer the request can ask for.
        steps.push(PreExecStep::EnterMountNamespace);
        if jail.isolate_network {
            steps.push(PreExecStep::EnterNetworkNamespace);
        }
        // Before any bind mount, or every mount below propagates back out to the host.
        steps.push(PreExecStep::MakeRootMountPrivate);
        for mount in &jail.mounts {
            steps.push(PreExecStep::BindMount {
                source: mount.source.clone(),
                target: mount.target.clone(),
                readonly: mount.readonly,
            });
        }
        if jail.isolate_network {
            // Inside the network namespace unshared above, so this configures the jail's loopback and
            // not the host's.
            steps.push(PreExecStep::BringLoopbackUp);
        }
    }

    steps
}

/// The credentials to drop to, or `None` when the supervisor already runs as the target.
///
/// Not a special case for tests: a supervisor started as the same account a service declares has no
/// privilege to give up, and `initgroups` would fail with `EPERM` for lack of it. The comparison is
/// against the *effective* ids, because those are what `setgid`/`setuid` would change.
fn privilege_to_drop(target: &TargetUser) -> Option<(u32, u32)> {
    // SAFETY: both accessors only read this process's own credentials.
    let (euid, egid) = unsafe { (libc::geteuid(), libc::getegid()) };
    if euid == target.uid && egid == target.gid {
        return None;
    }
    Some((target.uid, target.gid))
}

/// Owns the one thread in the process that is allowed to `fork`.
///
/// Two reasons it is a thread of its own rather than `tokio::task::spawn_blocking`:
///
/// * A runtime worker may be holding an allocator lock on behalf of another task, and that lock
///   would stay held forever in the forked child.
/// * `PR_SET_PDEATHSIG` is keyed to the *thread* that forked, not to the process. Tokio retires
///   idle blocking threads after a few seconds, which would tell every child its parent had died
///   while the supervisor was still running. A thread that lives as long as the supervisor makes
///   the signal mean what it says.
pub struct ForkBroker {
    jobs: mpsc::UnboundedSender<Job>,
}

type Job = (
    SpawnPlan,
    Option<SocketHandover>,
    oneshot::Sender<std::io::Result<u32>>,
);

impl ForkBroker {
    /// Start the fork thread. It runs until the last [`ForkBroker`] is dropped, which happens when
    /// the supervisor is on its way out.
    pub fn start() -> std::io::Result<ForkBroker> {
        // Taken before the thread exists and held for as long as it does — see the function's docs.
        let handover_slot = reserve_handover_slot()?;
        let (jobs, mut queue) = mpsc::unbounded_channel::<Job>();
        std::thread::Builder::new()
            .name("tddy-supervisor-fork".to_string())
            .spawn(move || {
                let _handover_slot = handover_slot;
                while let Some((plan, handover, reply)) = queue.blocking_recv() {
                    let _ = reply.send(spawn_now(plan, handover));
                }
            })?;
        Ok(ForkBroker { jobs })
    }

    /// Fork, drop privilege, and exec the plan on the fork thread. Returns the child's pid.
    ///
    /// `handover` is the listening socket the child should find at [`SD_LISTEN_FDS_START`]. It is
    /// not part of the [`SpawnPlan`] because it is a live descriptor the supervisor owns, not a
    /// decision about the child.
    pub async fn spawn(
        &self,
        plan: SpawnPlan,
        handover: Option<SocketHandover>,
    ) -> std::io::Result<u32> {
        let (reply, answer) = oneshot::channel();
        self.jobs
            .send((plan, handover, reply))
            .map_err(|_| std::io::Error::other("the fork thread has stopped"))?;
        answer
            .await
            .map_err(|_| std::io::Error::other("the fork thread dropped the request"))?
    }
}

/// A listening socket the supervisor created for a managed service, ready to be handed over.
///
/// The supervisor keeps the listener for as long as it runs, so a restarted service is handed the
/// same one. Rebinding per start would unlink and recreate the socket node, and a client that
/// connected in that gap would get `ECONNREFUSED` on a path that is about to work again; holding the
/// listener keeps the kernel's accept queue instead, so those connections simply wait. It is the
/// same bargain systemd's socket activation makes.
#[derive(Debug, Clone)]
pub struct SocketHandover {
    listener: Arc<UnixListener>,
}

impl SocketHandover {
    pub fn new(listener: Arc<UnixListener>) -> SocketHandover {
        SocketHandover { listener }
    }

    /// The descriptor to place at [`SD_LISTEN_FDS_START`] in the child. Borrowed, never owned: the
    /// listener outlives every child that is handed it.
    fn raw_fd(&self) -> RawFd {
        self.listener.as_raw_fd()
    }
}

/// Make sure descriptor [`SD_LISTEN_FDS_START`] is occupied, so nothing the standard library opens
/// can land on it.
///
/// A declared listener reaches the child with `dup2`, which silently replaces whatever is already
/// there. `Command::spawn` reports exec failures over a socket pair it opens just before the fork,
/// and if that descriptor were the handover slot then an exec that failed would look to the
/// supervisor like one that succeeded. The kernel hands out the lowest free descriptor, so occupying
/// the slot from startup is what makes that impossible.
///
/// Returns `None` when something already occupies it — the tokio runtime's epoll descriptor normally
/// does — because the invariant is then already satisfied.
fn reserve_handover_slot() -> std::io::Result<Option<OwnedFd>> {
    // SAFETY: `F_GETFD` only reads a descriptor's flags, and reports `EBADF` for a free one.
    if unsafe { libc::fcntl(SD_LISTEN_FDS_START, libc::F_GETFD) } >= 0 {
        return Ok(None);
    }
    // SAFETY: the path is a literal C string; the call returns a descriptor nothing else owns.
    let opened = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
    if opened < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `opened` was just returned by `open` and is not owned by anything else.
    Ok(Some(unsafe { OwnedFd::from_raw_fd(opened) }))
}

fn spawn_now(plan: SpawnPlan, handover: Option<SocketHandover>) -> std::io::Result<u32> {
    let mut command = Command::new(&plan.program);
    command.args(&plan.args);
    command.stdin(Stdio::null());
    if !plan.inherit_output {
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
    }
    // Deliberately no `Command::env` anywhere above: `Command` installs its own environment *after*
    // the `pre_exec` closure has run, which would discard the one variable only the child can know —
    // its own pid, in `LISTEN_PID`. The whole environment is built and installed by
    // [`ChildEnvironment`] instead.
    let mut steps = PreExecSteps::prepare(&plan, handover.as_ref())?;
    // SAFETY: the closure runs between `fork` and `exec`. It performs only async-signal-safe
    // syscalls on values captured before the fork — see this module's header for why that matters.
    unsafe {
        command.pre_exec(move || steps.run());
    }

    let child = command.spawn()?;
    Ok(child.id())
}

/// The pre-computed instructions the child executes between `fork` and `exec`.
///
/// Every field is already in its final, allocation-free form, so `run` never needs the allocator.
struct PreExecSteps {
    /// The supervisor's pid, so the child can tell whether it was orphaned before it managed to ask
    /// the kernel to signal it.
    supervisor_pid: libc::pid_t,
    /// The environment the child execs with, ready but for its own pid.
    environment: ChildEnvironment,
    /// A listener to place at [`SD_LISTEN_FDS_START`]. Owned by the supervisor, not by the plan.
    handover: Option<RawFd>,
    /// [`pre_exec_plan`]'s output, compiled. Executed in this order and no other.
    steps: Vec<CompiledStep>,
}

impl PreExecSteps {
    fn prepare(
        plan: &SpawnPlan,
        handover: Option<&SocketHandover>,
    ) -> std::io::Result<PreExecSteps> {
        let steps = pre_exec_plan(plan)
            .into_iter()
            .map(|step| CompiledStep::compile(step, plan))
            .collect::<std::io::Result<Vec<CompiledStep>>>()?;

        Ok(PreExecSteps {
            // SAFETY: reads this process's own pid.
            supervisor_pid: unsafe { libc::getpid() },
            environment: ChildEnvironment::build(plan, handover.is_some())?,
            handover: handover.map(SocketHandover::raw_fd),
            steps,
        })
    }

    fn run(&mut self) -> std::io::Result<()> {
        // The environment and the handed-over listener are not plan steps: each carries a runtime
        // value a `SpawnPlan` cannot hold — the child's own pid, a live descriptor number — and
        // neither depends on the credentials or the namespaces the plan establishes, so where they
        // sit relative to it is not a security property. Everything that *is* ordered is in the plan.
        //
        // SAFETY: writes only into memory this struct owns, plus one pointer store into `environ`.
        unsafe { self.environment.install() };
        if let Some(listener) = self.handover {
            // SAFETY: `listener` is open in this child, inherited across the fork.
            unsafe { hand_over_listener(listener) }?;
        }

        for step in &self.steps {
            step.execute(self.supervisor_pid)?;
        }
        Ok(())
    }
}

/// One [`PreExecStep`] in the form the child can execute: no owned paths left to convert, no
/// lookups left to perform, nothing that would need the allocator.
#[derive(Debug)]
enum CompiledStep {
    SetParentDeathSignal,
    LeadOwnProcessGroup,
    JoinCgroupScope {
        procs: CString,
    },
    DropPrivilege {
        uid: libc::uid_t,
        gid: libc::gid_t,
        /// The account's groups, already enumerated — see [`TargetUser::groups`].
        groups: Vec<libc::gid_t>,
    },
    ChangeDirectory {
        path: CString,
    },
    EnterUserNamespace {
        /// `0 <uid> 1\n` for `/proc/self/uid_map` — see [`single_id_map`].
        uid_map: Box<[u8]>,
        /// `0 <gid> 1\n` for `/proc/self/gid_map`.
        gid_map: Box<[u8]>,
    },
    EnterMountNamespace,
    EnterNetworkNamespace,
    MakeRootMountPrivate,
    BindMount {
        /// The source path, which the child resolves to a descriptor of its own before binding it —
        /// see [`bind_mount`].
        source: CString,
        target: CString,
        readonly: bool,
    },
    BringLoopbackUp,
}

impl CompiledStep {
    /// Translate one planned step into its executable form, before the fork.
    fn compile(step: PreExecStep, plan: &SpawnPlan) -> std::io::Result<CompiledStep> {
        match step {
            PreExecStep::SetParentDeathSignal => Ok(CompiledStep::SetParentDeathSignal),
            PreExecStep::LeadOwnProcessGroup => Ok(CompiledStep::LeadOwnProcessGroup),
            PreExecStep::JoinCgroupScope { procs } => Ok(CompiledStep::JoinCgroupScope {
                procs: path_to_c_string(&procs)?,
            }),
            PreExecStep::DropPrivilege { uid, gid } => Ok(CompiledStep::DropPrivilege {
                uid,
                gid,
                groups: plan.target.groups.clone(),
            }),
            PreExecStep::ChangeDirectory { path } => Ok(CompiledStep::ChangeDirectory {
                path: path_to_c_string(&path)?,
            }),
            PreExecStep::EnterUserNamespace => Ok(CompiledStep::EnterUserNamespace {
                uid_map: single_id_map(plan.target.uid),
                gid_map: single_id_map(plan.target.gid),
            }),
            PreExecStep::EnterMountNamespace => Ok(CompiledStep::EnterMountNamespace),
            PreExecStep::EnterNetworkNamespace => Ok(CompiledStep::EnterNetworkNamespace),
            PreExecStep::MakeRootMountPrivate => Ok(CompiledStep::MakeRootMountPrivate),
            PreExecStep::BindMount {
                source,
                target,
                readonly,
            } => compile_bind_mount(&source, &target, readonly),
            PreExecStep::BringLoopbackUp => Ok(CompiledStep::BringLoopbackUp),
        }
    }

    fn execute(&self, supervisor_pid: libc::pid_t) -> std::io::Result<()> {
        // SAFETY: every call below runs after `fork` and touches only this process's own
        // credentials, working directory, process group, descriptors, or the namespaces it has just
        // created for itself. Each `CString` and buffer is owned by this step and outlives the call.
        unsafe {
            match self {
                CompiledStep::SetParentDeathSignal => set_parent_death_signal(supervisor_pid),
                CompiledStep::LeadOwnProcessGroup => lead_own_process_group(),
                CompiledStep::JoinCgroupScope { procs } => join_cgroup_scope(procs),
                CompiledStep::DropPrivilege { uid, gid, groups } => {
                    drop_privilege(*uid, *gid, groups)
                }
                CompiledStep::ChangeDirectory { path } => change_directory(path),
                CompiledStep::EnterUserNamespace { uid_map, gid_map } => {
                    enter_user_namespace(uid_map, gid_map)
                }
                CompiledStep::EnterMountNamespace => enter_mount_namespace(),
                CompiledStep::EnterNetworkNamespace => enter_network_namespace(),
                CompiledStep::MakeRootMountPrivate => make_root_mount_private(),
                CompiledStep::BindMount {
                    source,
                    target,
                    readonly,
                } => bind_mount(source, target, *readonly),
                CompiledStep::BringLoopbackUp => bring_loopback_up(),
            }
        }
    }
}

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
fn only_on_linux(facility: &str) -> std::io::Error {
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
unsafe fn set_parent_death_signal(supervisor_pid: libc::pid_t) -> std::io::Result<()> {
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
unsafe fn lead_own_process_group() -> std::io::Result<()> {
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
unsafe fn drop_privilege(
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
unsafe fn change_directory(path: &CStr) -> std::io::Result<()> {
    if libc::chdir(path.as_ptr()) != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// The map that makes a jail rootless: one id, the account the child has already become, mapped to
/// root *inside* the new namespace and to nothing at all outside it.
///
/// Built from [`SpawnPlan::target`] and never from `geteuid()`, and that is the security property of
/// the whole feature rather than a detail. `compile` runs before the fork, where the effective uid is
/// still the supervisor's 0; a map written from it would name real host root as the child's outside
/// identity, which is precisely what putting the namespace steps after `DropPrivilege` exists to
/// prevent. The target *is* the effective id at the moment the step runs: either `DropPrivilege`
/// preceded it, or [`privilege_to_drop`] found nothing to drop because the supervisor already runs as
/// the target.
///
/// A single mapped id is also all the kernel will accept from an unprivileged writer — see
/// [`enter_user_namespace`]. Which is the other reason the value cannot be read in the child: by then
/// the `unshare` has happened, and an id read *inside* an unmapped namespace is the overflow uid
/// (65534), which the kernel refuses to accept as a mapping with `EPERM`.
fn single_id_map(id: u32) -> Box<[u8]> {
    format!("0 {id} 1\n").into_bytes().into_boxed_slice()
}

/// Refuse a bind mount whose source cannot be resolved, before a process exists to fail.
///
/// The resolution the *bind* uses happens in the child ([`bind_mount`]) and cannot happen here: a
/// descriptor opened before `unshare(CLONE_NEWNS)` belongs to the mount namespace the child was
/// cloned from, and `mount(2)` refuses a bind source from another namespace with `EINVAL`. So this is
/// the same check made twice, deliberately — the habit `resolve_session_user` follows in refusing
/// `root` that the loader already refused — and it earns its place twice over:
///
/// * the child cannot explain itself. `std` carries a `pre_exec` failure back to the parent as an
///   errno and nothing else, so a symlinked or absent source would reach an operator as
///   `No such file or directory` with no mention of which of a session's mounts it was.
/// * a request that is already unbindable does not cost a fork.
fn compile_bind_mount(
    source: &Path,
    target: &Path,
    readonly: bool,
) -> std::io::Result<CompiledStep> {
    let source_path = path_to_c_string(source)?;
    // The descriptor is dropped with the statement: it is the answer that matters here, not the
    // reference — the child opens one the mount will accept.
    open_without_following_symlinks(&source_path)
        .map_err(|error| unresolvable_mount_source(source, &error))?;
    Ok(CompiledStep::BindMount {
        source: source_path,
        target: path_to_c_string(target)?,
        readonly,
    })
}

/// Why a bind mount source could not be pinned to one object, in terms an operator can act on.
///
/// Every outcome refuses the spawn. `ENOSYS` is named because it is not a bad request but a kernel
/// with no `openat2(2)` (Linux 5.6) to ask, and `Function not implemented` against a path would tell
/// nobody that. There is no path-based check to fall back to — that check *is* the race.
fn unresolvable_mount_source(source: &Path, error: &std::io::Error) -> std::io::Error {
    if error.raw_os_error() == Some(libc::ENOSYS) {
        return std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!(
                "this host has no openat2(2) (Linux 5.6 or newer), so bind mount source {} cannot \
                 be resolved without a symlink race; the supervisor will not spawn a session it \
                 cannot isolate",
                source.display()
            ),
        );
    }
    std::io::Error::new(
        error.kind(),
        format!(
            "bind mount source {} could not be resolved (symlinks are not followed): {error}",
            source.display()
        ),
    )
}

/// `openat2(2)`'s resolution flags, named here rather than reached for at each call site because
/// they are the one thing in this file the two platforms cannot both spell: off Linux there is no
/// `openat2` to pass them to, and [`open_resolved`] refuses before it would read them.
#[cfg(target_os = "linux")]
pub(crate) const RESOLVE_NO_SYMLINKS: u64 = libc::RESOLVE_NO_SYMLINKS;
#[cfg(not(target_os = "linux"))]
pub(crate) const RESOLVE_NO_SYMLINKS: u64 = 0;
#[cfg(target_os = "linux")]
pub(crate) const RESOLVE_BENEATH: u64 = libc::RESOLVE_BENEATH;
#[cfg(not(target_os = "linux"))]
pub(crate) const RESOLVE_BENEATH: u64 = 0;

/// Open an absolute path as a bare reference, refusing to traverse a single symlink on the way.
fn open_without_following_symlinks(path: &CStr) -> std::io::Result<OwnedFd> {
    open_resolved(libc::AT_FDCWD, path, RESOLVE_NO_SYMLINKS)
}

/// Open a directory to resolve other paths against.
///
/// Symlinks *are* followed here, deliberately: the only caller passes a mount root out of the
/// root-owned policy file, which is root describing its own filesystem — the same trust
/// `policy::resolve_tool_path` places in an allowlisted tool path. What must not be followed is
/// anything below it, and that is [`open_resolved`]'s job.
#[cfg(target_os = "linux")]
pub(crate) fn open_directory_reference(path: &CStr) -> std::io::Result<OwnedFd> {
    // SAFETY: `path` is a live C string; the call returns a descriptor nothing else owns.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `open` just returned `fd` and nothing else owns it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// `O_PATH` is Linux's, and the descriptor exists only to be [`open_resolved`]'s starting point,
/// which off Linux there is no syscall to perform — see [`only_on_linux`]. Opening the directory
/// some other way would hand back a reference the resolution below can do nothing with.
#[cfg(not(target_os = "linux"))]
pub(crate) fn open_directory_reference(path: &CStr) -> std::io::Result<OwnedFd> {
    let _ = path;
    Err(only_on_linux("an O_PATH directory reference"))
}

/// `openat2(2)` under the given `RESOLVE_*` flags, as an `O_PATH` reference.
///
/// A raw `syscall` because `libc` carries `open_how` and `SYS_openat2` but no wrapper, and because a
/// kernel older than 5.6 has to report `ENOSYS` to its caller rather than have the supervisor guess
/// at containment. `openat2` rather than `open`: it is the only one that can be told not to follow
/// symlinks in the *intermediate* components of a path.
///
/// `O_PATH` because the descriptor is only ever used as a name (`/proc/self/fd/<n>`): it needs no
/// read permission on the object, does not block on a fifo, and does not open a device.
///
/// Safe to call after a `fork`: one syscall over a stack-allocated struct, no allocation and no lock.
#[cfg(target_os = "linux")]
pub(crate) fn open_resolved(dirfd: RawFd, path: &CStr, resolve: u64) -> std::io::Result<OwnedFd> {
    // SAFETY: `open_how` is plain data; zeroing initialises every field the kernel may read.
    let mut how: libc::open_how = unsafe { std::mem::zeroed() };
    how.flags = (libc::O_PATH | libc::O_CLOEXEC) as u64;
    how.resolve = resolve;
    // SAFETY: `path` is a live C string and `how` a live struct whose size is passed explicitly. A
    // kernel without `openat2` reports `ENOSYS` without reading either.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            dirfd as libc::c_long,
            path.as_ptr(),
            &how as *const libc::open_how,
            std::mem::size_of::<libc::open_how>() as libc::c_long,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `openat2` just returned `fd` and nothing else owns it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd as RawFd) })
}

/// `openat2(2)` is Linux 5.6's and has no counterpart elsewhere — see [`only_on_linux`]. Every
/// caller reads this the way it reads a kernel too old to have the syscall, which is the same answer
/// for the same reason: the path-based check that would stand in for it *is* the symlink race the
/// resolution exists to remove, so a source that cannot be shown to be contained is not granted.
#[cfg(not(target_os = "linux"))]
pub(crate) fn open_resolved(dirfd: RawFd, path: &CStr, resolve: u64) -> std::io::Result<OwnedFd> {
    let _ = (dirfd, path, resolve);
    Err(only_on_linux("openat2(2) path resolution"))
}

/// The files a fresh user namespace is configured through, and the switch that has to be thrown
/// between them.
#[cfg(target_os = "linux")]
const UID_MAP_PATH: &CStr = c"/proc/self/uid_map";
#[cfg(target_os = "linux")]
const SETGROUPS_PATH: &CStr = c"/proc/self/setgroups";
#[cfg(target_os = "linux")]
const GID_MAP_PATH: &CStr = c"/proc/self/gid_map";
#[cfg(target_os = "linux")]
const SETGROUPS_DENY: &[u8] = b"deny";

/// Enter a user namespace owned by the account the child has already become.
///
/// Three details are load-bearing, in this order:
///
/// * **`unshare` first, then the maps.** `map_write` requires the *opener* of `uid_map` to hold
///   `CAP_SYS_ADMIN` over the namespace being configured; a descriptor opened before the `unshare`
///   carries the credentials of a process that had no such namespace, so the write is refused.
/// * **`PR_SET_DUMPABLE` before opening anything under `/proc/self`.** The `setuid` in
///   [`drop_privilege`] left this process non-dumpable: the kernel does that on any change of
///   effective ids (`fs.suid_dumpable` is 0 or 2 on an ordinary host), and it then reports every
///   `/proc/<pid>` inode as owned by root. `uid_map` is mode 0644, so without this the child gets
///   `EACCES` opening its own map — measured, not inferred. Restoring dumpability weakens nothing:
///   the `execve` moments later sets it for this process anyway, because the tool is not setuid.
/// * **`setgroups=deny` before `gid_map`.** Since Linux 3.19 an unprivileged writer must give up
///   `setgroups(2)` inside the namespace before a single-id `gid_map` is accepted — a jail that kept
///   the ability to re-add supplementary groups would carry group access the drop surrendered. Not
///   best-effort: a jail that could not be closed is not a jail.
///
/// # Safety
///
/// Runs after `fork`, so only async-signal-safe syscalls: `unshare`, `prctl` and small `/proc`
/// writes. `uid_map` and `gid_map` must remain valid for the call.
#[cfg(target_os = "linux")]
unsafe fn enter_user_namespace(uid_map: &[u8], gid_map: &[u8]) -> std::io::Result<()> {
    if libc::unshare(libc::CLONE_NEWUSER) != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if libc::prctl(libc::PR_SET_DUMPABLE, 1 as libc::c_ulong) != 0 {
        return Err(std::io::Error::last_os_error());
    }
    write_small_file(UID_MAP_PATH, uid_map)?;
    write_small_file(SETGROUPS_PATH, SETGROUPS_DENY)?;
    write_small_file(GID_MAP_PATH, gid_map)?;
    Ok(())
}

/// User namespaces, and the `/proc/self` map files that configure them, are Linux's — see
/// [`only_on_linux`].
#[cfg(not(target_os = "linux"))]
unsafe fn enter_user_namespace(uid_map: &[u8], gid_map: &[u8]) -> std::io::Result<()> {
    let _ = (uid_map, gid_map);
    Err(only_on_linux("a user namespace"))
}

/// Write a buffer to a file in one call, allocation-free.
///
/// # Safety
///
/// Runs after `fork`. `path` and `bytes` must remain valid for the call; `open`, `write` and `close`
/// are async-signal-safe, and `std::io::Error` for an errno holds the code inline rather than
/// allocating.
///
/// Only [`enter_user_namespace`] writes these, so it follows that step onto Linux.
#[cfg(target_os = "linux")]
unsafe fn write_small_file(path: &CStr, bytes: &[u8]) -> std::io::Result<()> {
    let fd = libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let written = libc::write(fd, bytes.as_ptr().cast(), bytes.len());
    // Read before `close`, which is free to overwrite `errno` on its own way out.
    let failure = (written < 0).then(std::io::Error::last_os_error);
    libc::close(fd);
    if let Some(failure) = failure {
        return Err(failure);
    }
    if written as usize != bytes.len() {
        // A half-written map file is a half-built namespace, which is not a namespace anybody asked
        // for. `/proc` takes these in one write or not at all.
        return Err(std::io::Error::from(std::io::ErrorKind::WriteZero));
    }
    Ok(())
}

/// Take a mount table of our own, so the jail can be built without rearranging the host's.
///
/// # Safety
///
/// Runs after `fork`, and after [`enter_user_namespace`] — `CLONE_NEWNS` needs `CAP_SYS_ADMIN`, which
/// the child has only inside the user namespace it just created. `unshare` changes nothing outside
/// this process.
#[cfg(target_os = "linux")]
unsafe fn enter_mount_namespace() -> std::io::Result<()> {
    if libc::unshare(libc::CLONE_NEWNS) != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Mount namespaces are Linux's — see [`only_on_linux`].
#[cfg(not(target_os = "linux"))]
unsafe fn enter_mount_namespace() -> std::io::Result<()> {
    Err(only_on_linux("a mount namespace"))
}

/// Take an empty network of our own.
///
/// Separate from [`enter_mount_namespace`] because a jail chooses these independently: one that asked
/// to share the host's network keeps it, and unsharing anyway would leave it with an empty network and
/// a loopback that is down — the opposite of what it asked for.
///
/// # Safety
///
/// Runs after `fork`, and after [`enter_user_namespace`] — `CLONE_NEWNET` needs `CAP_SYS_ADMIN`, which
/// the child has only inside the user namespace it just created. `unshare` changes nothing outside
/// this process.
#[cfg(target_os = "linux")]
unsafe fn enter_network_namespace() -> std::io::Result<()> {
    if libc::unshare(libc::CLONE_NEWNET) != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Network namespaces are Linux's — see [`only_on_linux`].
#[cfg(not(target_os = "linux"))]
unsafe fn enter_network_namespace() -> std::io::Result<()> {
    Err(only_on_linux("a network namespace"))
}

/// Stop this mount namespace from propagating anything back to the host.
///
/// Without it every bind mount below would be shared with the namespace we were cloned from — the
/// host's — so building the jail would rearrange the host's filesystem.
///
/// # Safety
///
/// Runs after `fork`, inside the mount namespace [`enter_mount_namespace`] created. The
/// path is a literal C string.
#[cfg(target_os = "linux")]
unsafe fn make_root_mount_private() -> std::io::Result<()> {
    if libc::mount(
        std::ptr::null(),
        c"/".as_ptr(),
        std::ptr::null(),
        libc::MS_REC | libc::MS_PRIVATE,
        std::ptr::null(),
    ) != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Mount propagation is a property of a Linux mount namespace — see [`only_on_linux`].
#[cfg(not(target_os = "linux"))]
unsafe fn make_root_mount_private() -> std::io::Result<()> {
    Err(only_on_linux("a private root mount"))
}

/// `/proc/self/fd/`, plus room for every digit of a descriptor and the nul.
///
/// Follows [`bind_mount`], its only caller, onto Linux — `/proc` is where this name resolves and a
/// bind mount is what reads it.
#[cfg(target_os = "linux")]
const DESCRIPTOR_PATH_PREFIX: &[u8] = b"/proc/self/fd/";
#[cfg(target_os = "linux")]
const DESCRIPTOR_DIGITS: usize = 10;
#[cfg(target_os = "linux")]
const DESCRIPTOR_PATH_LEN: usize = DESCRIPTOR_PATH_PREFIX.len() + DESCRIPTOR_DIGITS + 1;

/// Name a descriptor as a path, in a buffer the caller already owns.
///
/// `mount(2)` takes its source as a path, and this is the path of the descriptor itself. Formatted by
/// hand rather than with `format!` because the only caller runs after `fork`, where there is no
/// allocator — the bargain [`join_cgroup_scope`] and [`write_listen_pid`] make with the same problem.
#[cfg(target_os = "linux")]
fn write_descriptor_path(fd: RawFd, buffer: &mut [u8; DESCRIPTOR_PATH_LEN]) {
    buffer[..DESCRIPTOR_PATH_PREFIX.len()].copy_from_slice(DESCRIPTOR_PATH_PREFIX);
    let mut digits = [0u8; DESCRIPTOR_DIGITS];
    let mut written = 0;
    let mut remaining = fd as u32;
    loop {
        digits[written] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
        written += 1;
        if remaining == 0 {
            break;
        }
    }
    let start = DESCRIPTOR_PATH_PREFIX.len();
    for (offset, digit) in digits[..written].iter().rev().enumerate() {
        buffer[start + offset] = *digit;
    }
    buffer[start + written] = 0;
}

/// Bind one source into the jail, and mark it read-only if that is what was asked for.
///
/// The source is resolved *here*, and the mount names the descriptor rather than the path, for two
/// reasons that both point the same way:
///
/// * **`RESOLVE_NO_SYMLINKS` and the bind are the same object.** A bind mount follows symlinks in its
///   source, and `SpawnPolicy::allowed_mount_roots` are prefixes over trees session users write into,
///   so a source approved by path and a source walked by `mount(2)` are two resolutions with a window
///   between them. There is no window between an `openat2` and the `mount` that names its descriptor.
/// * **it cannot be done any earlier.** A descriptor opened before `unshare(CLONE_NEWNS)` belongs to
///   the mount namespace the child was cloned from, and the kernel refuses a bind source from another
///   namespace with `EINVAL`. Resolving in the jail also means the walk happens with the target user's
///   authority rather than root's, which is the same argument that puts `chdir` after the drop.
///
/// A second `mount` is what makes a bind read-only: `MS_RDONLY` is a property of a mount rather than
/// of a bind, and passing it to the first call is silently ignored.
///
/// TODO(supervisor/jail): that remount marks this mount read-only but not the submounts `MS_REC`
/// brought with it. Recursive read-only needs `mount_setattr(AT_RECURSIVE)` (Linux 5.12); until then
/// this matches the daemon's own jail (`tddy_sandbox_cgroups::apply_bind_mounts`).
///
/// # Safety
///
/// Runs after `fork`, inside the private mount namespace [`make_root_mount_private`] prepared. Both C
/// strings must remain valid for the call. `openat2` allocates nothing and takes no lock, and the
/// descriptor it returns is closed on the way out of this function — before the `exec`, which is the
/// only thing that would otherwise close it.
#[cfg(target_os = "linux")]
unsafe fn bind_mount(source: &CStr, target: &CStr, readonly: bool) -> std::io::Result<()> {
    let source_fd = open_without_following_symlinks(source)?;
    let mut source_link = [0u8; DESCRIPTOR_PATH_LEN];
    write_descriptor_path(source_fd.as_raw_fd(), &mut source_link);

    if libc::mount(
        source_link.as_ptr().cast(),
        target.as_ptr(),
        std::ptr::null(),
        libc::MS_BIND | libc::MS_REC,
        std::ptr::null(),
    ) != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    if !readonly {
        return Ok(());
    }
    if libc::mount(
        std::ptr::null(),
        target.as_ptr(),
        std::ptr::null(),
        libc::MS_BIND | libc::MS_REMOUNT | libc::MS_RDONLY,
        std::ptr::null(),
    ) != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// A bind mount is a Linux `mount(2)` flag — see [`only_on_linux`]. `compile_bind_mount` has already
/// refused the same plan before the fork, because [`open_resolved`] cannot resolve the source there
/// either; this is the second half of the same refusal.
#[cfg(not(target_os = "linux"))]
unsafe fn bind_mount(source: &CStr, target: &CStr, readonly: bool) -> std::io::Result<()> {
    let _ = (source, target, readonly);
    Err(only_on_linux("a bind mount"))
}

/// Bring `lo` up inside the jail's own network namespace, so a loopback-only session can still talk
/// to itself.
///
/// The `SIOCSIFFLAGS` ioctl rather than the `ip` binary: there is nothing to guarantee one exists,
/// and this runs between `fork` and `exec` where spawning anything is out of the question. Same
/// mechanism as `tddy_sandbox_cgroups::bring_loopback_up`.
///
/// # Safety
///
/// Runs after `fork`, inside the network namespace [`enter_network_namespace`] created.
/// `socket`, `ioctl` and `close` touch nothing outside this process, and the request struct is a
/// zeroed local.
#[cfg(target_os = "linux")]
unsafe fn bring_loopback_up() -> std::io::Result<()> {
    /// `struct ifreq` as `SIOCGIFFLAGS`/`SIOCSIFFLAGS` read it: a name, then flags in a union whose
    /// remaining bytes the kernel ignores.
    #[repr(C)]
    struct InterfaceRequest {
        name: [libc::c_char; libc::IF_NAMESIZE],
        flags: libc::c_short,
        _rest: [u8; 22],
    }

    let socket = libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0);
    if socket < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut request: InterfaceRequest = std::mem::zeroed();
    for (slot, byte) in request.name.iter_mut().zip(b"lo") {
        *slot = *byte as libc::c_char;
    }

    // Read the current flags rather than assuming them: `SIOCSIFFLAGS` writes the whole set, so a
    // fabricated one would clear flags the kernel put there.
    let read = libc::ioctl(socket, libc::SIOCGIFFLAGS, &mut request);
    let result = if read < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        request.flags |= (libc::IFF_UP | libc::IFF_RUNNING) as libc::c_short;
        if libc::ioctl(socket, libc::SIOCSIFFLAGS, &request) < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    };
    libc::close(socket);
    result
}

/// The interface this configures exists only inside the network namespace
/// [`enter_network_namespace`] would have created, and off Linux there is none — see
/// [`only_on_linux`]. The step is also reached only from a plan that took that namespace, so this is
/// unreachable in practice; it refuses rather than succeeds so that stays true if it ever is not.
#[cfg(not(target_os = "linux"))]
unsafe fn bring_loopback_up() -> std::io::Result<()> {
    Err(only_on_linux("a network namespace's loopback"))
}

/// Put a listener the supervisor created where an activated service looks for it.
///
/// # Safety
///
/// Runs after `fork`. `listener` must be an open descriptor in this process. Only this process's own
/// descriptor table is touched.
unsafe fn hand_over_listener(listener: RawFd) -> std::io::Result<()> {
    if listener == SD_LISTEN_FDS_START {
        // `dup2` onto the same descriptor is a no-op that would leave `FD_CLOEXEC` set, closing the
        // listener at exec. This is the one case that needs the flag cleared by hand.
        if libc::fcntl(listener, libc::F_SETFD, 0) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        return Ok(());
    }
    // `dup2` leaves `FD_CLOEXEC` clear on the new descriptor, which is what carries the listener
    // through the exec; the inherited copy keeps the flag and closes itself.
    if libc::dup2(listener, SD_LISTEN_FDS_START) < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// `LISTEN_PID=`, the one variable whose value the child has to fill in for itself.
const LISTEN_PID_PREFIX: &[u8] = b"LISTEN_PID=";

/// Room for every digit of a `pid_t`.
const LISTEN_PID_DIGITS: usize = 10;

/// Environment variables describing a socket activation. Removed from every child's environment
/// before anything is added back, because they only ever describe one process.
const ACTIVATION_VARIABLES: [&str; 3] = ["LISTEN_FDS", "LISTEN_PID", "LISTEN_FDNAMES"];

/// The exact environment a child execs with, assembled before the fork.
///
/// The supervisor owns the whole environment rather than letting `Command` compose it, for one
/// reason: `LISTEN_PID` must name the child, whose pid does not exist until after the fork, and
/// `Command` installs its environment only *after* the `pre_exec` closure has run — so anything the
/// closure did to the environment would be discarded. Building the array here and storing its
/// pointer in `environ` from the child is the same mechanism `Command` itself uses, minus the
/// ordering problem.
struct ChildEnvironment {
    /// Owns the bytes the pointers below refer to. Never read after construction: keeping those
    /// buffers alive until the `exec` reads them is the whole point of the field.
    #[allow(dead_code)]
    variables: Vec<CString>,
    /// `LISTEN_PID=…` with its digits still to be written. Present only for a child being handed a
    /// listener.
    listen_pid: Option<Box<[u8]>>,
    /// A null-terminated `envp`, exactly what `execvp` expects.
    pointers: Vec<*const libc::c_char>,
}

// SAFETY: every pointer in `pointers` refers to a buffer owned by the same struct, and nothing else
// holds a pointer into either. Moving the struct between threads moves all of it.
unsafe impl Send for ChildEnvironment {}
unsafe impl Sync for ChildEnvironment {}

impl ChildEnvironment {
    fn build(plan: &SpawnPlan, hands_over_listener: bool) -> std::io::Result<ChildEnvironment> {
        let mut values: BTreeMap<OsString, OsString> = match plan.environment {
            EnvironmentBase::Inherited => std::env::vars_os().collect(),
            EnvironmentBase::Minimal => minimal_environment(),
        };
        for (key, value) in &plan.env {
            values.insert(OsString::from(key), OsString::from(value));
        }
        // The child's own account, not the supervisor's, and after the request's own variables so a
        // caller cannot describe the child as somebody it is not. A child that kept root's `HOME`
        // would write its state where the account it runs as cannot read it, and one that kept root's
        // `USER` would tell every tool it runs that it is root.
        values.insert(
            OsString::from("HOME"),
            plan.target.home.clone().into_os_string(),
        );
        let account = OsString::from(plan.target.name.to_string_lossy().into_owned());
        values.insert(OsString::from("USER"), account.clone());
        values.insert(OsString::from("LOGNAME"), account);
        // Last, and after the request's own variables: an activation describes exactly one process,
        // so only the supervisor gets to say whether this child has one. Inherited from its own
        // activation, or asked for by a caller, these would have a child adopt whatever its
        // descriptor 3 happened to be as a listener.
        for variable in ACTIVATION_VARIABLES {
            values.remove(OsStr::new(variable));
        }
        if hands_over_listener {
            values.insert(OsString::from("LISTEN_FDS"), OsString::from("1"));
        }

        let mut variables = Vec::with_capacity(values.len());
        for (key, value) in &values {
            variables.push(environment_entry(key, value)?);
        }
        let listen_pid = hands_over_listener.then(listen_pid_entry);

        let mut pointers: Vec<*const libc::c_char> =
            variables.iter().map(|entry| entry.as_ptr()).collect();
        if let Some(entry) = &listen_pid {
            pointers.push(entry.as_ptr().cast());
        }
        pointers.push(std::ptr::null());

        Ok(ChildEnvironment {
            variables,
            listen_pid,
            pointers,
        })
    }

    /// Stamp this environment with the child's own pid and install it.
    ///
    /// # Safety
    ///
    /// Runs after `fork`. Writes into memory this struct owns and stores one pointer into `environ`;
    /// it allocates nothing, takes no lock, and the array outlives the `exec` that reads it.
    unsafe fn install(&mut self) {
        if let Some(entry) = &mut self.listen_pid {
            write_listen_pid(entry);
        }
        environ = self.pointers.as_ptr();
    }
}

/// The base environment of a session: a `PATH` it did not choose, plus the locale.
///
/// `HOME`, `USER` and `LOGNAME` are not here — [`ChildEnvironment::build`] derives them from the
/// resolved account after everything else, so no caller can set them.
fn minimal_environment() -> BTreeMap<OsString, OsString> {
    let mut values = BTreeMap::new();
    values.insert(OsString::from("PATH"), OsString::from(SESSION_PATH));
    for variable in FORWARDED_VARIABLES {
        if let Some(value) = std::env::var_os(variable) {
            values.insert(OsString::from(variable), value);
        }
    }
    values
}

fn environment_entry(key: &OsStr, value: &OsStr) -> std::io::Result<CString> {
    let mut entry = Vec::with_capacity(key.as_bytes().len() + 1 + value.as_bytes().len());
    entry.extend_from_slice(key.as_bytes());
    entry.push(b'=');
    entry.extend_from_slice(value.as_bytes());
    CString::new(entry).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "environment variable {} contains a nul byte",
                key.to_string_lossy()
            ),
        )
    })
}

/// `LISTEN_PID=` with room for the digits the child writes into it.
fn listen_pid_entry() -> Box<[u8]> {
    let mut entry = vec![0u8; LISTEN_PID_PREFIX.len() + LISTEN_PID_DIGITS + 1];
    entry[..LISTEN_PID_PREFIX.len()].copy_from_slice(LISTEN_PID_PREFIX);
    entry.into_boxed_slice()
}

/// Write this process's own pid into a `LISTEN_PID=` entry, in place.
///
/// Formatted by hand into the buffer the entry already owns: between `fork` and `exec` there is no
/// allocator to reach for, so `format!` is not available. The daemon checks this value against its
/// own pid before adopting the descriptor, which is the contract being satisfied.
fn write_listen_pid(entry: &mut [u8]) {
    // SAFETY: `getpid` only reads this process's own pid.
    let mut remaining = unsafe { libc::getpid() } as u32;
    let mut digits = [0u8; LISTEN_PID_DIGITS];
    let mut written = 0;
    loop {
        digits[written] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
        written += 1;
        if remaining == 0 {
            break;
        }
    }
    let start = LISTEN_PID_PREFIX.len();
    for (offset, digit) in digits[..written].iter().rev().enumerate() {
        entry[start + offset] = *digit;
    }
    entry[start + written] = 0;
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
unsafe fn join_cgroup_scope(procs_path: &CStr) -> std::io::Result<()> {
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

pub(crate) fn path_to_c_string(path: &Path) -> std::io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("path {} contains a nul byte", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TDDY_UID: u32 = 998;
    const TDDY_GID: u32 = 998;
    const DEVELOPERS_GID: u32 = 1042;

    struct SpawnPlanBuilder {
        plan: SpawnPlan,
    }

    /// A plain session spawn: a tool, a target user, no scope, no jail.
    fn a_spawn_plan() -> SpawnPlanBuilder {
        SpawnPlanBuilder {
            plan: SpawnPlan {
                program: PathBuf::from("/usr/local/bin/tddy-coder"),
                args: Vec::new(),
                env: BTreeMap::new(),
                working_dir: PathBuf::from("/srv/tddy/repos/alice/project"),
                target: TargetUser {
                    uid: TDDY_UID,
                    gid: TDDY_GID,
                    name: CString::new("alice").expect("target user name"),
                    home: PathBuf::from("/home/alice"),
                    groups: vec![TDDY_GID, DEVELOPERS_GID],
                },
                scope_procs: None,
                inherit_output: false,
                environment: EnvironmentBase::Minimal,
                sandbox: None,
            },
        }
    }

    impl SpawnPlanBuilder {
        fn with_requested_env(mut self, key: &str, value: &str) -> Self {
            self.plan.env.insert(key.to_string(), value.to_string());
            self
        }

        /// Credentials distinct from each other, so a test can tell a uid map from a gid map.
        fn for_an_account_with(mut self, uid: u32, gid: u32) -> Self {
            self.plan.target.uid = uid;
            self.plan.target.gid = gid;
            self
        }

        /// The account the supervisor itself runs as, so the plan has no privilege to surrender.
        fn for_the_account_the_supervisor_runs_as(mut self) -> Self {
            // SAFETY: both accessors only read this process's own credentials, which is what
            // `privilege_to_drop` compares the target against.
            let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
            self.plan.target.uid = uid;
            self.plan.target.gid = gid;
            self
        }

        fn inheriting_the_supervisors_environment(mut self) -> Self {
            self.plan.environment = EnvironmentBase::Inherited;
            self
        }

        fn in_cgroup_scope(mut self, procs: &str) -> Self {
            self.plan.scope_procs = Some(PathBuf::from(procs));
            self
        }

        fn jailed(mut self) -> Self {
            self.plan.sandbox = Some(SandboxJail {
                mounts: Vec::new(),
                isolate_network: true,
            });
            self
        }

        fn with_bind_mount(mut self, source: &str, target: &str, readonly: bool) -> Self {
            let jail = self.plan.sandbox.get_or_insert_with(SandboxJail::default);
            jail.mounts.push(JailMount {
                source: PathBuf::from(source),
                target: PathBuf::from(target),
                readonly,
            });
            self
        }

        fn sharing_the_hosts_network(mut self) -> Self {
            let jail = self.plan.sandbox.get_or_insert_with(SandboxJail::default);
            jail.isolate_network = false;
            self
        }

        fn build(self) -> SpawnPlan {
            self.plan
        }
    }

    /// Index of the first step matching `predicate`, or a failure naming the whole plan.
    fn position_of(
        steps: &[PreExecStep],
        label: &str,
        predicate: impl Fn(&PreExecStep) -> bool,
    ) -> usize {
        steps
            .iter()
            .position(predicate)
            .unwrap_or_else(|| panic!("no {label} step in plan: {steps:#?}"))
    }

    fn drop_privilege_at(steps: &[PreExecStep]) -> usize {
        position_of(steps, "DropPrivilege", |step| {
            matches!(step, PreExecStep::DropPrivilege { .. })
        })
    }

    /// Every index at which the plan asks the kernel for a parent-death signal.
    fn parent_death_signals_at(steps: &[PreExecStep]) -> Vec<usize> {
        steps
            .iter()
            .enumerate()
            .filter(|(_, step)| **step == PreExecStep::SetParentDeathSignal)
            .map(|(at, _)| at)
            .collect()
    }

    /// The last such index — the arming that is still in force when the child execs.
    fn last_parent_death_signal_at(steps: &[PreExecStep]) -> usize {
        *parent_death_signals_at(steps)
            .last()
            .unwrap_or_else(|| panic!("no SetParentDeathSignal step in plan: {steps:#?}"))
    }

    #[test]
    fn drops_privilege_before_creating_any_namespace() {
        // Given a jailed sandbox spawn
        let plan = a_spawn_plan().jailed().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — the load-bearing contract of the whole feature. A namespace created while still
        // root is owned by the host's root user, giving the child real root-mapped capabilities
        // against it; created after the drop it is owned by the unprivileged target, and the uid
        // map is the same rootless map the daemon builds today.
        let user_namespace = position_of(&steps, "EnterUserNamespace", |step| {
            matches!(step, PreExecStep::EnterUserNamespace)
        });
        assert!(
            drop_privilege_at(&steps) < user_namespace,
            "privilege must be dropped before the user namespace is created, got: {steps:#?}"
        );
    }

    #[test]
    fn drops_privilege_before_entering_the_mount_and_network_namespaces() {
        // Given
        let plan = a_spawn_plan().jailed().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then
        let mount_namespace = position_of(&steps, "EnterMountNamespace", |step| {
            matches!(step, PreExecStep::EnterMountNamespace)
        });
        let network_namespace = position_of(&steps, "EnterNetworkNamespace", |step| {
            matches!(step, PreExecStep::EnterNetworkNamespace)
        });
        assert!(
            drop_privilege_at(&steps) < mount_namespace,
            "privilege must be dropped before unsharing the mount namespace, got: {steps:#?}"
        );
        assert!(
            drop_privilege_at(&steps) < network_namespace,
            "privilege must be dropped before unsharing the network namespace, got: {steps:#?}"
        );
    }

    #[test]
    fn joins_the_cgroup_scope_while_it_still_has_the_privilege_to_do_so() {
        // Given
        let plan = a_spawn_plan()
            .in_cgroup_scope("/sys/fs/cgroup/tddy.slice/tddy-session-alpha.scope/cgroup.procs")
            .jailed()
            .build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — the scope directory is root-owned, so this is the one step that must happen
        // before the drop rather than after it.
        let join = position_of(&steps, "JoinCgroupScope", |step| {
            matches!(step, PreExecStep::JoinCgroupScope { .. })
        });
        assert!(
            join < drop_privilege_at(&steps),
            "the cgroup scope must be joined before privilege is dropped, got: {steps:#?}"
        );
    }

    #[test]
    fn makes_the_root_mount_private_before_establishing_any_bind_mount() {
        // Given
        let plan = a_spawn_plan()
            .jailed()
            .with_bind_mount("/srv/tddy/repos/alice", "/workspace", false)
            .build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — without the recursive private remount first, every mount below propagates out to
        // the host namespace.
        let make_private = position_of(&steps, "MakeRootMountPrivate", |step| {
            matches!(step, PreExecStep::MakeRootMountPrivate)
        });
        let bind = position_of(&steps, "BindMount", |step| {
            matches!(step, PreExecStep::BindMount { .. })
        });
        assert!(
            make_private < bind,
            "root must be made private before binding, got: {steps:#?}"
        );
    }

    #[test]
    fn establishes_bind_mounts_in_declaration_order() {
        // Given
        let plan = a_spawn_plan()
            .jailed()
            .with_bind_mount("/srv/tddy/repos/alice", "/workspace", false)
            .with_bind_mount("/usr/lib", "/usr/lib", true)
            .build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — a later mount may nest inside an earlier one, so order is meaningful.
        let mounts: Vec<&PreExecStep> = steps
            .iter()
            .filter(|step| matches!(step, PreExecStep::BindMount { .. }))
            .collect();
        assert_eq!(
            mounts,
            vec![
                &PreExecStep::BindMount {
                    source: PathBuf::from("/srv/tddy/repos/alice"),
                    target: PathBuf::from("/workspace"),
                    readonly: false,
                },
                &PreExecStep::BindMount {
                    source: PathBuf::from("/usr/lib"),
                    target: PathBuf::from("/usr/lib"),
                    readonly: true,
                },
            ]
        );
    }

    #[test]
    fn brings_loopback_up_only_after_entering_the_network_namespace() {
        // Given
        let plan = a_spawn_plan().jailed().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — bringing `lo` up before the unshare would reconfigure the *host's* loopback.
        let network_namespace = position_of(&steps, "EnterNetworkNamespace", |step| {
            matches!(step, PreExecStep::EnterNetworkNamespace)
        });
        let loopback = position_of(&steps, "BringLoopbackUp", |step| {
            matches!(step, PreExecStep::BringLoopbackUp)
        });
        assert!(
            network_namespace < loopback,
            "loopback must come up inside the new network namespace, got: {steps:#?}"
        );
    }

    #[test]
    fn leaves_loopback_alone_when_the_jail_shares_the_hosts_network() {
        // Given
        let plan = a_spawn_plan().jailed().sharing_the_hosts_network().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — there is no network namespace of its own to configure, and `lo` here is the host's.
        assert!(
            !steps.contains(&PreExecStep::BringLoopbackUp),
            "a jail sharing the host network must not touch its loopback, got: {steps:#?}"
        );
    }

    #[test]
    fn keeps_the_hosts_network_but_still_takes_a_mount_namespace_when_asked_to_share_it() {
        // Given
        let plan = a_spawn_plan().jailed().sharing_the_hosts_network().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — a jail that unshared the network anyway would get an *empty* network with loopback
        // down, which is neither the host's network nor an isolated one anybody asked for. The
        // private remount and the bind mounts need the mount namespace either way.
        assert!(
            steps.contains(&PreExecStep::EnterMountNamespace),
            "a jail always needs a mount namespace of its own, got: {steps:#?}"
        );
        assert!(
            !steps.contains(&PreExecStep::EnterNetworkNamespace),
            "a jail sharing the host network must not unshare one, got: {steps:#?}"
        );
    }

    #[test]
    fn changes_directory_after_dropping_privilege() {
        // Given
        let plan = a_spawn_plan().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — a caller-supplied directory must be traversed with the target user's authority,
        // not root's, or a symlink in it becomes a way to reach paths the user cannot.
        let change_directory = position_of(&steps, "ChangeDirectory", |step| {
            matches!(step, PreExecStep::ChangeDirectory { .. })
        });
        assert!(
            drop_privilege_at(&steps) < change_directory,
            "cwd must be entered after the drop, got: {steps:#?}"
        );
    }

    #[test]
    fn asks_the_kernel_to_signal_the_child_before_doing_anything_else() {
        // Given
        let plan = a_spawn_plan().jailed().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — every later step can fail, and a child that dies holding no parent-death signal
        // is a child that outlives its supervisor.
        assert_eq!(steps.first(), Some(&PreExecStep::SetParentDeathSignal));
    }

    #[test]
    fn re_arms_the_parent_death_signal_after_surrendering_privilege() {
        // Given a spawn with root to give up
        let plan = a_spawn_plan().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — the kernel zeroes `pdeath_signal` on any change of effective ids, so a plan that
        // asked for it only once would hand every child that changes account no signal at all.
        assert!(
            last_parent_death_signal_at(&steps) > drop_privilege_at(&steps),
            "the parent-death signal must be asked for again after the drop clears it, got: \
             {steps:#?}"
        );
    }

    #[test]
    fn re_arms_the_parent_death_signal_after_entering_the_user_namespace() {
        // Given
        let plan = a_spawn_plan().jailed().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — `unshare(CLONE_NEWUSER)` hands the child a full capability set inside the new
        // namespace, which is a credential change, and so clears the signal a second time.
        let user_namespace = position_of(&steps, "EnterUserNamespace", |step| {
            matches!(step, PreExecStep::EnterUserNamespace)
        });
        assert!(
            last_parent_death_signal_at(&steps) > user_namespace,
            "the parent-death signal must be asked for again after the user namespace clears it, \
             got: {steps:#?}"
        );
    }

    #[test]
    fn asks_for_the_parent_death_signal_once_when_nothing_in_the_plan_can_clear_it() {
        // Given a supervisor already running as the target, and no jail
        let plan = a_spawn_plan()
            .for_the_account_the_supervisor_runs_as()
            .build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then — the re-arming is keyed to the steps that clear the signal, not added blindly.
        assert_eq!(parent_death_signals_at(&steps), vec![0]);
    }

    #[test]
    fn plans_no_namespace_steps_for_a_session_that_asked_for_no_jail() {
        // Given a plain session spawn
        let plan = a_spawn_plan().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then
        let jail_steps: Vec<&PreExecStep> = steps
            .iter()
            .filter(|step| {
                matches!(
                    step,
                    PreExecStep::EnterUserNamespace
                        | PreExecStep::EnterMountNamespace
                        | PreExecStep::EnterNetworkNamespace
                        | PreExecStep::MakeRootMountPrivate
                        | PreExecStep::BindMount { .. }
                        | PreExecStep::BringLoopbackUp
                )
            })
            .collect();
        assert_eq!(
            jail_steps,
            Vec::<&PreExecStep>::new(),
            "an unjailed spawn must build no namespaces"
        );
    }

    // -----------------------------------------------------------------------------------------
    // The jail, compiled before the fork
    //
    // What the child then *does* with these values needs a host that permits unprivileged user
    // namespaces, which is operator smoke — see `tests/supervisor_sandbox.rs`. What the supervisor
    // hands it is decided here, and that is what these assert.
    // -----------------------------------------------------------------------------------------

    /// The one step, compiled, or a failure naming what came back instead.
    fn compiled(step: PreExecStep, plan: &SpawnPlan) -> CompiledStep {
        CompiledStep::compile(step, plan)
            .unwrap_or_else(|error| panic!("compile a jail step: {error}"))
    }

    /// Compiling a bind mount resolves its source with `openat2(2)`, so this asserts what a Linux
    /// kernel answers — see [`open_resolved`]. Everywhere else the step is refused before it is
    /// compiled, which is the platform's answer and not this test's subject.
    #[cfg(target_os = "linux")]
    #[test]
    fn compiles_every_step_a_jailed_plan_asks_for() {
        // Given a jail with a bind mount whose source is really on disk
        let source = tempfile::tempdir().expect("a bind mount source");
        let plan = a_spawn_plan()
            .jailed()
            .with_bind_mount(
                source.path().to_str().expect("a utf-8 source path"),
                "/workspace",
                false,
            )
            .build();
        let planned = pre_exec_plan(&plan);

        // When
        let compiled = planned
            .iter()
            .cloned()
            .map(|step| CompiledStep::compile(step, &plan))
            .collect::<std::io::Result<Vec<CompiledStep>>>();

        // Then — the namespace and mount steps are performed rather than refused. Refusing a jail
        // the supervisor cannot build is right; refusing one it can is a session that never runs.
        assert_eq!(
            compiled.expect("compile a jailed plan").len(),
            planned.len()
        );
    }

    #[test]
    fn maps_the_account_the_child_has_become_to_root_inside_the_jail() {
        // Given
        let plan = a_spawn_plan()
            .jailed()
            .for_an_account_with(4001, 4002)
            .build();

        // When
        let compiled = compiled(PreExecStep::EnterUserNamespace, &plan);

        // Then — the map is the target's uid because this step runs *after* `DropPrivilege`, so the
        // effective uid at that moment is the target's. Read from `geteuid()` here it would be the
        // supervisor's own 0, and the jail would map to real host root.
        let CompiledStep::EnterUserNamespace { uid_map, .. } = compiled else {
            panic!("compiling EnterUserNamespace produced another step");
        };
        assert_eq!(uid_map.as_ref(), b"0 4001 1\n");
    }

    #[test]
    fn maps_the_group_the_child_has_become_to_the_root_group_inside_the_jail() {
        // Given
        let plan = a_spawn_plan()
            .jailed()
            .for_an_account_with(4001, 4002)
            .build();

        // When
        let compiled = compiled(PreExecStep::EnterUserNamespace, &plan);

        // Then
        let CompiledStep::EnterUserNamespace { gid_map, .. } = compiled else {
            panic!("compiling EnterUserNamespace produced another step");
        };
        assert_eq!(gid_map.as_ref(), b"0 4002 1\n");
    }

    /// The name it builds is a `/proc/self/fd` path, which is the thing only a Linux bind mount
    /// reads — see [`write_descriptor_path`].
    #[cfg(target_os = "linux")]
    #[test]
    fn names_the_descriptor_it_binds_without_reaching_for_the_allocator() {
        // Given the buffer the child brings to the mount
        let mut buffer = [0u8; DESCRIPTOR_PATH_LEN];

        // When
        write_descriptor_path(1234, &mut buffer);

        // Then — the child mounts this path, so it names the descriptor it just resolved rather than
        // the source path, and it is formatted in place because there is no allocator after a fork.
        assert_eq!(
            CStr::from_bytes_until_nul(&buffer).expect("a nul-terminated path"),
            c"/proc/self/fd/1234"
        );
    }

    /// Compiles a real source with `openat2(2)` — Linux, for the reason above.
    #[cfg(target_os = "linux")]
    #[test]
    fn compiles_a_readonly_bind_mount_as_one_the_child_remounts_readonly() {
        // Given
        let source = tempfile::tempdir().expect("a bind mount source");
        let plan = a_spawn_plan().jailed().build();

        // When
        let compiled = compiled(
            PreExecStep::BindMount {
                source: source.path().to_path_buf(),
                target: PathBuf::from("/usr/lib"),
                readonly: true,
            },
            &plan,
        );

        // Then — `MS_RDONLY` is a property of a mount rather than of a bind, so the flag has to
        // survive to the child, which spends a second `mount` on it.
        let CompiledStep::BindMount { readonly, .. } = compiled else {
            panic!("compiling BindMount produced another step");
        };
        assert!(readonly, "a readonly mount compiled as writable");
    }

    /// Asserts the errno `RESOLVE_NO_SYMLINKS` produces, so it needs the kernel that has it.
    #[cfg(target_os = "linux")]
    #[test]
    fn refuses_a_bind_mount_source_that_is_reached_through_a_symlink() {
        // Given a session user's own escape, planted under a directory policy permits
        let root = tempfile::tempdir().expect("a mount root");
        let escape = root.path().join("esc");
        std::os::unix::fs::symlink("/", &escape).expect("plant the escape");
        let plan = a_spawn_plan().jailed().build();

        // When
        let refused = CompiledStep::compile(
            PreExecStep::BindMount {
                source: escape.clone(),
                target: PathBuf::from("/workspace"),
                readonly: false,
            },
            &plan,
        )
        .expect_err("a symlinked source must be refused");

        // Then — `mount(2)` follows symlinks in its source, so this is `/` bound into the jail. The
        // resolution refuses to traverse one at all.
        assert_eq!(
            refused.to_string(),
            format!(
                "bind mount source {} could not be resolved (symlinks are not followed): {}",
                escape.display(),
                std::io::Error::from_raw_os_error(libc::ELOOP)
            )
        );
    }

    /// Asserts the errno the kernel's own resolution returns for an absent source.
    #[cfg(target_os = "linux")]
    #[test]
    fn refuses_a_bind_mount_source_that_does_not_exist_rather_than_skipping_it() {
        // Given
        let root = tempfile::tempdir().expect("a mount root");
        let missing = root.path().join("never-created");
        let plan = a_spawn_plan().jailed().build();

        // When
        let refused = CompiledStep::compile(
            PreExecStep::BindMount {
                source: missing.clone(),
                target: PathBuf::from("/workspace"),
                readonly: false,
            },
            &plan,
        )
        .expect_err("an absent source must be refused");

        // Then — the daemon's own jail skips a source that is not there. A supervisor that did the
        // same would hand back a session whose jail is quietly missing a mount it asked for.
        assert_eq!(
            refused.to_string(),
            format!(
                "bind mount source {} could not be resolved (symlinks are not followed): {}",
                missing.display(),
                std::io::Error::from_raw_os_error(libc::ENOENT)
            )
        );
    }

    #[test]
    fn refuses_a_bind_mount_on_a_kernel_that_cannot_resolve_a_path_without_a_symlink_race() {
        // Given the answer a kernel older than 5.6 gives to `openat2`
        let unsupported = std::io::Error::from_raw_os_error(libc::ENOSYS);

        // When
        let refused = unresolvable_mount_source(Path::new("/srv/tddy/repos/alice"), &unsupported);

        // Then — there is nothing to fall back to: a path-based check is exactly the race this
        // resolution exists to remove, so the session is not spawned.
        assert_eq!(refused.kind(), std::io::ErrorKind::Unsupported);
        assert_eq!(
            refused.to_string(),
            "this host has no openat2(2) (Linux 5.6 or newer), so bind mount source \
             /srv/tddy/repos/alice cannot be resolved without a symlink race; the supervisor will \
             not spawn a session it cannot isolate"
        );
    }

    // -----------------------------------------------------------------------------------------
    // Credentials that resolve to root
    // -----------------------------------------------------------------------------------------

    #[test]
    fn accepts_an_account_that_resolves_to_an_unprivileged_uid_and_gid() {
        // Given / When
        let refused = refuse_root_credentials("tddy", TDDY_UID, TDDY_GID);

        // Then
        assert!(refused.is_ok(), "got {refused:?}");
    }

    #[test]
    fn refuses_an_account_that_resolves_to_uid_zero_under_another_name() {
        // Given a passwd entry aliased to root — `toor` is the classic one, and the config's
        // rejection of the *name* `root` says nothing about it.
        let refused = refuse_root_credentials("toor", 0, TDDY_GID);

        // Then — nothing downstream would catch it: the supervisor is already uid 0, so there is no
        // privilege to drop and the child would exec as real root.
        assert_eq!(
            refused.expect_err("uid 0 must be refused").to_string(),
            "account `toor` resolves to uid 0; the supervisor is the only privileged process on the \
             host"
        );
    }

    #[test]
    fn refuses_an_account_whose_primary_group_is_the_root_group() {
        // Given / When
        let refused = refuse_root_credentials("tddy", TDDY_UID, 0);

        // Then — gid 0 is read access to most of what the supervisor exists to keep from a session.
        assert_eq!(
            refused.expect_err("gid 0 must be refused").to_string(),
            "account `tddy` resolves to gid 0; the supervisor is the only privileged process on the \
             host"
        );
    }

    #[test]
    fn refuses_to_resolve_the_root_account_that_exists_on_every_host() {
        // Given / When — the passwd lookup succeeds; it is the credentials it returns that are
        // refused, which is what makes this the gate rather than the name check in `config.rs`.
        let resolved = resolve_target_user("root");

        // Then
        assert_eq!(
            resolved
                .expect_err("the root account must not resolve to a spawn target")
                .to_string(),
            "account `root` resolves to uid 0; the supervisor is the only privileged process on the \
             host"
        );
    }

    // -----------------------------------------------------------------------------------------
    // Supplementary groups
    // -----------------------------------------------------------------------------------------

    #[test]
    fn hands_the_child_the_group_list_that_was_enumerated_before_the_fork() {
        // Given
        let plan = a_spawn_plan().build();
        let steps = pre_exec_plan(&plan);
        let drop = steps[drop_privilege_at(&steps)].clone();

        // When
        let compiled = CompiledStep::compile(drop, &plan).expect("compile the privilege drop");

        // Then — the child calls `setgroups` with this list. Recomputing it after the fork would mean
        // `initgroups` → NSS → allocation, in a process where none of that is safe.
        let CompiledStep::DropPrivilege { groups, .. } = compiled else {
            panic!("compiling DropPrivilege produced another step");
        };
        assert_eq!(groups, vec![TDDY_GID, DEVELOPERS_GID]);
    }

    #[test]
    fn resolves_an_accounts_primary_group_into_its_group_list() {
        // Given a primary group the host cannot also have listed the account in, so the assertion is
        // about this function and not about the host's `/etc/group`.
        let groups = resolve_supplementary_groups(c"root", DEVELOPERS_GID)
            .expect("enumerate the groups of an account");

        // Then — the rest of the list is whatever the host says; the primary group is the part
        // `setgroups` must not be missing.
        assert!(
            groups.contains(&DEVELOPERS_GID),
            "primary group missing from {groups:?}"
        );
    }

    // -----------------------------------------------------------------------------------------
    // The child's environment
    // -----------------------------------------------------------------------------------------

    /// The environment a child would exec with, read back out of the array built for it.
    fn environment_of(plan: &SpawnPlan) -> BTreeMap<String, String> {
        ChildEnvironment::build(plan, false)
            .expect("build the child environment")
            .variables
            .iter()
            .map(|entry| {
                let entry = entry.to_str().expect("environment entry is utf-8");
                let (key, value) = entry.split_once('=').expect("environment entry has a `=`");
                (key.to_string(), value.to_string())
            })
            .collect()
    }

    #[test]
    fn gives_a_session_a_path_it_did_not_choose() {
        // Given
        let plan = a_spawn_plan().build();

        // When
        let environment = environment_of(&plan);

        // Then
        assert_eq!(
            environment.get("PATH"),
            Some(&"/usr/local/bin:/usr/bin:/bin".to_string())
        );
    }

    #[test]
    fn names_the_account_a_session_actually_runs_as() {
        // Given
        let plan = a_spawn_plan().build();

        // When
        let environment = environment_of(&plan);

        // Then — a session that kept root's `HOME` would write its state where the account it runs as
        // cannot read it, and one that kept root's `USER` would tell every tool it runs that it is root.
        assert_eq!(environment.get("HOME"), Some(&"/home/alice".to_string()));
        assert_eq!(environment.get("USER"), Some(&"alice".to_string()));
        assert_eq!(environment.get("LOGNAME"), Some(&"alice".to_string()));
    }

    #[test]
    fn refuses_to_let_a_request_describe_a_session_as_a_different_account() {
        // Given a caller asking for a session that claims to be root.
        let plan = a_spawn_plan()
            .with_requested_env("USER", "root")
            .with_requested_env("HOME", "/root")
            .build();

        // When
        let environment = environment_of(&plan);

        // Then — the account is derived after the request's own variables, so there is nothing to
        // override.
        assert_eq!(environment.get("USER"), Some(&"alice".to_string()));
        assert_eq!(environment.get("HOME"), Some(&"/home/alice".to_string()));
    }

    #[test]
    fn keeps_the_supervisors_own_environment_out_of_a_session() {
        // Given something in the supervisor's environment that an `EnvironmentFile=` would put there.
        std::env::set_var("TDDY_SUPERVISOR_UNIT_SECRET", "hunter2");
        let plan = a_spawn_plan().build();

        // When
        let environment = environment_of(&plan);

        // Then — a session runs as somebody else on a caller's behalf and has no claim on what the
        // root supervisor was started with.
        assert_eq!(environment.get("TDDY_SUPERVISOR_UNIT_SECRET"), None);
    }

    #[test]
    fn hands_a_declared_service_the_environment_the_supervisor_was_started_with() {
        // Given
        std::env::set_var("TDDY_SUPERVISOR_UNIT_LOG_LEVEL", "debug");
        let plan = a_spawn_plan()
            .inheriting_the_supervisors_environment()
            .build();

        // When
        let environment = environment_of(&plan);

        // Then — a declared service is root's own decision, and an operator setting a variable on the
        // supervisor unit means it for the services under it.
        assert_eq!(
            environment.get("TDDY_SUPERVISOR_UNIT_LOG_LEVEL"),
            Some(&"debug".to_string())
        );
    }

    #[test]
    fn omits_the_privilege_drop_when_it_already_runs_as_the_target_user() {
        // Given a supervisor whose own uid/gid are the target's — nothing to surrender, and
        // `initgroups` would fail for lack of the privilege to call it.
        let plan = a_spawn_plan().build();
        let current = TargetUser {
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
            name: plan.target.name.clone(),
            home: plan.target.home.clone(),
            groups: plan.target.groups.clone(),
        };
        let plan = SpawnPlan {
            target: current,
            ..plan
        };

        // When
        let steps = pre_exec_plan(&plan);

        // Then
        assert!(
            !steps
                .iter()
                .any(|step| matches!(step, PreExecStep::DropPrivilege { .. })),
            "there is no privilege to drop, got: {steps:#?}"
        );
    }
}
