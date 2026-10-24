use crate::cgroup_broker::{self, ScopeRemoval};
use crate::error::SupervisorError;
use crate::request::{SessionState, SessionStatus};
use crate::signals::{signal_process_group, KILL, TERMINATE};
use crate::spawn_broker::fork_broker::ChildStdio;
use crate::spawn_broker::fork_broker::SpawnedChild;
use crate::spawn_broker::fork_broker::StdioMode;
use crate::spawn_broker::SpawnPlan;
use crate::supervisor::scope_directory_of;
use crate::supervisor::Supervisor;
use crate::supervisor::SCOPE_REMOVAL_GRACE;
use crate::supervisor::SCOPE_REMOVAL_RETRY_INTERVAL;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
pub(super) struct SessionGeneration(u64);

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
pub(super) struct SessionTable {
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
    pub(super) fn record_spawned(&mut self, pid: u32) -> SessionGeneration {
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
    pub(super) fn own_scope(&mut self, generation: SessionGeneration, scope: PathBuf) {
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
    pub(super) fn record_exit(&mut self, pid: u32, code: Option<i32>) -> Option<PathBuf> {
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
    pub(super) fn snapshot(&self, pid: u32) -> Option<(SessionGeneration, SessionStatus)> {
        let generation = self.records.get(&pid)?.generation;
        self.status(pid).map(|status| (generation, status))
    }

    pub(super) fn status(&self, pid: u32) -> Option<SessionStatus> {
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
    pub(super) fn is_running_generation(&self, pid: u32, generation: SessionGeneration) -> bool {
        self.records.get(&pid)
            == Some(&SessionRecord {
                generation,
                outcome: SessionOutcome::Running,
            })
    }

    pub(super) fn running_pids(&self) -> Vec<u32> {
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
    pub(super) fn require_running(&self, pid: u32) -> Result<(), SupervisorError> {
        match self.records.get(&pid).map(|record| record.outcome) {
            Some(SessionOutcome::Running) => Ok(()),
            Some(SessionOutcome::Exited { .. }) | None => Err(SupervisorError::Denied),
        }
    }
}

/// How many exited sessions keep their status available for a caller that has not asked yet.
///
/// Some bound is mandatory: the supervisor is a process that must not need restarting, so a map of
/// dead pids that only ever grows is a slow leak with no upper edge. The bound is a capacity rather
/// than "forget it once somebody has read it" because a caller polls repeatedly — the daemon's
/// exit-diagnostic loop asks every 50ms and asks again after it has its answer — so a status that
/// vanished on first read would answer one poll and deny the next.
const RETAINED_EXITED_SESSIONS: usize = 256;

impl Supervisor {
    /// Fork a session on a caller's behalf and take ownership of it.
    ///
    /// The supervisor, not the privileged surface, owns every fork and every pid it produces: that
    /// is what lets one gate cover both the services it starts itself and the sessions it is asked
    /// for, and what makes `shutdown` able to account for all of them.
    pub async fn spawn_session(&self, plan: SpawnPlan) -> std::io::Result<u32> {
        Ok(self.fork_session(plan, StdioMode::Plan).await?.pid)
    }

    /// [`Self::spawn_session`] for a plan whose output is
    /// [`ChildOutput::CaptureStderr`](crate::spawn_broker::ChildOutput::CaptureStderr): the pid, and
    /// the read end of the session's stderr.
    pub async fn spawn_session_capturing_stderr(
        &self,
        plan: SpawnPlan,
    ) -> std::io::Result<(u32, std::process::ChildStderr)> {
        let spawned = self.fork_session(plan, StdioMode::Plan).await?;
        let stderr = spawned.stderr.ok_or_else(|| {
            std::io::Error::other("the session's plan did not ask for its stderr to be captured")
        })?;
        Ok((spawned.pid, stderr))
    }

    /// Wait until a session this supervisor spawned has exited, and report how.
    ///
    /// Woken by the reap that records the exit rather than polling for it. A pid the supervisor
    /// never spawned is [`SupervisorError::Denied`], exactly as for [`Self::session_status`].
    pub async fn wait_for_session_exit(&self, pid: u32) -> Result<SessionStatus, SupervisorError> {
        loop {
            // Registered before the status is read, so an exit recorded in between wakes this
            // waiter instead of being missed.
            let exited = self.session_exited.notified();
            tokio::pin!(exited);
            exited.as_mut().enable();
            let status = self.session_status(pid)?;
            if status.state == SessionState::Exited {
                return Ok(status);
            }
            exited.await;
        }
    }

    /// [`Self::spawn_session`] with the child's stdio piped to the supervisor, for a relayed spawn.
    pub async fn spawn_session_piped(&self, plan: SpawnPlan) -> std::io::Result<(u32, ChildStdio)> {
        let spawned = self.fork_session(plan, StdioMode::Piped).await?;
        match spawned.stdio {
            Some(stdio) => Ok((spawned.pid, stdio)),
            None => Err(std::io::Error::other(
                "a piped spawn came back without its pipes",
            )),
        }
    }

    /// The fork both session methods share: one at a time behind the spawn gate, and the session
    /// recorded — with the scope it was placed in — the instant it exists.
    async fn fork_session(
        &self,
        plan: SpawnPlan,
        stdio: StdioMode,
    ) -> std::io::Result<SpawnedChild> {
        let _in_flight = self.spawn_gate.lock().await;
        // Read before the plan is handed over. A scope becomes this session's the moment the session
        // exists, so it can be reclaimed when the session ends without a caller having to ask.
        let scope = scope_directory_of(plan.scope_procs.as_deref()).map(Path::to_path_buf);
        // No listener: socket handover is a declared managed service's privilege, never something a
        // caller can ask for.
        let spawned = self.forks.submit(plan, None, stdio).await?;
        let mut sessions = self.sessions();
        let generation = sessions.record_spawned(spawned.pid);
        if let Some(scope) = scope {
            sessions.own_scope(generation, scope);
        }
        Ok(spawned)
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
    pub(super) fn reclaim_scope(&self, scope: PathBuf) {
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
}
