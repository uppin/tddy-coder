//! Fork/exec with a privilege drop — the one place in the stack that turns root into somebody else.
//!
//! ## Ordering is the security contract
//!
//! Everything the child does between `fork` and `exec` happens in this order, and the order is
//! load-bearing:
//!
//! 1. **tie the child's lifetime to the supervisor's** — a supervisor that is killed rather than
//!    asked to stop must not leave the daemon and every session running unsupervised on the host.
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
//! 6. **namespace and mount setup** (not yet implemented, see [`CompiledStep::compile`]) — must
//!    stay after step 4. A user namespace created by root maps the child to *real* uid 0 against
//!    the host; created by the unprivileged target it maps to the target, which is what today's
//!    rootless jail already does.
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
use std::path::PathBuf;
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
        // the buffer it was given.
        let found =
            unsafe { libc::getgrouplist(name.as_ptr(), gid, groups.as_mut_ptr(), &mut count) };
        if found >= 0 {
            groups.truncate(found as usize);
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
    /// `unshare(CLONE_NEWNS | CLONE_NEWNET)`.
    EnterMountAndNetworkNamespaces,
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
        steps.push(PreExecStep::EnterMountAndNetworkNamespaces);
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
            // Inside the new network namespace, so this configures the jail's loopback and not the
            // host's.
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
            // TODO(supervisor/milestone-5): implement the jail. Each of these becomes the syscall
            // `tddy_sandbox_cgroups::enter_rootless_jail` performs today —
            // `unshare(CLONE_NEWUSER)` with `uid_map`/`setgroups=deny`/`gid_map`,
            // `unshare(CLONE_NEWNS | CLONE_NEWNET)`, `mount(/, MS_REC | MS_PRIVATE)`, the bind
            // mounts, and the loopback bring-up. Until then a jailed spawn is refused *here*, in
            // the parent, where the caller gets a real message: silently spawning the session
            // without its jail would be the one outcome worse than refusing it.
            PreExecStep::EnterUserNamespace
            | PreExecStep::EnterMountAndNetworkNamespaces
            | PreExecStep::MakeRootMountPrivate
            | PreExecStep::BindMount { .. }
            | PreExecStep::BringLoopbackUp => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "sandbox namespace and mount setup is not implemented; the supervisor will not \
                 spawn a session it cannot isolate",
            )),
        }
    }

    fn execute(&self, supervisor_pid: libc::pid_t) -> std::io::Result<()> {
        // SAFETY: every call below runs after `fork` and touches only this process's own
        // credentials, working directory, process group or descriptors. Each `CString` is owned by
        // this step and outlives the call.
        unsafe {
            match self {
                CompiledStep::SetParentDeathSignal => set_parent_death_signal(supervisor_pid),
                CompiledStep::LeadOwnProcessGroup => lead_own_process_group(),
                CompiledStep::JoinCgroupScope { procs } => join_cgroup_scope(procs),
                CompiledStep::DropPrivilege { uid, gid, groups } => {
                    drop_privilege(*uid, *gid, groups)
                }
                CompiledStep::ChangeDirectory { path } => change_directory(path),
            }
        }
    }
}

/// Ask the kernel to signal us when the supervisor dies, and check it has not died already.
///
/// # Safety
///
/// Runs after `fork`, so it may only use async-signal-safe syscalls. `prctl` and `getppid` read and
/// write nothing but this process's own state.
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
    if libc::setgroups(groups.len(), groups.as_ptr()) != 0 {
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

fn path_to_c_string(path: &std::path::Path) -> std::io::Result<CString> {
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
        let namespaces = position_of(&steps, "EnterMountAndNetworkNamespaces", |step| {
            matches!(step, PreExecStep::EnterMountAndNetworkNamespaces)
        });
        assert!(
            drop_privilege_at(&steps) < namespaces,
            "privilege must be dropped before unsharing mount/net, got: {steps:#?}"
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
        let namespaces = position_of(&steps, "EnterMountAndNetworkNamespaces", |step| {
            matches!(step, PreExecStep::EnterMountAndNetworkNamespaces)
        });
        let loopback = position_of(&steps, "BringLoopbackUp", |step| {
            matches!(step, PreExecStep::BringLoopbackUp)
        });
        assert!(
            namespaces < loopback,
            "loopback must come up inside the new namespace, got: {steps:#?}"
        );
    }

    #[test]
    fn leaves_loopback_alone_when_the_jail_shares_the_hosts_network() {
        // Given
        let plan = a_spawn_plan().jailed().sharing_the_hosts_network().build();

        // When
        let steps = pre_exec_plan(&plan);

        // Then
        assert!(
            !steps.contains(&PreExecStep::BringLoopbackUp),
            "a jail sharing the host network must not touch its loopback, got: {steps:#?}"
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
                        | PreExecStep::EnterMountAndNetworkNamespaces
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
