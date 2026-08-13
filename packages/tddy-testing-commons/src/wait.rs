//! Polling waits for conditions a test cannot observe synchronously.
//!
//! Every test in this workspace that watches for a spawned process to write a file, print a
//! marker, or open a socket needs the same shape: poll a cheap predicate until it holds, and fail
//! with something readable if it never does. Hand-rolled versions of that loop drifted apart —
//! different cadences, different ceilings, and failure messages that said only "timed out".
//!
//! Two rules these helpers encode:
//!
//! - **The ceiling is a safety net, not a prediction.** A test that passes in 40ms on an idle
//!   machine and 900ms under a parallel build is not flaky; a test whose ceiling was calibrated on
//!   the idle number is. Pick a ceiling generous enough that only a genuinely broken system
//!   reaches it — the wait costs nothing when the condition holds early.
//! - **A timeout must say what it last saw.** The probe returns `Result<T, String>`, not
//!   `Option<T>`, so the failure carries the observed state ("argv file holds 2 of 3 lines") and
//!   not just the absence of success.

use std::future::Future;
use std::time::Duration;

/// How often every helper here re-probes. Deliberately not caller-tunable: a test that needs a
/// different cadence is a test whose probe is too expensive to poll, and it should be rewritten
/// around a real readiness signal instead.
pub const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Polls `probe` on the tokio clock until it returns `Ok`, then yields that value.
///
/// Panics with the last error `probe` reported if `within` elapses first.
pub async fn eventually<T>(
    condition: &str,
    within: Duration,
    mut probe: impl FnMut() -> Result<T, String>,
) -> T {
    eventually_awaiting(condition, within, || std::future::ready(probe())).await
}

/// [`eventually`] for a probe that is itself asynchronous — an RPC round trip, a socket connect.
pub async fn eventually_awaiting<T, F>(
    condition: &str,
    within: Duration,
    mut probe: impl FnMut() -> F,
) -> T
where
    F: Future<Output = Result<T, String>>,
{
    let deadline = tokio::time::Instant::now() + within;
    let mut polls = 0_u32;
    loop {
        polls += 1;
        match probe().await {
            Ok(value) => return value,
            Err(observed) if tokio::time::Instant::now() >= deadline => {
                panic!("{}", timed_out(condition, within, polls, &observed));
            }
            Err(_) => {}
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// [`eventually`] for tests with no async runtime — `#[test]` bodies polling the filesystem.
#[track_caller]
pub fn eventually_blocking<T>(
    condition: &str,
    within: Duration,
    mut probe: impl FnMut() -> Result<T, String>,
) -> T {
    let deadline = std::time::Instant::now() + within;
    let mut polls = 0_u32;
    loop {
        polls += 1;
        match probe() {
            Ok(value) => return value,
            Err(observed) if std::time::Instant::now() >= deadline => {
                panic!("{}", timed_out(condition, within, polls, &observed));
            }
            Err(_) => {}
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn timed_out(condition: &str, within: Duration, polls: u32, last: &str) -> String {
    format!(
        "timed out waiting for {condition}\n  gave up after {within:?} ({polls} polls, one every {POLL_INTERVAL:?})\n  last observed: {last}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const A_GENEROUS_CEILING: Duration = Duration::from_secs(5);
    const A_CEILING_NOTHING_CAN_MEET: Duration = Duration::from_millis(60);

    /// A probe that fails the first `failures` times and then succeeds with `value`.
    fn a_probe_succeeding_after(
        failures: u32,
        value: &'static str,
    ) -> impl FnMut() -> Result<&'static str, String> {
        let attempts = Cell::new(0_u32);
        move || {
            let seen = attempts.get();
            attempts.set(seen + 1);
            if seen < failures {
                Err(format!("attempt {seen} saw nothing yet"))
            } else {
                Ok(value)
            }
        }
    }

    fn a_probe_that_never_succeeds() -> impl FnMut() -> Result<&'static str, String> {
        || Err("the file is still empty".to_string())
    }

    #[test]
    fn eventually_blocking_returns_the_value_once_the_probe_succeeds() {
        // Given a condition that only holds on the third poll
        let probe = a_probe_succeeding_after(2, "the argv line");

        // When
        let observed =
            eventually_blocking("the argv file to be written", A_GENEROUS_CEILING, probe);

        // Then
        assert_eq!(observed, "the argv line");
    }

    #[test]
    fn eventually_blocking_reports_the_last_observed_state_when_it_gives_up() {
        // Given a condition that never holds
        let probe = a_probe_that_never_succeeds();

        // When
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            eventually_blocking(
                "the argv file to be written",
                A_CEILING_NOTHING_CAN_MEET,
                probe,
            )
        }))
        .expect_err("a condition that never holds must panic");

        // Then the diagnosis names the condition and what the probe last saw
        let message = failure
            .downcast_ref::<String>()
            .expect("panic payload is a String")
            .clone();
        assert!(
            message.contains("the argv file to be written"),
            "message must name the condition: {message}"
        );
        assert!(
            message.contains("the file is still empty"),
            "message must carry the last observed state: {message}"
        );
    }

    #[test]
    fn eventually_blocking_probes_at_least_once_before_giving_up() {
        // Given a ceiling of zero — the caller asked for no waiting at all
        let probe = a_probe_succeeding_after(0, "already true");

        // When
        let observed = eventually_blocking("an already-true condition", Duration::ZERO, probe);

        // Then the condition is still evaluated, not skipped
        assert_eq!(observed, "already true");
    }

    #[tokio::test]
    async fn eventually_returns_the_value_once_the_probe_succeeds() {
        // Given
        let probe = a_probe_succeeding_after(2, "the marker");

        // When
        let observed = eventually("the marker to appear", A_GENEROUS_CEILING, probe).await;

        // Then
        assert_eq!(observed, "the marker");
    }

    #[tokio::test]
    #[should_panic(expected = "the socket to accept a connection")]
    async fn eventually_panics_naming_the_condition_when_it_never_holds() {
        // Given / When / Then
        eventually(
            "the socket to accept a connection",
            A_CEILING_NOTHING_CAN_MEET,
            a_probe_that_never_succeeds(),
        )
        .await;
    }

    #[tokio::test]
    async fn eventually_awaiting_polls_an_async_probe_until_it_succeeds() {
        // Given an async probe that only succeeds on its third call
        let attempts = Cell::new(0_u32);
        let probe = || async {
            let seen = attempts.get();
            attempts.set(seen + 1);
            if seen < 2 {
                Err(format!("connect refused on attempt {seen}"))
            } else {
                Ok(seen)
            }
        };

        // When
        let observed = eventually_awaiting("the server to accept", A_GENEROUS_CEILING, probe).await;

        // Then
        assert_eq!(observed, 2);
    }
}
