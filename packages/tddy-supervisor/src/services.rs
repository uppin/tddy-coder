//! Per-service state, kept separate from the fork/exec/waitpid machinery so the transitions are
//! testable without processes.

use std::time::Duration;

use crate::config::ManagedService;
use crate::restart::{BackoffState, RestartDecision};
use crate::service::{ServiceState, ServiceStatus};

/// How long a freshly exec'd service has to survive before it counts as running.
///
/// Mirrors the 500ms grace the daemon's own spawner uses to catch a child that dies instantly (a
/// missing shared library, a bad config) rather than reporting it as healthy.
pub const STARTUP_GRACE_PERIOD: Duration = Duration::from_millis(500);

/// What the supervisor should do after one of its services exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitOutcome {
    /// Start it again after this delay.
    Restart { after: Duration },
    /// The retry budget is spent.
    GaveUp,
    /// It exited because it was asked to; leave it alone.
    StoppedOnRequest,
}

/// Lifecycle state of one declared service.
#[derive(Debug, Clone)]
pub struct ServiceRuntime {
    name: String,
    state: ServiceState,
    pid: Option<u32>,
    backoff: BackoffState,
    stop_requested: bool,
}

impl ServiceRuntime {
    /// A declared but not yet started service.
    pub fn new(service: &ManagedService) -> ServiceRuntime {
        ServiceRuntime {
            name: service.name.clone(),
            state: ServiceState::Starting,
            pid: None,
            backoff: BackoffState::new(service.restart.clone()),
            stop_requested: false,
        }
    }

    /// Snapshot for `ListServices`.
    pub fn status(&self) -> ServiceStatus {
        ServiceStatus {
            name: self.name.clone(),
            pid: self.pid,
            state: self.state,
            restarts: self.backoff.restarts(),
        }
    }

    /// The service has been forked and exec'd.
    pub fn record_started(&mut self, pid: u32) {
        self.state = ServiceState::Starting;
        self.pid = Some(pid);
    }

    /// The service is still alive after [`STARTUP_GRACE_PERIOD`].
    pub fn record_survived_startup(&mut self) {
        // Only a service still in `Starting` is promoted: the grace period elapsing after the
        // reaper already saw the process die must not resurrect it into `Running`.
        if self.state == ServiceState::Starting {
            self.state = ServiceState::Running;
        }
    }

    /// The service exited after running for `uptime`.
    pub fn record_exit(&mut self, uptime: Duration) -> ExitOutcome {
        // The pid goes first, whatever happens next: handing out a dead pid invites a caller to
        // signal one the kernel may already have recycled.
        self.pid = None;

        if self.stop_requested {
            self.stop_requested = false;
            self.state = ServiceState::Stopped;
            return ExitOutcome::StoppedOnRequest;
        }

        match self.backoff.record_exit(uptime) {
            RestartDecision::Restart(after) => {
                self.state = ServiceState::Backoff;
                ExitOutcome::Restart { after }
            }
            RestartDecision::GiveUp => {
                self.state = ServiceState::GaveUp;
                ExitOutcome::GaveUp
            }
        }
    }

    /// An operator asked for the service to stop; its restart policy is suppressed.
    pub fn record_stop_requested(&mut self) {
        self.stop_requested = true;
    }

    /// An operator asked for a stopped or given-up service to run again.
    pub fn record_start_requested(&mut self) {
        // A start supersedes a stop that has not been observed yet, and the budget comes back with
        // it — otherwise a `StartService` on a given-up service would give up again on its first
        // exit.
        self.stop_requested = false;
        self.backoff.restore_budget();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{a_managed_service, a_restart_policy};

    fn a_service_runtime() -> ServiceRuntime {
        ServiceRuntime::new(&a_managed_service().named("tddy-daemon").build())
    }

    fn crashed_immediately() -> Duration {
        Duration::from_millis(0)
    }

    #[test]
    fn reports_a_declared_but_unstarted_service_as_starting_without_a_pid() {
        // Given
        let runtime = a_service_runtime();

        // When
        let status = runtime.status();

        // Then
        assert_eq!(status.name, "tddy-daemon");
        assert_eq!(status.state, ServiceState::Starting);
        assert_eq!(status.pid, None);
        assert_eq!(status.restarts, 0);
    }

    #[test]
    fn reports_the_pid_of_a_service_it_has_just_started() {
        // Given
        let mut runtime = a_service_runtime();

        // When
        runtime.record_started(4242);

        // Then — still Starting: the process exists but has not proved it can survive exec.
        assert_eq!(runtime.status().state, ServiceState::Starting);
        assert_eq!(runtime.status().pid, Some(4242));
    }

    #[test]
    fn reports_a_service_as_running_once_it_survives_the_startup_grace_period() {
        // Given
        let mut runtime = a_service_runtime();
        runtime.record_started(4242);

        // When
        runtime.record_survived_startup();

        // Then
        assert_eq!(runtime.status().state, ServiceState::Running);
        assert_eq!(runtime.status().pid, Some(4242));
    }

    #[test]
    fn forgets_the_pid_and_reports_backoff_while_waiting_to_restart() {
        // Given
        let mut runtime = a_service_runtime();
        runtime.record_started(4242);
        runtime.record_survived_startup();

        // When
        let outcome = runtime.record_exit(crashed_immediately());

        // Then the dead pid is dropped immediately — reporting it would invite a caller to signal
        // a pid the kernel may already have recycled.
        assert_eq!(
            outcome,
            ExitOutcome::Restart {
                after: Duration::from_millis(100)
            }
        );
        assert_eq!(runtime.status().state, ServiceState::Backoff);
        assert_eq!(runtime.status().pid, None);
        assert_eq!(runtime.status().restarts, 1);
    }

    #[test]
    fn reports_giving_up_with_the_number_of_restarts_it_performed() {
        // Given a service allowed two retries, whose every start fails at once.
        let mut runtime = ServiceRuntime::new(
            &a_managed_service()
                .with_restart_policy(a_restart_policy().with_max_retries(2).build())
                .build(),
        );
        runtime.record_exit(crashed_immediately());
        runtime.record_exit(crashed_immediately());

        // When
        let outcome = runtime.record_exit(crashed_immediately());

        // Then
        assert_eq!(outcome, ExitOutcome::GaveUp);
        assert_eq!(runtime.status().state, ServiceState::GaveUp);
        assert_eq!(runtime.status().pid, None);
        assert_eq!(runtime.status().restarts, 2);
    }

    #[test]
    fn reports_a_service_stopped_on_request_rather_than_restarting_it() {
        // Given
        let mut runtime = a_service_runtime();
        runtime.record_started(4242);
        runtime.record_survived_startup();

        // When
        runtime.record_stop_requested();
        let outcome = runtime.record_exit(crashed_immediately());

        // Then
        assert_eq!(outcome, ExitOutcome::StoppedOnRequest);
        assert_eq!(runtime.status().state, ServiceState::Stopped);
        assert_eq!(runtime.status().pid, None);
    }

    #[test]
    fn does_not_spend_the_retry_budget_on_a_stop_that_was_asked_for() {
        // Given
        let mut runtime = a_service_runtime();
        runtime.record_started(4242);
        runtime.record_survived_startup();

        // When
        runtime.record_stop_requested();
        runtime.record_exit(crashed_immediately());

        // Then an operator stopping a service must not push it closer to being given up on.
        assert_eq!(runtime.status().restarts, 0);
    }

    #[test]
    fn restores_a_given_up_service_to_starting_when_it_is_started_again() {
        // Given a service that has exhausted its budget.
        let mut runtime = ServiceRuntime::new(
            &a_managed_service()
                .with_restart_policy(a_restart_policy().with_max_retries(1).build())
                .build(),
        );
        runtime.record_exit(crashed_immediately());
        runtime.record_exit(crashed_immediately());

        // When
        runtime.record_start_requested();
        runtime.record_started(5150);

        // Then the retry budget is restored too, or an operator's StartService would fail again on
        // the very next exit.
        assert_eq!(runtime.status().state, ServiceState::Starting);
        assert_eq!(runtime.status().pid, Some(5150));
        assert_eq!(runtime.status().restarts, 0);
    }

    #[test]
    fn resumes_restarting_a_stopped_service_after_it_is_started_again() {
        // Given a service that was deliberately stopped.
        let mut runtime = a_service_runtime();
        runtime.record_started(4242);
        runtime.record_stop_requested();
        runtime.record_exit(crashed_immediately());

        // When it is started again and then crashes on its own
        runtime.record_start_requested();
        runtime.record_started(5150);
        let outcome = runtime.record_exit(crashed_immediately());

        // Then the suppression applied to that one stop, not forever.
        assert_eq!(
            outcome,
            ExitOutcome::Restart {
                after: Duration::from_millis(100)
            }
        );
    }
}
