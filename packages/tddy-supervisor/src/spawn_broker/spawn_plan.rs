use crate::spawn_broker::accounts::TargetUser;
use crate::spawn_broker::fork_broker::CompiledStep;
use crate::spawn_broker::open_without_following_symlinks;
use crate::spawn_broker::path_to_c_string;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

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
    /// Where the child's stdout/stderr go. See [`ChildOutput`].
    pub output: ChildOutput,
    /// What the child's environment is built on top of, before `env` is applied.
    pub environment: EnvironmentBase,
    /// Jail to build around the child. `None` spawns a plain process in the host's namespaces.
    pub sandbox: Option<SandboxJail>,
    /// The checked executable, held so its descriptor stays open until the exec.
    pub checked: Option<crate::policy::CheckedExecutable>,
}

/// Where a child's stdout and stderr go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildOutput {
    /// Shared with the supervisor's own, so a managed service's output reaches the journal.
    Inherit,
    /// `/dev/null`. A session's output belongs to whoever asked for it, not to the supervisor's
    /// journal.
    Discard,
    /// stdout to `/dev/null`, stderr to a pipe the supervisor reads: a clone's progress and its own
    /// explanation of a failure. The read end is handed back by
    /// [`Supervisor::spawn_session_capturing_stderr`](crate::supervisor::Supervisor::spawn_session_capturing_stderr).
    CaptureStderr,
}

/// Where a child's environment starts from.
///
/// The supervisor's own environment is not a neutral starting point. On a real host it is whatever the
/// unit's `Environment=` and `EnvironmentFile=` put there, which is where operators keep credentials,
/// and the supervisor is uid 0 when it reads them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentBase {
    /// The supervisor's own environment, verbatim, plus [`SUPERVISOR_SOCKET_ENV`] naming the socket
    /// this supervisor listens on.
    ///
    /// For a *declared managed service* only. Root wrote that declaration, and an operator who sets
    /// `RUST_LOG` on the supervisor unit expects it to reach the daemon exactly as it does today.
    /// The socket travels in this variant, rather than with every spawn, so that only a managed
    /// service can ever be told where the supervisor listens.
    Inherited {
        /// `socket.path` from `supervisor.yaml`.
        supervisor_socket: PathBuf,
    },
    /// `PATH`, the account's own `HOME`/`USER`/`LOGNAME`, and the host's locale. Nothing else.
    ///
    /// For a session, which runs as somebody else on a caller's behalf and has no claim on anything
    /// the supervisor was started with.
    Minimal,
    /// The request's own variables, as the daemon computed them, and the account's
    /// `HOME`/`USER`/`LOGNAME`. No supervisor `PATH`, no forwarded locale, nothing the supervisor
    /// was started with (`#agent-tools 28/28`).
    ///
    /// For a relayed spawn: what the child sees is the caller's configuration, owned by the
    /// daemon and its YAML; the supervisor knows only the OS user.
    Exactly,
}

/// `PATH` for a session, whose caller does not get to choose one.
///
/// Fixed rather than taken from the supervisor's own environment, so what a session resolves a
/// command to does not depend on how the unit happened to be started. The daemon's env passes
/// whole under the one env rule, and this is the base it is laid over for a session.
pub(super) const SESSION_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

/// Variables a minimal environment still takes from the supervisor: they decide how text is rendered
/// and what the clock reads, not what code runs.
pub(super) const FORWARDED_VARIABLES: [&str; 2] = ["LANG", "TZ"];

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
/// This is the only place the order is decided. [`PreExecSteps`](fork_broker::PreExecSteps) executes exactly this sequence,
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
pub(super) fn single_id_map(id: u32) -> Box<[u8]> {
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
pub(super) fn compile_bind_mount(
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
pub(super) fn unresolvable_mount_source(source: &Path, error: &std::io::Error) -> std::io::Error {
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
