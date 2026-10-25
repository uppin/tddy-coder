//! The mini-init: start declared services in order, keep them alive, take them down together.
//!
//! All mutable state lives behind one lock, and the decisions about it are [`ServiceRuntime`]'s —
//! this module only performs them. The lock is a `std::sync::Mutex` and is never held across an
//! `await`, because the two things that happen under it (reading a spawn plan, recording an exit)
//! are pure bookkeeping.

mod service_restart;
mod service_slot;
mod session_table;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::config::SupervisorConfig;
use crate::error::SupervisorError;
use crate::host_socket;
use crate::reaper;
use crate::service::ServiceStatus;
use crate::services::ServiceRuntime;
use crate::signals::{signal_process, KILL, TERMINATE};
use crate::spawn_broker::accounts;
use crate::spawn_broker::fork_broker::ForkBroker;
use crate::supervisor::service_slot::bind_service_listener;
use crate::supervisor::service_slot::Slot;
use crate::supervisor::session_table::SessionTable;

/// How often the shutdown sequence re-checks whether its children are gone.
const REAP_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long a `SIGKILL`ed process gets to disappear before the supervisor stops waiting for it.
/// `SIGKILL` is not refusable, so this only covers the kernel doing the work.
const KILL_TIMEOUT: Duration = Duration::from_secs(2);

/// How long a scope's leftover processes get to disappear before its directory is left behind.
///
/// `rmdir` on a cgroup fails with `EBUSY` while *any* process remains in it, and a session's own
/// descendants routinely outlive the session leader by a moment — a shell that exits before the
/// children it signalled do. Waiting is therefore normal and short. Waiting *forever* is not: a scope
/// whose processes never leave is a leak, and this is how long the supervisor tries before saying so.
const SCOPE_REMOVAL_GRACE: Duration = Duration::from_secs(2);

/// How often a scope that still holds processes is retried within [`SCOPE_REMOVAL_GRACE`].
const SCOPE_REMOVAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// Starts, reaps, restarts and shuts down the declared services.
pub struct Supervisor {
    slots: Mutex<Vec<Slot>>,
    /// Sessions spawned on a caller's behalf. They have no restart policy and no state machine —
    /// they are tracked so shutdown can take them with it, and so a caller can be told what became
    /// of one.
    sessions: Mutex<SessionTable>,
    forks: Arc<ForkBroker>,
    /// `socket.path` from the config: the privileged socket every managed service is told about,
    /// through [`EnvironmentBase::Inherited`](crate::spawn_broker::EnvironmentBase::Inherited).
    supervisor_socket: PathBuf,
    /// Held while a start is between `fork` and having its pid recorded, so a reap cannot see the
    /// exit of a process nothing has claimed yet. Async, because the fork round-trip is awaited.
    spawn_gate: tokio::sync::Mutex<()>,
    /// Woken after every session exit is recorded, so a caller waiting on one
    /// ([`Self::wait_for_session_exit`]) is told instead of polling.
    session_exited: tokio::sync::Notify,
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
            let mut target = accounts::resolve_target_user(&service.user).map_err(|error| {
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
                target.gid = accounts::resolve_group_gid(group).map_err(|error| {
                    anyhow::anyhow!(
                        "service `{}` declares group `{group}`: {error}",
                        service.name
                    )
                })?;
            }
            // Again, because the override just replaced a gid that `resolve_target_user` had already
            // checked: `group: root` is the same escalation as an account aliased to uid 0, reached
            // by a different key.
            accounts::refuse_root_credentials(&service.user, target.uid, target.gid)
                .map_err(|error| anyhow::anyhow!("service `{}`: {error}", service.name))?;
            // Before the service exists, so a client that races its startup finds a socket that
            // queues rather than a path that is about to work.
            let listener = match &service.socket {
                Some(socket) => Some(Arc::new(bind_service_listener(service, socket)?)),
                None => None,
            };
            let host_listeners = service
                .host_sockets
                .iter()
                .map(|host| {
                    let listener = host_socket::bind_host_socket(&service.name, host)?;
                    Ok((host.user.clone(), Arc::new(listener)))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            slots.push(Slot {
                runtime: ServiceRuntime::new(service),
                service: service.clone(),
                target,
                started_at: None,
                listener,
                host_listeners,
            });
        }
        Ok(Arc::new(Supervisor {
            slots: Mutex::new(slots),
            sessions: Mutex::new(SessionTable::default()),
            forks,
            supervisor_socket: config.socket.path.clone(),
            spawn_gate: tokio::sync::Mutex::new(()),
            session_exited: tokio::sync::Notify::new(),
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
                self.session_exited.notify_waiters();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::spawn_policy::SpawnPolicy;
    use crate::config::{ManagedService, ServiceSocket, SocketConfig};
    use crate::request::{SessionState, SessionStatus};
    use std::path::PathBuf;
    // Only `a_plan_running`, which is Linux-only, builds a plan by hand.
    #[cfg(target_os = "linux")]
    use crate::spawn_broker::accounts::TargetUser;
    #[cfg(target_os = "linux")]
    use crate::spawn_broker::{ChildOutput, EnvironmentBase, SpawnPlan};
    use crate::supervisor::session_table::SessionTable;
    use crate::test_util::a_managed_service;
    #[cfg(target_os = "linux")]
    use std::collections::BTreeMap;

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
        let forks = Arc::new(
            crate::spawn_broker::fork_broker::ForkBroker::start().expect("start the fork thread"),
        );
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
            output: ChildOutput::Discard,
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
        let forks = Arc::new(
            crate::spawn_broker::fork_broker::ForkBroker::start().expect("start the fork thread"),
        );

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

    // -----------------------------------------------------------------------------------------
    // Per-OS-user host sockets
    // -----------------------------------------------------------------------------------------

    /// The account this test process runs as.
    fn this_account() -> String {
        // SAFETY: `getpwuid` points into static storage that is read at once.
        unsafe {
            let entry = libc::getpwuid(libc::geteuid());
            assert!(!entry.is_null(), "the test account has a passwd entry");
            std::ffi::CStr::from_ptr((*entry).pw_name)
                .to_string_lossy()
                .into_owned()
        }
    }

    /// A supervisor config whose one service, running as this account, declares `host_sockets`.
    fn a_config_declaring_host_sockets(
        directory: &Path,
        host_sockets: Vec<crate::config::HostSocket>,
    ) -> SupervisorConfig {
        let service = ManagedService {
            user: this_account(),
            socket: Some(ServiceSocket {
                path: directory.join("daemon.sock"),
                group: None,
                mode: "0660".to_string(),
            }),
            host_sockets,
            ..a_managed_service().build()
        };
        SupervisorConfig {
            socket: SocketConfig {
                path: directory.join("supervisor.sock"),
                group: None,
                mode: "0660".to_string(),
            },
            services: vec![service],
            spawn_policy: SpawnPolicy::default(),
            cgroup: crate::config::CgroupPolicy::default(),
            shutdown_grace_secs: 1,
        }
    }

    #[test]
    fn creates_every_declared_host_socket_when_it_is_built() {
        // Given a service declaring a host socket for the account running the test
        let directory = tempfile::TempDir::new().expect("create a workspace");
        let path = directory.path().join("run/user/host.sock");
        let config = a_config_declaring_host_sockets(
            directory.path(),
            vec![crate::config::HostSocket {
                user: this_account(),
                path: path.clone(),
            }],
        );
        let forks = Arc::new(
            crate::spawn_broker::fork_broker::ForkBroker::start().expect("start the fork thread"),
        );

        // When the supervisor is built
        let _supervisor = Supervisor::new(&config, forks).expect("build the supervisor");

        // Then the socket exists, before anything has been started
        assert!(path.exists(), "{} was not created", path.display());
    }

    #[test]
    fn refuses_to_be_built_when_a_host_socket_cannot_be_created() {
        // Given a host socket for an account the host does not have
        let directory = tempfile::TempDir::new().expect("create a workspace");
        let config = a_config_declaring_host_sockets(
            directory.path(),
            vec![crate::config::HostSocket {
                user: "no-such-account-tddy".to_string(),
                path: directory.path().join("run/ghost/host.sock"),
            }],
        );
        let forks = Arc::new(
            crate::spawn_broker::fork_broker::ForkBroker::start().expect("start the fork thread"),
        );

        // When / Then — a service started without a socket it declared would serve nobody
        let error = Supervisor::new(&config, forks).err().expect("refused");
        assert!(
            format!("{error:#}").contains("no-such-account-tddy"),
            "{error:#}"
        );
    }

    #[test]
    fn hands_the_service_its_own_socket_first_and_every_host_socket_after_it() {
        // Given a built supervisor with a host socket declared for this account
        let directory = tempfile::TempDir::new().expect("create a workspace");
        let account = this_account();
        let config = a_config_declaring_host_sockets(
            directory.path(),
            vec![crate::config::HostSocket {
                user: account.clone(),
                path: directory.path().join("run/user/host.sock"),
            }],
        );
        let forks = Arc::new(
            crate::spawn_broker::fork_broker::ForkBroker::start().expect("start the fork thread"),
        );
        let supervisor = Supervisor::new(&config, forks).expect("build the supervisor");

        // When the handover for the service's next start is taken
        let handover = supervisor.lock()[0].socket_handover().expect("a handover");

        // Then
        assert_eq!(
            handover.fd_names(),
            format!("connection:host-session.{account}")
        );
    }
}
