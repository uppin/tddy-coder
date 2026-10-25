use crate::reaper;
use crate::services::{ExitOutcome, STARTUP_GRACE_PERIOD};
use crate::supervisor::Supervisor;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

impl Supervisor {
    /// Fork, drop privilege and exec one service, then start its startup-grace timer.
    pub(super) async fn start(self: &Arc<Self>, index: usize) {
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        let (name, plan, handover) = {
            let slots = self.lock();
            let slot = &slots[index];
            (
                slot.service.name.clone(),
                slot.spawn_plan(&self.supervisor_socket),
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
    pub(super) fn record_exit(&self, pid: u32) -> Option<(usize, String, ExitOutcome)> {
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

    pub(super) fn react_to_exit(
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
}
