//! Restart backoff — pure, so the timing policy is testable without waiting for it.

use std::time::Duration;

use crate::config::RestartPolicy;

/// What to do after a managed service has exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartDecision {
    /// Restart the service after this delay.
    Restart(Duration),
    /// The retry budget is spent; do not restart again.
    GiveUp,
}

/// Per-service backoff bookkeeping.
#[derive(Debug, Clone)]
pub struct BackoffState {
    policy: RestartPolicy,
    restarts: u32,
}

impl BackoffState {
    pub fn new(policy: RestartPolicy) -> BackoffState {
        BackoffState {
            policy,
            restarts: 0,
        }
    }

    /// Restarts actually performed since the last stable run.
    ///
    /// This never counts the exit that exhausted the budget, so a service configured for two
    /// retries reports two restarts when it gives up — not three.
    pub fn restarts(&self) -> u32 {
        self.restarts
    }

    /// Record that the service exited after running for `uptime`, and decide what happens next.
    ///
    /// An exit after a stable run resets the retry budget: a service that has been healthy for
    /// hours should get its full allowance again, not inherit a count from last week's crash.
    pub fn record_exit(&mut self, uptime: Duration) -> RestartDecision {
        if uptime.as_millis() >= u128::from(self.policy.stability_threshold_ms) {
            self.restore_budget();
        }
        if self.restarts >= self.policy.max_retries {
            return RestartDecision::GiveUp;
        }

        self.restarts += 1;
        // The first restart waits the initial backoff, so the exponent is one less than the count.
        let doubling = 2u64
            .checked_pow(self.restarts - 1)
            .unwrap_or(u64::MAX)
            .saturating_mul(self.policy.initial_backoff_ms);
        RestartDecision::Restart(Duration::from_millis(
            doubling.min(self.policy.max_backoff_ms),
        ))
    }

    /// Give the service its full retry allowance back, as a stable run would.
    pub fn restore_budget(&mut self) {
        self.restarts = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::a_restart_policy;

    fn crashed_immediately() -> Duration {
        Duration::from_millis(0)
    }

    #[test]
    fn waits_the_initial_backoff_before_the_first_restart() {
        // Given
        let mut backoff =
            BackoffState::new(a_restart_policy().with_initial_backoff_ms(100).build());

        // When
        let decision = backoff.record_exit(crashed_immediately());

        // Then
        assert_eq!(
            decision,
            RestartDecision::Restart(Duration::from_millis(100))
        );
    }

    #[test]
    fn doubles_the_backoff_on_each_consecutive_failure() {
        // Given
        let mut backoff = BackoffState::new(
            a_restart_policy()
                .with_initial_backoff_ms(100)
                .with_max_backoff_ms(10_000)
                .with_max_retries(4)
                .build(),
        );

        // When
        let delays: Vec<RestartDecision> = (0..4)
            .map(|_| backoff.record_exit(crashed_immediately()))
            .collect();

        // Then
        assert_eq!(
            delays,
            vec![
                RestartDecision::Restart(Duration::from_millis(100)),
                RestartDecision::Restart(Duration::from_millis(200)),
                RestartDecision::Restart(Duration::from_millis(400)),
                RestartDecision::Restart(Duration::from_millis(800)),
            ]
        );
    }

    #[test]
    fn caps_the_backoff_at_the_configured_maximum() {
        // Given
        let mut backoff = BackoffState::new(
            a_restart_policy()
                .with_initial_backoff_ms(100)
                .with_max_backoff_ms(250)
                .with_max_retries(4)
                .build(),
        );

        // When
        let delays: Vec<RestartDecision> = (0..4)
            .map(|_| backoff.record_exit(crashed_immediately()))
            .collect();

        // Then
        assert_eq!(
            delays,
            vec![
                RestartDecision::Restart(Duration::from_millis(100)),
                RestartDecision::Restart(Duration::from_millis(200)),
                RestartDecision::Restart(Duration::from_millis(250)),
                RestartDecision::Restart(Duration::from_millis(250)),
            ]
        );
    }

    #[test]
    fn gives_up_after_spending_the_configured_retry_budget() {
        // Given a budget of two retries.
        let mut backoff = BackoffState::new(a_restart_policy().with_max_retries(2).build());
        backoff.record_exit(crashed_immediately());
        backoff.record_exit(crashed_immediately());

        // When the third start also fails
        let decision = backoff.record_exit(crashed_immediately());

        // Then
        assert_eq!(decision, RestartDecision::GiveUp);
    }

    #[test]
    fn reports_the_restarts_it_performed_rather_than_the_exits_it_saw() {
        // Given a budget of two retries, spent by three failing starts.
        let mut backoff = BackoffState::new(a_restart_policy().with_max_retries(2).build());
        backoff.record_exit(crashed_immediately());
        backoff.record_exit(crashed_immediately());
        backoff.record_exit(crashed_immediately());

        // When / Then — three exits, but only two restarts were ever performed.
        assert_eq!(backoff.restarts(), 2);
    }

    #[test]
    fn never_restarts_a_service_whose_retry_budget_is_zero() {
        // Given
        let mut backoff = BackoffState::new(a_restart_policy().with_max_retries(0).build());

        // When
        let decision = backoff.record_exit(crashed_immediately());

        // Then
        assert_eq!(decision, RestartDecision::GiveUp);
    }

    #[test]
    fn resets_the_backoff_delay_after_a_run_past_the_stability_threshold() {
        // Given a service that has already failed twice with a doubling backoff.
        let mut backoff = BackoffState::new(
            a_restart_policy()
                .with_initial_backoff_ms(100)
                .with_max_retries(10)
                .with_stability_threshold_ms(5_000)
                .build(),
        );
        backoff.record_exit(crashed_immediately());
        backoff.record_exit(crashed_immediately());

        // When the next run lasts longer than the stability threshold
        let decision = backoff.record_exit(Duration::from_millis(5_000));

        // Then the delay starts over from the initial backoff rather than continuing to double.
        assert_eq!(
            decision,
            RestartDecision::Restart(Duration::from_millis(100))
        );
    }

    #[test]
    fn restores_the_full_retry_budget_after_a_run_past_the_stability_threshold() {
        // Given a service that has spent its entire two-retry budget.
        let mut backoff = BackoffState::new(
            a_restart_policy()
                .with_max_retries(2)
                .with_stability_threshold_ms(5_000)
                .build(),
        );
        backoff.record_exit(crashed_immediately());
        backoff.record_exit(crashed_immediately());

        // When it then stays up past the stability threshold and exits again
        backoff.record_exit(Duration::from_millis(5_000));

        // Then it is treated as a first failure, not as a service that already gave up: a daemon
        // that ran healthily for a week must not be abandoned on its first crash.
        assert_eq!(backoff.restarts(), 1);
    }

    #[test]
    fn treats_a_run_exactly_at_the_stability_threshold_as_stable() {
        // Given
        let mut backoff = BackoffState::new(
            a_restart_policy()
                .with_initial_backoff_ms(100)
                .with_max_retries(10)
                .with_stability_threshold_ms(5_000)
                .build(),
        );
        backoff.record_exit(crashed_immediately());

        // When
        let decision = backoff.record_exit(Duration::from_millis(5_000));

        // Then
        assert_eq!(
            decision,
            RestartDecision::Restart(Duration::from_millis(100))
        );
    }
}
