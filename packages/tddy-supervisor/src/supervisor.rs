//! The mini-init: start declared services in order, keep them alive, take them down together.
//!
//! All mutable state lives behind one lock, and the decisions about it are [`ServiceRuntime`]'s —
//! this module only performs them. The lock is a `std::sync::Mutex` and is never held across an
//! `await`, because the two things that happen under it (reading a spawn plan, recording an exit)
//! are pure bookkeeping.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::cgroup_broker::{self, ScopeRemoval};
use crate::config::{ManagedService, ServiceSocket, SupervisorConfig};
use crate::error::SupervisorError;
use crate::reaper;
use crate::request::{SessionState, SessionStatus};
use crate::service::ServiceStatus;
use crate::services::{ExitOutcome, ServiceRuntime, STARTUP_GRACE_PERIOD};
use crate::signals::{signal_process, signal_process_group, KILL, TERMINATE};
use crate::spawn_broker::{
    self, EnvironmentBase, ForkBroker, SocketHandover, SpawnPlan, TargetUser,
};

/// How often the shutdown sequence re-checks whether its children are gone.
const REAP_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long a `SIGKILL`ed process gets to disappear before the supervisor stops waiting for it.
/// `SIGKILL` is not refusable, so this only covers the kernel doing the work.
const KILL_TIMEOUT: Duration = Duration::from_secs(2);

/// How many exited sessions keep their status available for a caller that has not asked yet.
///
/// Some bound is mandatory: the supervisor is a process that must not need restarting, so a map of
/// dead pids that only ever grows is a slow leak with no upper edge. The bound is a capacity rather
/// than "forget it once somebody has read it" because a caller polls repeatedly — the daemon's
/// exit-diagnostic loop asks every 50ms and asks again after it has its answer — so a status that
/// vanished on first read would answer one poll and deny the next.
const RETAINED_EXITED_SESSIONS: usize = 256;

/// How long a scope's leftover processes get to disappear before its directory is left behind.
///
/// `rmdir` on a cgroup fails with `EBUSY` while *any* process remains in it, and a session's own
/// descendants routinely outlive the session leader by a moment — a shell that exits before the
/// children it signalled do. Waiting is therefore normal and short. Waiting *forever* is not: a scope
/// whose processes never leave is a leak, and this is how long the supervisor tries before saying so.
const SCOPE_REMOVAL_GRACE: Duration = Duration::from_secs(2);

/// How often a scope that still holds processes is retried within [`SCOPE_REMOVAL_GRACE`].
const SCOPE_REMOVAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// One declared service: its declaration, its resolved account, and its lifecycle state.
struct Slot {
    service: ManagedService,
    target: TargetUser,
    runtime: ServiceRuntime,
    started_at: Option<Instant>,
    /// The listener created for a service that declared a socket, held for as long as the supervisor
    /// runs so every start — including every restart — is handed the same one.
    listener: Option<Arc<UnixListener>>,
}

/// What became of one session the supervisor spawned on a caller's behalf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionOutcome {
    Running,
    /// `None` when the session was killed by a signal rather than exiting with a code.
    Exited {
        code: Option<i32>,
    },
}

/// Which incarnation of a pid a record belongs to.
///
/// A pid is not an identity. `pid_max` is 32768 on a stock host, so a pid freed by one session is
/// routinely handed to the next one within seconds — well inside the shutdown grace period that
/// [`Supervisor::stop_session`] arms a `SIGKILL` after. A generation is taken once per spawn and never
/// reused, so a request made about one session can be recognised as no longer being about the process
/// that now holds its pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SessionGeneration(u64);

/// One session's identity and state, as of the last thing observed about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionRecord {
    generation: SessionGeneration,
    outcome: SessionOutcome,
}

/// Every session the supervisor spawned on a caller's behalf: the live ones, and the most recently
/// exited ones with the status they exited with.
///
/// An exited session's status has to outlive its reap. The supervisor is the only process that can
/// `waitpid` its own children, and a caller's poll always arrives *after* the child was reaped, so a
/// status dropped at reap time is one nobody could ever observe. Retention is bounded — see
/// [`RETAINED_EXITED_SESSIONS`].
#[derive(Default)]
struct SessionTable {
    records: BTreeMap<u32, SessionRecord>,
    /// Exited pids in reap order, so the oldest retained status is the first one evicted.
    exited: VecDeque<u32>,
    /// Generations handed out so far. Monotonic for the life of the process.
    spawned: u64,
    /// The cgroup scope directory each session was spawned into, for the sessions that were spawned
    /// into one at all.
    ///
    /// Keyed by generation rather than by pid, which is the same hazard [`SessionGeneration`] exists
    /// for: a pid is reissued while the previous session's status is still retained, and a scope
    /// reclaimed on the strength of a pid alone would be the *new* session's live scope.
    ///
    /// Only a session the supervisor spawned *into* a scope has an entry here. A scope a caller
    /// created with `CreateScope` and attached no session to is not in this map and is never swept:
    /// the association is what makes a scope garbage, and without one the supervisor cannot know
    /// whether the caller has finished with it.
    scopes: BTreeMap<SessionGeneration, PathBuf>,
}

impl SessionTable {
    /// Record a new session and return the generation that identifies *this* incarnation of its pid.
    ///
    /// A caller holding a generation can later be told that the request it is making is about a
    /// session that no longer exists, rather than being silently applied to whichever process
    /// inherited the pid.
    fn record_spawned(&mut self, pid: u32) -> SessionGeneration {
        // A pid the kernel has recycled must not be evicted later as if it were still the dead
        // session that used to own it.
        self.exited.retain(|retained| *retained != pid);
        self.spawned += 1;
        let generation = SessionGeneration(self.spawned);
        if let Some(displaced) = self.records.insert(
            pid,
            SessionRecord {
                generation,
                outcome: SessionOutcome::Running,
            },
        ) {
            // TODO(supervisor): a caller can only ask about a session by pid (`SessionRef`), so the
            // status of the displaced session is no longer reachable even though it is the answer a
            // caller polling for it wants. Closing that needs the generation on the wire — a session
            // handle in `SpawnedProcess`/`SessionRef` — which is a protocol decision, not something
            // to add here. Until then the loss is at least visible in the journal.
            if displaced.outcome != SessionOutcome::Running {
                log::warn!(
                    target: "tddy_supervisor::supervisor",
                    "pid {pid} was reissued to a new session; the exit status of the previous one is \
                     no longer reportable"
                );
            }
        }
        generation
    }

    /// Record that `generation`'s session was spawned into the cgroup scope at `scope`.
    ///
    /// This association is the whole reason a scope can be cleaned up without a caller asking for it,
    /// and it is deliberately made from the spawn itself rather than declared separately: the scope a
    /// session joins *is* its plan's `cgroup.procs`, so there is nothing for the two to disagree about.
    fn own_scope(&mut self, generation: SessionGeneration, scope: PathBuf) {
        self.scopes.insert(generation, scope);
    }

    /// Record an exit, if `pid` is a session at all, and hand back the cgroup scope that has just
    /// become garbage. A pid this table does not know is left alone: only the sessions the supervisor
    /// was asked for are reportable.
    ///
    /// The scope and the status have different lifetimes on purpose. The directory is reclaimed now —
    /// nothing will ever run in it again — while the *status* stays retained for a caller whose poll
    /// has not arrived yet ([`RETAINED_EXITED_SESSIONS`]). Handing the scope back exactly once is what
    /// keeps a repeated reap, or a pid the kernel has reissued, from removing a directory that by then
    /// belongs to a later session.
    fn record_exit(&mut self, pid: u32, code: Option<i32>) -> Option<PathBuf> {
        let record = self.records.get_mut(&pid)?;
        record.outcome = SessionOutcome::Exited { code };
        let generation = record.generation;

        self.exited.push_back(pid);
        while self.exited.len() > RETAINED_EXITED_SESSIONS {
            if let Some(evicted) = self.exited.pop_front() {
                self.records.remove(&evicted);
            }
        }
        // Taken, not read: the scope of a session that has exited is handed to the caller once, and
        // the only other entries left are those of sessions that are still running.
        self.scopes.remove(&generation)
    }

    /// The state of `pid` together with the incarnation it belongs to, read as one value.
    fn snapshot(&self, pid: u32) -> Option<(SessionGeneration, SessionStatus)> {
        let generation = self.records.get(&pid)?.generation;
        self.status(pid).map(|status| (generation, status))
    }

    fn status(&self, pid: u32) -> Option<SessionStatus> {
        self.records.get(&pid).map(|record| match record.outcome {
            SessionOutcome::Running => SessionStatus {
                pid,
                state: SessionState::Running,
                exit_code: None,
            },
            SessionOutcome::Exited { code } => SessionStatus {
                pid,
                state: SessionState::Exited,
                exit_code: code,
            },
        })
    }

    /// Whether `pid` is still the running session that was spawned as `generation`.
    ///
    /// Both halves matter: a session that has exited must not be signalled, and a pid that now
    /// belongs to a *later* session must not be signalled on an earlier one's behalf.
    fn is_running_generation(&self, pid: u32, generation: SessionGeneration) -> bool {
        self.records.get(&pid)
            == Some(&SessionRecord {
                generation,
                outcome: SessionOutcome::Running,
            })
    }

    fn running_pids(&self) -> Vec<u32> {
        self.records
            .iter()
            .filter(|(_, record)| record.outcome == SessionOutcome::Running)
            .map(|(pid, _)| *pid)
            .collect()
    }

    /// Confirm `pid` is a session this supervisor spawned and has not reaped yet.
    ///
    /// An unknown pid is [`SupervisorError::Denied`] rather than reported on, for the same reason
    /// [`Supervisor::session_status`] refuses one: the privileged surface must not become a way to
    /// act on, or learn about, processes the supervisor was never asked for.
    ///
    /// An *exited* session is refused too, even though its status is still retained for a caller
    /// that has not polled yet. Once the reap has happened the kernel is free to hand that pid to
    /// somebody else's process, so a pid that is only known to have been a session is
    /// indistinguishable from a stranger's.
    fn require_running(&self, pid: u32) -> Result<(), SupervisorError> {
        match self.records.get(&pid).map(|record| record.outcome) {
            Some(SessionOutcome::Running) => Ok(()),
            Some(SessionOutcome::Exited { .. }) | None => Err(SupervisorError::Denied),
        }
    }
}

/// Starts, reaps, restarts and shuts down the declared services.
pub struct Supervisor {
    slots: Mutex<Vec<Slot>>,
    /// Sessions spawned on a caller's behalf. They have no restart policy and no state machine —
    /// they are tracked so shutdown can take them with it, and so a caller can be told what became
    /// of one.
    sessions: Mutex<SessionTable>,
    forks: Arc<ForkBroker>,
    /// Held while a start is between `fork` and having its pid recorded, so a reap cannot see the
    /// exit of a process nothing has claimed yet. Async, because the fork round-trip is awaited.
    spawn_gate: tokio::sync::Mutex<()>,
    shutting_down: AtomicBool,
}

impl Supervisor {
    /// Resolve every declared service's OS account, create the socket it declared, and record it as
    /// not-yet-started.
    ///
    /// An account that does not exist is a startup failure, not a per-start surprise: a service the
    /// supervisor can never run is a misconfiguration an operator should hear about immediately. So
    /// is a socket that cannot be bound — a service started without the listener it declared would
    /// silently serve nobody.
    pub fn new(
        config: &SupervisorConfig,
        forks: Arc<ForkBroker>,
    ) -> anyhow::Result<Arc<Supervisor>> {
        let mut slots = Vec::with_capacity(config.services.len());
        for service in &config.services {
            let mut target = spawn_broker::resolve_target_user(&service.user).map_err(|error| {
                anyhow::anyhow!(
                    "service `{}` declares user `{}`: {error}",
                    service.name,
                    service.user
                )
            })?;
            // An explicit `group:` overrides the account's primary group. Resolving it here rather
            // than at spawn time means a typo is a startup failure instead of a service that dies
            // on every restart attempt.
            if let Some(group) = &service.group {
                target.gid = spawn_broker::resolve_group_gid(group).map_err(|error| {
                    anyhow::anyhow!(
                        "service `{}` declares group `{group}`: {error}",
                        service.name
                    )
                })?;
            }
            // Again, because the override just replaced a gid that `resolve_target_user` had already
            // checked: `group: root` is the same escalation as an account aliased to uid 0, reached
            // by a different key.
            spawn_broker::refuse_root_credentials(&service.user, target.uid, target.gid)
                .map_err(|error| anyhow::anyhow!("service `{}`: {error}", service.name))?;
            // Before the service exists, so a client that races its startup finds a socket that
            // queues rather than a path that is about to work.
            let listener = match &service.socket {
                Some(socket) => Some(Arc::new(bind_service_listener(service, socket)?)),
                None => None,
            };
            slots.push(Slot {
                runtime: ServiceRuntime::new(service),
                service: service.clone(),
                target,
                started_at: None,
                listener,
            });
        }
        Ok(Arc::new(Supervisor {
            slots: Mutex::new(slots),
            sessions: Mutex::new(SessionTable::default()),
            forks,
            spawn_gate: tokio::sync::Mutex::new(()),
            shutting_down: AtomicBool::new(false),
        }))
    }

    /// The uids that own a declared service — exactly the peers the privileged socket serves.
    pub fn service_uids(&self) -> Vec<u32> {
        self.lock()
            .iter()
            .map(|slot| slot.target.uid)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Start every declared service, in declaration order.
    pub async fn start_declared_services(self: &Arc<Self>) {
        let count = self.lock().len();
        for index in 0..count {
            self.start(index).await;
        }
    }

    /// Every service and its current state, in declaration order.
    pub fn statuses(&self) -> Vec<ServiceStatus> {
        self.lock()
            .iter()
            .map(|slot| slot.runtime.status())
            .collect()
    }

    /// One service's state.
    pub fn status(&self, name: &str) -> Result<ServiceStatus, SupervisorError> {
        self.lock()
            .iter()
            .find(|slot| slot.service.name == name)
            .map(|slot| slot.runtime.status())
            .ok_or_else(|| SupervisorError::NotFound {
                name: name.to_string(),
            })
    }

    /// Run a service that is stopped or has been given up on, restoring its retry budget.
    pub async fn start_by_name(
        self: &Arc<Self>,
        name: &str,
    ) -> Result<ServiceStatus, SupervisorError> {
        let index = self.index_of(name)?;
        {
            let mut slots = self.lock();
            if slots[index].runtime.status().pid.is_some() {
                return Ok(slots[index].runtime.status());
            }
            slots[index].runtime.record_start_requested();
        }
        self.start(index).await;
        Ok(self.lock()[index].runtime.status())
    }

    /// Stop a service and suppress its restart policy for that one exit.
    pub fn stop_by_name(&self, name: &str) -> Result<ServiceStatus, SupervisorError> {
        let index = self.index_of(name)?;
        let pid = {
            let mut slots = self.lock();
            slots[index].runtime.record_stop_requested();
            slots[index].runtime.status().pid
        };
        if let Some(pid) = pid {
            signal_process(pid, TERMINATE);
        }
        Ok(self.lock()[index].runtime.status())
    }

    /// Fork a session on a caller's behalf and take ownership of it.
    ///
    /// The supervisor, not the privileged surface, owns every fork and every pid it produces: that
    /// is what lets one gate cover both the services it starts itself and the sessions it is asked
    /// for, and what makes `shutdown` able to account for all of them.
    pub async fn spawn_session(&self, plan: SpawnPlan) -> std::io::Result<u32> {
        let _in_flight = self.spawn_gate.lock().await;
        // Read before the plan is handed over. A scope becomes this session's the moment the session
        // exists, so it can be reclaimed when the session ends without a caller having to ask.
        let scope = scope_directory_of(plan.scope_procs.as_deref()).map(Path::to_path_buf);
        // No listener: socket handover is a declared managed service's privilege, never something a
        // caller can ask for.
        let pid = self.forks.spawn(plan, None).await?;
        let mut sessions = self.sessions();
        let generation = sessions.record_spawned(pid);
        if let Some(scope) = scope {
            sessions.own_scope(generation, scope);
        }
        Ok(pid)
    }

    /// What became of a session this supervisor spawned.
    ///
    /// A pid it never spawned is [`SupervisorError::Denied`], not reported on: answering would make
    /// the privileged surface a liveness oracle for every process on the host.
    pub fn session_status(&self, pid: u32) -> Result<SessionStatus, SupervisorError> {
        self.sessions().status(pid).ok_or(SupervisorError::Denied)
    }

    /// Confirm a caller may have this supervisor act on `pid` as one of its live sessions.
    ///
    /// The gate every pid-bearing request that *does* something has to pass, `AttachPid` above all:
    /// a scope's `cgroup.procs` accepts any pid the writer has the privilege to move, and the
    /// supervisor has the privilege to move all of them. Without this, a caller could point a scope
    /// whose `memory.max` was clamped small at `sshd`, at the daemon, or at the supervisor itself and
    /// have it throttled or OOM-killed — which makes the clamping that keeps `CreateScope` safe
    /// beside the point.
    pub fn require_running_session(&self, pid: u32) -> Result<(), SupervisorError> {
        self.sessions().require_running(pid)
    }

    /// Ask a session to stop, and take it out if it does not.
    ///
    /// The signal goes to the session's *process group*, which reaches the descendants a session
    /// spawned for itself. That is safe precisely because every child leads its own group
    /// ([`crate::spawn_broker::PreExecStep::LeadOwnProcessGroup`]) — a session left in the
    /// supervisor's group would turn this into a signal to the supervisor and everything under it.
    ///
    /// Returns the session's state as of the request rather than waiting for the exit: the escalation
    /// to `SIGKILL` takes the whole grace period, and no caller should have its RPC held open for
    /// that long. The exit is observed by polling [`Self::session_status`].
    pub fn stop_session(
        self: &Arc<Self>,
        pid: u32,
        grace: Duration,
    ) -> Result<SessionStatus, SupervisorError> {
        // State and identity are read as one value under one lock. Reading them separately would
        // leave a window in which the session exits, is reaped, and has its pid reissued between the
        // two reads, so the kill below would be armed against the wrong incarnation from the start.
        let (generation, status) = self
            .sessions()
            .snapshot(pid)
            .ok_or(SupervisorError::Denied)?;
        if status.state == SessionState::Exited {
            return Ok(status);
        }

        log::info!(
            target: "tddy_supervisor::supervisor",
            "stopping session {pid}, {grace:?} grace"
        );
        signal_process_group(pid, TERMINATE);

        let supervisor = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(grace).await;
            // Read from the session table, not from the kernel, and match the generation as well as
            // the pid. A pid alone is not enough: this session can have exited, been reaped, and had
            // its pid reissued to a different session within the grace period, and `SIGKILL` to a
            // process *group* would take that session and everything it had spawned with it.
            if supervisor.sessions().is_running_generation(pid, generation) {
                log::warn!(
                    target: "tddy_supervisor::supervisor",
                    "killing session {pid}, which outlived its grace period"
                );
                signal_process_group(pid, KILL);
            }
        });
        Ok(status)
    }

    /// Reap everything that has exited and act on it.
    ///
    /// Called once per `SIGCHLD` (which coalesces, hence the drain inside [`reaper`]) and again
    /// from the shutdown sequence, which needs its children reaped before it can call them gone.
    pub async fn reap(self: &Arc<Self>) {
        // Held across the whole drain so no exit is attributed while a start is in flight. A
        // service that dies the instant it is exec'd would otherwise be reaped before its pid was
        // recorded, be attributed to nobody, and leave the service stuck in `Starting` forever
        // waiting for a process that no longer exists.
        let _in_flight = self.spawn_gate.lock().await;
        for child in reaper::reap_exited_children() {
            let Some((index, name, outcome)) = self.record_exit(child.pid) else {
                // A session, or something the supervisor no longer has a claim on. Recording the
                // exit rather than forgetting the pid is what lets a caller ask afterwards: its poll
                // cannot arrive before this point, because the status it wants is what `waitpid`
                // just produced.
                let reclaimed = self.sessions().record_exit(child.pid, child.exit_code());
                if let Some(scope) = reclaimed {
                    self.reclaim_scope(scope);
                }
                log::debug!(
                    target: "tddy_supervisor::supervisor",
                    "reaped pid {} ({})",
                    child.pid,
                    child.describe()
                );
                continue;
            };
            self.react_to_exit(index, &name, outcome, &child);
        }
    }

    /// Terminate every managed service and session, then wait for them.
    ///
    /// Restarts are suppressed before the first signal goes out, so an exit observed between the
    /// two is not mistaken for a crash worth recovering from.
    pub async fn shutdown(self: &Arc<Self>, grace: Duration) {
        self.shutting_down.store(true, Ordering::SeqCst);

        let mut pids = Vec::new();
        {
            let mut slots = self.lock();
            for slot in slots.iter_mut() {
                slot.runtime.record_stop_requested();
                if let Some(pid) = slot.runtime.status().pid {
                    pids.push(pid);
                }
            }
        }
        pids.extend(self.running_session_pids());

        log::info!(
            target: "tddy_supervisor::supervisor",
            "terminating {} child process(es), {grace:?} grace",
            pids.len()
        );
        for pid in &pids {
            signal_process(*pid, TERMINATE);
        }
        if self.await_children_gone(grace).await {
            return;
        }

        let survivors = self.live_children();
        log::warn!(
            target: "tddy_supervisor::supervisor",
            "killing {} child process(es) that outlived the grace period",
            survivors.len()
        );
        for pid in survivors {
            signal_process(pid, KILL);
        }
        if !self.await_children_gone(KILL_TIMEOUT).await {
            log::error!(
                target: "tddy_supervisor::supervisor",
                "children {:?} are still present after SIGKILL",
                self.live_children()
            );
        }
    }

    // ---------------------------------------------------------------------------------------
    // Internals
    // ---------------------------------------------------------------------------------------

    /// Fork, drop privilege and exec one service, then start its startup-grace timer.
    async fn start(self: &Arc<Self>, index: usize) {
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        let (name, plan, handover) = {
            let slots = self.lock();
            let slot = &slots[index];
            (
                slot.service.name.clone(),
                slot.spawn_plan(),
                slot.socket_handover(),
            )
        };

        // Fork and record the pid without letting a reap in between — see [`Self::reap`].
        let in_flight = self.spawn_gate.lock().await;
        let spawned = self.forks.spawn(plan, handover).await;
        match spawned {
            Ok(pid) => {
                {
                    let mut slots = self.lock();
                    slots[index].runtime.record_started(pid);
                    slots[index].started_at = Some(Instant::now());
                }
                drop(in_flight);
                log::info!(
                    target: "tddy_supervisor::supervisor",
                    "started service '{name}' as pid {pid}"
                );
                self.watch_startup(index, pid);
            }
            Err(error) => {
                drop(in_flight);
                // A service that cannot be exec'd goes through the same backoff as one that dies
                // after exec — otherwise a transient failure would never be retried.
                log::error!(
                    target: "tddy_supervisor::supervisor",
                    "could not start service '{name}': {error}"
                );
                let outcome = {
                    let mut slots = self.lock();
                    slots[index].started_at = None;
                    slots[index].runtime.record_exit(Duration::ZERO)
                };
                self.schedule_restart(index, &name, outcome);
            }
        }
    }

    /// Promote a service to `Running` once it has survived [`STARTUP_GRACE_PERIOD`].
    fn watch_startup(self: &Arc<Self>, index: usize, pid: u32) {
        let supervisor = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(STARTUP_GRACE_PERIOD).await;
            let mut slots = supervisor.lock();
            // Only the start this timer belongs to. A service that already died and restarted has
            // a different pid, and that start must serve its own grace period.
            if slots[index].runtime.status().pid == Some(pid) {
                slots[index].runtime.record_survived_startup();
            }
        });
    }

    /// Attribute an exit to a declared service, if it belongs to one.
    fn record_exit(&self, pid: u32) -> Option<(usize, String, ExitOutcome)> {
        let mut slots = self.lock();
        let index = slots
            .iter()
            .position(|slot| slot.runtime.status().pid == Some(pid))?;
        let slot = &mut slots[index];
        let uptime = slot
            .started_at
            .take()
            .map(|started| started.elapsed())
            .unwrap_or_default();
        let outcome = slot.runtime.record_exit(uptime);
        Some((index, slot.service.name.clone(), outcome))
    }

    fn react_to_exit(
        self: &Arc<Self>,
        index: usize,
        name: &str,
        outcome: ExitOutcome,
        child: &reaper::ReapedChild,
    ) {
        log::info!(
            target: "tddy_supervisor::supervisor",
            "service '{name}' (pid {}) exited with {}",
            child.pid,
            child.describe()
        );
        self.schedule_restart(index, name, outcome);
    }

    fn schedule_restart(self: &Arc<Self>, index: usize, name: &str, outcome: ExitOutcome) {
        match outcome {
            ExitOutcome::Restart { after } => {
                log::info!(
                    target: "tddy_supervisor::supervisor",
                    "restarting service '{name}' in {after:?}"
                );
                let supervisor = Arc::clone(self);
                tokio::spawn(async move {
                    tokio::time::sleep(after).await;
                    supervisor.start(index).await;
                });
            }
            ExitOutcome::GaveUp => log::error!(
                target: "tddy_supervisor::supervisor",
                "service '{name}' exhausted its retry budget and will not be restarted"
            ),
            ExitOutcome::StoppedOnRequest => log::info!(
                target: "tddy_supervisor::supervisor",
                "service '{name}' stopped on request"
            ),
        }
    }

    /// Remove the cgroup scope a session was spawned into, now that the session is gone.
    ///
    /// The first attempt is made here, inline: the common case is a session that left nothing behind,
    /// and one `rmdir` costs less than a task. `EBUSY` is *not* retried here. The kernel refuses to
    /// remove a cgroup while any process remains in it, a session's own descendants can outlive the
    /// session leader, and every other exit on the host — the daemon's restart included — is queued
    /// behind this reap. So the waiting happens in a task with a bounded grace
    /// ([`SCOPE_REMOVAL_GRACE`]), and a scope that never empties is reported rather than retried
    /// forever or forgotten: an operator can then find out what is still living in it.
    ///
    /// Not fatal to anything. The session is over either way, the caller already has its exit status,
    /// and `DestroyScope` remains available for a scope this could not remove. A retry still waiting
    /// when the supervisor exits goes with the runtime, which is correct: on a real host the delegated
    /// subtree is torn down with the unit, so there is nothing left to remove.
    fn reclaim_scope(&self, scope: PathBuf) {
        match cgroup_broker::remove_scope_dir(&scope) {
            ScopeRemoval::Removed => log::debug!(
                target: "tddy_supervisor::supervisor",
                "removed the scope {} of a session that has exited",
                scope.display()
            ),
            ScopeRemoval::Failed { message } => log::warn!(
                target: "tddy_supervisor::supervisor",
                "the scope of a session that has exited is left behind: {message}"
            ),
            ScopeRemoval::StillPopulated => {
                tokio::spawn(async move {
                    let deadline = Instant::now() + SCOPE_REMOVAL_GRACE;
                    loop {
                        tokio::time::sleep(SCOPE_REMOVAL_RETRY_INTERVAL).await;
                        match cgroup_broker::remove_scope_dir(&scope) {
                            ScopeRemoval::Removed => {
                                log::debug!(
                                    target: "tddy_supervisor::supervisor",
                                    "removed the scope {} once the last of its processes had gone",
                                    scope.display()
                                );
                                return;
                            }
                            ScopeRemoval::Failed { message } => {
                                log::warn!(
                                    target: "tddy_supervisor::supervisor",
                                    "the scope of a session that has exited is left behind: {message}"
                                );
                                return;
                            }
                            ScopeRemoval::StillPopulated if Instant::now() >= deadline => {
                                log::warn!(
                                    target: "tddy_supervisor::supervisor",
                                    "the scope {} still holds processes {SCOPE_REMOVAL_GRACE:?} after \
                                     its session exited, so it is left behind; its `cgroup.procs` \
                                     names what is still running in it",
                                    scope.display()
                                );
                                return;
                            }
                            ScopeRemoval::StillPopulated => continue,
                        }
                    }
                });
            }
        }
    }

    /// Poll until nothing the supervisor spawned is left, or `budget` runs out.
    async fn await_children_gone(self: &Arc<Self>, budget: Duration) -> bool {
        let deadline = Instant::now() + budget;
        loop {
            self.reap().await;
            if self.live_children().is_empty() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(REAP_POLL_INTERVAL).await;
        }
    }

    fn live_children(&self) -> Vec<u32> {
        let mut pids: Vec<u32> = self
            .lock()
            .iter()
            .filter_map(|slot| slot.runtime.status().pid)
            .collect();
        pids.extend(self.running_session_pids());
        pids
    }

    /// Sessions that have not been reaped yet. An exited session whose status is being retained for
    /// a caller is not a child anymore, so shutdown must not wait for it.
    fn running_session_pids(&self) -> Vec<u32> {
        self.sessions().running_pids()
    }

    fn sessions(&self) -> MutexGuard<'_, SessionTable> {
        lock_or_recover(&self.sessions, "session table")
    }

    fn index_of(&self, name: &str) -> Result<usize, SupervisorError> {
        self.lock()
            .iter()
            .position(|slot| slot.service.name == name)
            .ok_or_else(|| SupervisorError::NotFound {
                name: name.to_string(),
            })
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Slot>> {
        lock_or_recover(&self.slots, "service slots")
    }
}

/// The cgroup scope directory a spawn plan places its child in, if any.
///
/// A plan carries the scope as the `cgroup.procs` the child writes itself into, which is
/// `<scope>/cgroup.procs` by construction ([`crate::cgroup_broker::CgroupBroker::scope_procs_path`]).
/// The directory around that file is what becomes garbage when the session ends. Pure, and taking the
/// path rather than the plan, so the derivation is assertable on its own.
fn scope_directory_of(scope_procs: Option<&Path>) -> Option<&Path> {
    scope_procs.and_then(Path::parent)
}

/// Take a lock, and take it back if a panic under an earlier guard poisoned it.
///
/// The workspace unwinds on panic, so a panic under either of this module's guards does not end the
/// process: it poisons the lock. Propagating that — which is what `expect` did — turns one panic into
/// a supervisor that never works again. The reaper task ([`crate::run`]) dies on its next lock, so
/// nothing is reaped, nothing is restarted and no session is accounted for, while systemd still
/// reports the unit `active` because the process is alive and still answering `ListServices`.
///
/// Degrading is the lesser evil of the two available. Failing hard would mean the supervisor killing
/// itself over a bookkeeping panic, and every child dies with it (`PR_SET_PDEATHSIG`) — the daemon and
/// every live session — for a fault the data itself almost certainly survived: both guards protect
/// plain collections, and neither mutation sequence leaves a half-built value behind that a later
/// reader could act on.
///
/// What an operator sees is one `ERROR` line per recovery in the journal, naming the lock, plus the
/// panic that caused it. What they do not see is a unit that reports `active` and supervises nothing.
fn lock_or_recover<'a, T>(lock: &'a Mutex<T>, what: &str) -> MutexGuard<'a, T> {
    lock.lock().unwrap_or_else(|poisoned| {
        log::error!(
            target: "tddy_supervisor::supervisor",
            "recovered the poisoned {what} lock after a panic; the supervisor keeps supervising \
             rather than stopping dead, but the panic above is a bug worth reporting"
        );
        poisoned.into_inner()
    })
}

impl Slot {
    fn spawn_plan(&self) -> SpawnPlan {
        SpawnPlan {
            program: self.service.exec_start.clone(),
            args: self.service.args.clone(),
            env: self.service.env.clone(),
            working_dir: self
                .service
                .working_dir
                .clone()
                .unwrap_or_else(|| self.target.home.clone()),
            target: self.target.clone(),
            // Managed services live in the supervisor's own cgroup: they are the host's services,
            // not somebody's session, and nothing has asked for them to be limited.
            scope_procs: None,
            // So a managed service's output reaches the same journal the supervisor's does.
            inherit_output: true,
            // A declared service is root's own decision, and an operator who sets a variable on the
            // supervisor unit means it for the services under it.
            environment: EnvironmentBase::Inherited,
            // A managed service is a host service, not a sandboxed session.
            sandbox: None,
        }
    }

    /// The listener this service's socket declaration created, ready to hand to the next start.
    fn socket_handover(&self) -> Option<SocketHandover> {
        self.listener.clone().map(SocketHandover::new)
    }
}

/// Create the listening socket a service declared, as root, before the service exists.
///
/// This is the job `tddy-daemon.socket` used to do: root binds the socket so it can live in a
/// directory the service user cannot write, and the service is handed the listener instead of trying
/// to create one. The service's own account is never the owner — the mode, and the group when one is
/// declared, are the whole access grant.
///
/// Deliberately a second implementation of what `server::bind_privileged_listener` does for the
/// supervisor's own socket: that one also has to cope with systemd handing over a listener, which a
/// managed service's socket never involves.
// TODO(supervisor): fold both into one helper owned by `socket.rs`, so ownership and mode are
// applied by exactly one piece of code.
fn bind_service_listener(
    service: &ManagedService,
    socket: &ServiceSocket,
) -> anyhow::Result<UnixListener> {
    let mode = socket
        .mode_bits()
        .map_err(|error| anyhow::anyhow!("service `{}`: {error}", service.name))?;

    if let Some(parent) = socket.path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            anyhow::anyhow!(
                "service `{}`: create socket directory {}: {error}",
                service.name,
                parent.display()
            )
        })?;
    }
    // A socket left behind by a previous run would make `bind` fail with EADDRINUSE.
    let _ = std::fs::remove_file(&socket.path);
    let listener = UnixListener::bind(&socket.path).map_err(|error| {
        anyhow::anyhow!(
            "service `{}`: bind {}: {error}",
            service.name,
            socket.path.display()
        )
    })?;
    apply_socket_ownership(&socket.path, socket.group.as_deref(), mode)
        .map_err(|error| anyhow::anyhow!("service `{}`: {error}", service.name))?;

    log::info!(
        target: "tddy_supervisor::supervisor",
        "created {} (mode {mode:o}) for service '{}'",
        socket.path.display(),
        service.name
    );
    Ok(listener)
}

/// Give a socket the group and mode its declaration asks for.
///
/// The socket stays owned by root, so a group that cannot be resolved is an error rather than
/// something to shrug at: it is the grant.
fn apply_socket_ownership(
    path: &std::path::Path,
    group: Option<&str>,
    mode: u32,
) -> anyhow::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    if let Some(group) = group {
        let gid = spawn_broker::resolve_group_gid(group)
            .map_err(|error| anyhow::anyhow!("socket group `{group}`: {error}"))?;
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| anyhow::anyhow!("socket path contains a nul byte"))?;
        // SAFETY: `c_path` outlives the call; `-1` leaves the owning uid untouched.
        let changed = unsafe { libc::chown(c_path.as_ptr(), u32::MAX, gid) };
        if changed != 0 {
            return Err(anyhow::anyhow!(
                "chown {} to group `{group}`: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
    }

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| anyhow::anyhow!("chmod {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SocketConfig, SpawnPolicy};
    use crate::test_util::a_managed_service;

    /// A pid the kernel is free to hand to anybody, used as the pid of a session that exists.
    const SESSION_PID: u32 = 4242;
    /// Some other process on the host — `sshd`, the daemon, the supervisor itself.
    const A_STRANGERS_PID: u32 = 1337;

    fn a_session_table() -> SessionTable {
        SessionTable::default()
    }

    // -----------------------------------------------------------------------------------------
    // Which pids a caller may have the supervisor act on
    // -----------------------------------------------------------------------------------------

    #[test]
    fn permits_acting_on_a_session_it_spawned_and_has_not_reaped() {
        // Given
        let mut sessions = a_session_table();
        sessions.record_spawned(SESSION_PID);

        // When
        let permitted = sessions.require_running(SESSION_PID);

        // Then
        assert_eq!(permitted, Ok(()));
    }

    #[test]
    fn refuses_acting_on_a_pid_it_never_spawned() {
        // Given a supervisor that has spawned one session, asked about a different pid.
        let mut sessions = a_session_table();
        sessions.record_spawned(SESSION_PID);

        // When
        let permitted = sessions.require_running(A_STRANGERS_PID);

        // Then — this is what stands between `AttachPid` and moving `sshd` into a scope whose
        // `memory.max` was clamped small.
        assert_eq!(permitted, Err(SupervisorError::Denied));
    }

    #[test]
    fn refuses_acting_on_an_exited_session_whose_pid_may_now_be_somebody_elses() {
        // Given a session whose status is still retained for a caller that has not polled yet.
        let mut sessions = a_session_table();
        sessions.record_spawned(SESSION_PID);
        sessions.record_exit(SESSION_PID, Some(0));

        // When
        let permitted = sessions.require_running(SESSION_PID);

        // Then — reporting on it is safe, acting on it is not: once it is reaped the kernel may have
        // given that pid to anybody.
        assert_eq!(permitted, Err(SupervisorError::Denied));
    }

    // -----------------------------------------------------------------------------------------
    // Telling one incarnation of a pid from the next
    // -----------------------------------------------------------------------------------------

    #[test]
    fn recognises_the_session_a_stop_request_was_made_about() {
        // Given
        let mut sessions = a_session_table();
        let generation = sessions.record_spawned(SESSION_PID);

        // When / Then
        assert!(
            sessions.is_running_generation(SESSION_PID, generation),
            "the session that was just spawned must be the one a stop request is about"
        );
    }

    #[test]
    fn stops_recognising_a_stop_request_once_its_pid_belongs_to_a_later_session() {
        // Given a session that exited and was reaped, and a new session the kernel handed the same
        // pid to — ordinary within a twenty-second grace period on a host with pid_max 32768.
        let mut sessions = a_session_table();
        let stopped = sessions.record_spawned(SESSION_PID);
        sessions.record_exit(SESSION_PID, Some(0));
        sessions.record_spawned(SESSION_PID);

        // When / Then — the deferred `SIGKILL` armed for the first session must not fire, or it takes
        // the second session and every process it had spawned with it.
        assert!(
            !sessions.is_running_generation(SESSION_PID, stopped),
            "a stop request must not carry over to whoever inherited the pid"
        );
    }

    #[test]
    fn stops_recognising_a_stop_request_for_a_session_that_has_already_exited() {
        // Given
        let mut sessions = a_session_table();
        let generation = sessions.record_spawned(SESSION_PID);
        sessions.record_exit(SESSION_PID, Some(0));

        // When / Then
        assert!(
            !sessions.is_running_generation(SESSION_PID, generation),
            "an exited session must not be signalled"
        );
    }

    #[test]
    fn gives_every_session_an_identity_of_its_own() {
        // Given
        let mut sessions = a_session_table();

        // When
        let first = sessions.record_spawned(SESSION_PID);
        sessions.record_exit(SESSION_PID, Some(0));
        let second = sessions.record_spawned(SESSION_PID);

        // Then — a pid is not an identity, so two sessions that shared one are still two sessions.
        assert_ne!(first, second);
    }

    // -----------------------------------------------------------------------------------------
    // Reclaiming the cgroup scope a session was spawned into
    // -----------------------------------------------------------------------------------------

    /// The scope directory a session was spawned into, as [`CgroupBroker::scope_procs_path`] names it.
    fn a_scope_dir(name: &str) -> PathBuf {
        PathBuf::from(format!("/sys/fs/cgroup/tddy.slice/tddy-{name}.scope"))
    }

    #[test]
    fn takes_the_scope_a_session_joins_from_the_procs_file_it_was_handed() {
        // Given the `cgroup.procs` a spawn plan carries for a session that joins a scope.
        let procs = a_scope_dir("session-alpha").join("cgroup.procs");

        // When
        let scope = scope_directory_of(Some(&procs));

        // Then — the plan names the file the child writes itself into; the directory around it is what
        // becomes garbage when the session ends.
        assert_eq!(scope, Some(a_scope_dir("session-alpha").as_path()));
    }

    #[test]
    fn takes_no_scope_for_a_session_that_joins_none() {
        // Given / When
        let scope = scope_directory_of(None);

        // Then
        assert_eq!(scope, None);
    }

    #[test]
    fn reclaims_the_scope_of_a_session_that_has_exited() {
        // Given a session spawned into a scope of its own.
        let mut sessions = a_session_table();
        let generation = sessions.record_spawned(SESSION_PID);
        sessions.own_scope(generation, a_scope_dir("session-alpha"));

        // When
        let reclaimed = sessions.record_exit(SESSION_PID, Some(0));

        // Then — nobody had to ask: the scope became garbage the moment its session did.
        assert_eq!(reclaimed, Some(a_scope_dir("session-alpha")));
    }

    #[test]
    fn reclaims_no_scope_for_a_session_that_was_never_spawned_into_one() {
        // Given a session with no scope, on a supervisor that may well hold scopes a caller created
        // with `CreateScope` and attached nothing to.
        let mut sessions = a_session_table();
        sessions.record_spawned(SESSION_PID);

        // When
        let reclaimed = sessions.record_exit(SESSION_PID, Some(0));

        // Then — only the association makes a scope garbage. A scope nothing was spawned into is the
        // caller's to destroy, and sweeping it would remove a directory it is still filling.
        assert_eq!(reclaimed, None);
    }

    #[test]
    fn reclaims_no_scope_for_a_pid_that_was_never_a_session() {
        // Given a supervisor whose only session is somebody else's pid away.
        let mut sessions = a_session_table();
        let generation = sessions.record_spawned(SESSION_PID);
        sessions.own_scope(generation, a_scope_dir("session-alpha"));

        // When a managed service, or anything else the supervisor forked, is reaped.
        let reclaimed = sessions.record_exit(A_STRANGERS_PID, Some(0));

        // Then
        assert_eq!(reclaimed, None);
    }

    #[test]
    fn keeps_the_exit_status_of_a_session_whose_scope_it_reclaimed() {
        // Given
        let mut sessions = a_session_table();
        let generation = sessions.record_spawned(SESSION_PID);
        sessions.own_scope(generation, a_scope_dir("session-alpha"));

        // When
        sessions.record_exit(SESSION_PID, Some(3));

        // Then — the directory and the status have different lifetimes. A caller polling for what
        // became of its session must still get an answer after the scope is gone.
        assert_eq!(
            sessions.status(SESSION_PID),
            Some(SessionStatus {
                pid: SESSION_PID,
                state: SessionState::Exited,
                exit_code: Some(3),
            })
        );
    }

    #[test]
    fn reclaims_a_session_scope_once_and_not_again() {
        // Given a session whose exit has already been recorded.
        let mut sessions = a_session_table();
        let generation = sessions.record_spawned(SESSION_PID);
        sessions.own_scope(generation, a_scope_dir("session-alpha"));
        sessions.record_exit(SESSION_PID, Some(0));

        // When the same pid is reaped again — the shutdown sequence drains repeatedly.
        let reclaimed = sessions.record_exit(SESSION_PID, Some(0));

        // Then — a scope handed out twice is a directory removed twice, and by then the kernel may
        // have given that name to a later session.
        assert_eq!(reclaimed, None);
    }

    #[test]
    fn reclaims_no_scope_on_behalf_of_a_later_session_that_inherited_a_pid() {
        // Given a session that exited and had its scope reclaimed, and a new session the kernel handed
        // the same pid to — this one spawned into no scope at all.
        let mut sessions = a_session_table();
        let exited = sessions.record_spawned(SESSION_PID);
        sessions.own_scope(exited, a_scope_dir("session-alpha"));
        sessions.record_exit(SESSION_PID, Some(0));
        sessions.record_spawned(SESSION_PID);

        // When
        let reclaimed = sessions.record_exit(SESSION_PID, Some(0));

        // Then — a scope belongs to the incarnation that was spawned into it, not to whoever holds its
        // pid afterwards.
        assert_eq!(reclaimed, None);
    }

    // -----------------------------------------------------------------------------------------
    // Sweeping a real session's scope on the way through the reap path
    // -----------------------------------------------------------------------------------------

    /// A supervisor that manages no declared service. What it does with the sessions it is *asked*
    /// for is the subject here, and a declaration would only add accounts to resolve and children to
    /// start.
    #[cfg(target_os = "linux")]
    fn a_supervisor_managing_nothing() -> Arc<Supervisor> {
        let config = SupervisorConfig {
            socket: SocketConfig {
                path: PathBuf::from("/run/tddy-supervisor.sock"),
                group: None,
                mode: "0660".to_string(),
            },
            services: Vec::new(),
            spawn_policy: SpawnPolicy::default(),
            cgroup: crate::config::CgroupPolicy::default(),
            shutdown_grace_secs: 1,
        };
        let forks = Arc::new(spawn_broker::ForkBroker::start().expect("start the fork thread"));
        Supervisor::new(&config, forks).expect("build a supervisor")
    }

    /// A session that exits the instant it starts, as a script under `directory`.
    ///
    /// A `/bin/sh` script rather than a binary from `PATH`, which is the only interpreter this
    /// crate's tests already assume.
    #[cfg(target_os = "linux")]
    fn a_session_that_exits_immediately(directory: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = directory.join("session.sh");
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write the session script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("make the session script executable");
        path
    }

    /// A plan for a session this test process can really fork and exec.
    ///
    /// The target account is the one already running, so no privilege drop is planned — pinned by
    /// `spawn_broker::tests::omits_the_privilege_drop_when_it_already_runs_as_the_target_user` — and
    /// the whole path runs unprivileged. `name` and `home` become the child's `USER` and `HOME`, which
    /// nothing here looks at.
    #[cfg(target_os = "linux")]
    fn a_plan_running(program: PathBuf, scope_procs: Option<PathBuf>) -> SpawnPlan {
        // SAFETY: reads this process's own effective ids and nothing else.
        let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
        SpawnPlan {
            program,
            args: Vec::new(),
            env: BTreeMap::new(),
            working_dir: std::env::temp_dir(),
            target: TargetUser {
                uid,
                gid,
                name: std::ffi::CString::new("tester").expect("a target user name"),
                home: std::env::temp_dir(),
                groups: Vec::new(),
            },
            scope_procs,
            inherit_output: false,
            environment: EnvironmentBase::Minimal,
            sandbox: None,
        }
    }

    /// The state the kernel leaves a cgroup directory in once the last process has left it: removable.
    ///
    /// A cgroup's control files are not directory entries, so an emptied cgroup is an empty directory.
    /// Under the plain-directory base an operator's `base_override` (and every test) can name, the pid
    /// the child wrote into `cgroup.procs` is an ordinary file, so it is removed here to give the
    /// stand-in the shape the real thing has. Nothing about the supervisor is faked: it made the
    /// association itself, from the plan it was handed.
    #[cfg(target_os = "linux")]
    fn empty_the_scope_the_way_the_kernel_does(scope: &Path) {
        std::fs::remove_file(scope.join("cgroup.procs"))
            .expect("the child should have joined the scope before exec");
    }

    /// Reap until `pid` is recorded as exited, which is the point at which everything the reap path
    /// does about that session — its scope included — has happened.
    ///
    /// ⚠️ `waitpid(-1)` is process-wide, so whichever caller reaps first collects *every* exited child
    /// of the test binary. The test below must therefore stay the only unit test in this crate that
    /// forks a child and reaps it: a second one would steal this one's exit and both would time out
    /// here. Everything else about a session's lifetime is asserted against [`SessionTable`] directly,
    /// and against the real binary in `tests/`, where each supervisor is its own process.
    #[cfg(target_os = "linux")]
    async fn await_session_exit(supervisor: &Arc<Supervisor>, pid: u32) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            supervisor.reap().await;
            if supervisor.session_status(pid).map(|status| status.state) == Ok(SessionState::Exited)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("session {pid} never exited");
    }

    /// The one test here that forks a child all the way through its pre-exec plan, and that plan
    /// opens with `PR_SET_PDEATHSIG` — Linux's, and refused elsewhere rather than skipped, so off
    /// Linux the spawn fails before there is a session to reap. What the scope's lifetime does
    /// *after* that is asserted against [`SessionTable`] on every host.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn removes_the_scope_of_a_session_once_that_session_has_exited() {
        // Given a scope on disk and a session spawned into it, which exits straight away.
        let workspace = tempfile::TempDir::new().expect("create a workspace");
        let scope = workspace.path().join("tddy-session-alpha.scope");
        std::fs::create_dir(&scope).expect("create the scope directory");
        let supervisor = a_supervisor_managing_nothing();
        let pid = supervisor
            .spawn_session(a_plan_running(
                a_session_that_exits_immediately(workspace.path()),
                Some(scope.join("cgroup.procs")),
            ))
            .await
            .expect("spawn a session into the scope");
        empty_the_scope_the_way_the_kernel_does(&scope);

        // When the session is reaped. Nobody asks for the scope to be destroyed.
        await_session_exit(&supervisor, pid).await;

        // Then
        assert!(
            !scope.exists(),
            "the scope {} outlived the session it was created for",
            scope.display()
        );
    }

    // -----------------------------------------------------------------------------------------
    // Surviving a panic under a lock
    // -----------------------------------------------------------------------------------------

    #[test]
    fn recovers_a_poisoned_lock_instead_of_disabling_the_supervisor_for_good() {
        // Given a lock poisoned by a panic under an earlier guard.
        let slots: Mutex<Vec<u32>> = Mutex::new(vec![SESSION_PID]);
        std::thread::scope(|threads| {
            // The panic message this prints belongs to the scenario, not to a failure.
            let poisoner = threads.spawn(|| {
                let _guard = slots.lock().expect("take the lock before poisoning it");
                panic!("a bug under the guard");
            });
            assert!(poisoner.join().is_err(), "the poisoning panic must happen");
        });

        // When
        let recovered = lock_or_recover(&slots, "service slots");

        // Then — the alternative is a supervisor that reaps nothing and restarts nothing for the rest
        // of its life while systemd reports the unit `active`.
        assert_eq!(*recovered, vec![SESSION_PID]);
    }

    // -----------------------------------------------------------------------------------------
    // Refusing to manage a privileged service
    // -----------------------------------------------------------------------------------------

    #[tokio::test]
    async fn refuses_to_manage_a_service_whose_account_resolves_to_root() {
        // Given a declaration that passed the loader's name check — this one is built directly, which
        // is exactly the position an account aliased to uid 0 under another name would be in.
        let config = SupervisorConfig {
            socket: SocketConfig {
                path: std::path::PathBuf::from("/run/tddy-supervisor.sock"),
                group: None,
                mode: "0660".to_string(),
            },
            services: vec![a_managed_service().running_as("root").build()],
            spawn_policy: SpawnPolicy::default(),
            cgroup: crate::config::CgroupPolicy::default(),
            shutdown_grace_secs: 20,
        };
        let forks = Arc::new(spawn_broker::ForkBroker::start().expect("start the fork thread"));

        // When
        let started = Supervisor::new(&config, forks);

        // Then — startup fails rather than the service running with the privilege the supervisor
        // exists to keep to itself.
        assert_eq!(
            started
                .err()
                .expect("a root service must not be managed")
                .to_string(),
            "service `tddy-daemon` declares user `root`: account `root` resolves to uid 0; the \
             supervisor is the only privileged process on the host"
        );
    }
}
