//! Quitting Tddy Desktop runs the daemon's shutdown on the main (UI) thread, so it must be bounded:
//! a wedged blocking pool held the 2026-11-26 quit for 7 minutes ("Application not responding").
//!
//! TODO(red): stub — implemented in the green commit.

use std::future::Future;
use std::time::Duration;

/// How a bounded shutdown ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ShutdownOutcome {
    Finished,
    GaveUpAfter(Duration),
}

/// Drive `shut_down` for at most `deadline`.
pub(crate) async fn shut_down_within(
    _deadline: Duration,
    shut_down: impl Future<Output = ()>,
) -> ShutdownOutcome {
    shut_down.await;
    ShutdownOutcome::Finished
}

#[cfg(test)]
mod tests {
    use super::{shut_down_within, ShutdownOutcome};
    use std::time::{Duration, Instant};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_shutdown_that_never_finishes_is_given_up_at_its_deadline() {
        // Given a shutdown waiting on work that never completes
        let wedged = std::future::pending::<()>();

        // When
        let started = Instant::now();
        let outcome = shut_down_within(Duration::from_millis(200), wedged).await;

        // Then
        assert_eq!(
            outcome,
            ShutdownOutcome::GaveUpAfter(Duration::from_millis(200))
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_shutdown_that_finishes_in_time_reports_finished() {
        // Given
        let quick = async {};

        // When
        let outcome = shut_down_within(Duration::from_secs(5), quick).await;

        // Then
        assert_eq!(outcome, ShutdownOutcome::Finished);
    }
}
