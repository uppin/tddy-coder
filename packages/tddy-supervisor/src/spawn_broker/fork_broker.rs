use crate::handover::SocketHandover;
use crate::handover::{hand_over_listeners, reserve_handover_slots};
use crate::spawn_broker::bind_mount;
use crate::spawn_broker::bring_loopback_up;
use crate::spawn_broker::ChildOutput;
use crate::spawn_broker::change_directory;
use crate::spawn_broker::child_environment::ChildEnvironment;
use crate::spawn_broker::compile_bind_mount;
use crate::spawn_broker::drop_privilege;
use crate::spawn_broker::enter_mount_namespace;
use crate::spawn_broker::enter_network_namespace;
use crate::spawn_broker::enter_user_namespace;
use crate::spawn_broker::join_cgroup_scope;
use crate::spawn_broker::lead_own_process_group;
use crate::spawn_broker::make_root_mount_private;
use crate::spawn_broker::path_to_c_string;
use crate::spawn_broker::pre_exec_plan;
use crate::spawn_broker::set_parent_death_signal;
use crate::spawn_broker::single_id_map;
use crate::spawn_broker::PreExecStep;
use crate::spawn_broker::SpawnPlan;
use std::ffi::CString;
use std::os::fd::RawFd;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use tokio::sync::{mpsc, oneshot};

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
    StdioMode,
    oneshot::Sender<std::io::Result<SpawnedChild>>,
);

/// How the supervisor holds a child's stdio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StdioMode {
    /// As [`SpawnPlan::output`] says; stdin is always null.
    Plan,
    /// All three streams piped to the supervisor, for a relayed spawn.
    Piped,
}

/// A forked child's pid, the read end of its stderr when the plan asked for it to be captured, and,
/// for a relayed spawn, the supervisor's ends of its stdio.
pub struct SpawnedChild {
    pub pid: u32,
    pub stderr: Option<std::process::ChildStderr>,
    pub stdio: Option<ChildStdio>,
}

/// The supervisor's ends of a relayed child's stdin, stdout and stderr.
pub struct ChildStdio {
    pub stdin: std::process::ChildStdin,
    pub stdout: std::process::ChildStdout,
    pub stderr: std::process::ChildStderr,
}

impl ForkBroker {
    /// Start the fork thread. It runs until the last [`ForkBroker`] is dropped, which happens when
    /// the supervisor is on its way out.
    pub fn start() -> std::io::Result<ForkBroker> {
        // Taken before the thread exists and held for as long as it does — see the function's docs.
        let handover_slots = reserve_handover_slots()?;
        let (jobs, mut queue) = mpsc::unbounded_channel::<Job>();
        std::thread::Builder::new()
            .name("tddy-supervisor-fork".to_string())
            .spawn(move || {
                let _handover_slots = handover_slots;
                while let Some((plan, handover, stdio, reply)) = queue.blocking_recv() {
                    let _ = reply.send(spawn_now(plan, handover, stdio));
                }
            })?;
        Ok(ForkBroker { jobs })
    }

    /// Fork, drop privilege, and exec the plan on the fork thread. Returns the child's pid.
    ///
    /// `handover` is the listening socket the child should find at [`crate::socket::SD_LISTEN_FDS_START`]. It is
    /// not part of the [`SpawnPlan`] because it is a live descriptor the supervisor owns, not a
    /// decision about the child.
    pub async fn spawn(
        &self,
        plan: SpawnPlan,
        handover: Option<SocketHandover>,
    ) -> std::io::Result<u32> {
        Ok(self.submit(plan, handover, StdioMode::Plan).await?.pid)
    }

    /// [`Self::spawn`] with the child's stdio held as `stdio` says. [`StdioMode::Piped`] is a
    /// relayed spawn's, whose pipes go to [`crate::relay`]; it is never given a listener handover.
    pub(crate) async fn submit(
        &self,
        plan: SpawnPlan,
        handover: Option<SocketHandover>,
        stdio: StdioMode,
    ) -> std::io::Result<SpawnedChild> {
        let (reply, answer) = oneshot::channel();
        self.jobs
            .send((plan, handover, stdio, reply))
            .map_err(|_| std::io::Error::other("the fork thread has stopped"))?;
        answer
            .await
            .map_err(|_| std::io::Error::other("the fork thread dropped the request"))?
    }
}

fn spawn_now(
    plan: SpawnPlan,
    handover: Option<SocketHandover>,
    stdio: StdioMode,
) -> std::io::Result<SpawnedChild> {
    let mut command = Command::new(&plan.program);
    command.args(&plan.args);
    configure_stdio(&mut command, stdio, plan.output);
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

    let mut child = command.spawn()?;
    let piped = take_piped(&mut child, stdio);
    let stderr = match stdio {
        StdioMode::Plan => child.stderr.take(),
        StdioMode::Piped => None,
    };
    Ok(SpawnedChild {
        pid: child.id(),
        stderr,
        stdio: piped,
    })
}

/// Point the child's stdio where `stdio` says: for [`StdioMode::Plan`], stdin null and the output
/// where the plan's [`ChildOutput`] sends it; for [`StdioMode::Piped`], all three to the supervisor.
fn configure_stdio(command: &mut Command, stdio: StdioMode, output: ChildOutput) {
    match stdio {
        StdioMode::Plan => {
            command.stdin(Stdio::null());
            match output {
                ChildOutput::Inherit => {}
                ChildOutput::Discard => {
                    command.stdout(Stdio::null());
                    command.stderr(Stdio::null());
                }
                ChildOutput::CaptureStderr => {
                    command.stdout(Stdio::null());
                    command.stderr(Stdio::piped());
                }
            }
        }
        StdioMode::Piped => {
            command.stdin(Stdio::piped());
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());
        }
    }
}

/// The supervisor's ends of a [`StdioMode::Piped`] child's stdio; `None` for any other child.
fn take_piped(child: &mut std::process::Child, stdio: StdioMode) -> Option<ChildStdio> {
    if stdio != StdioMode::Piped {
        return None;
    }
    Some(ChildStdio {
        stdin: child.stdin.take()?,
        stdout: child.stdout.take()?,
        stderr: child.stderr.take()?,
    })
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
    /// The listeners to place at [`crate::socket::SD_LISTEN_FDS_START`] and after, in order — none for a child that
    /// is handed nothing. Owned by the supervisor, not by the plan.
    handover: Vec<RawFd>,
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
            environment: ChildEnvironment::build(plan, handover)?,
            handover: handover.map(SocketHandover::raw_fds).unwrap_or_default(),
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
        if !self.handover.is_empty() {
            // SAFETY: every descriptor is open in this child, inherited across the fork.
            unsafe { hand_over_listeners(&self.handover) }?;
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
pub(super) enum CompiledStep {
    SetParentDeathSignal,
    LeadOwnProcessGroup,
    JoinCgroupScope {
        procs: CString,
    },
    DropPrivilege {
        uid: libc::uid_t,
        gid: libc::gid_t,
        /// The account's groups, already enumerated — see [`TargetUser::groups`](super::accounts::TargetUser::groups).
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
    pub(super) fn compile(step: PreExecStep, plan: &SpawnPlan) -> std::io::Result<CompiledStep> {
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
