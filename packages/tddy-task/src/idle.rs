//! Idle-timeout tracking: record activity and report when a resource has been idle long
//! enough to tear down. Shared by the relay daemon and the reusable-LSP registry.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Tracks the last recorded activity instant and compares it against a timeout.
pub struct IdleTimeoutTracker {
    last_activity: Mutex<Instant>,
    timeout: Duration,
}

impl IdleTimeoutTracker {
    /// Create a new tracker with the given idle timeout. `last_activity` is initialized to
    /// `Instant::now()`.
    pub fn new(timeout: Duration) -> Self {
        Self {
            last_activity: Mutex::new(Instant::now()),
            timeout,
        }
    }

    /// A tracker whose last activity was at `last_activity` rather than now — one that is already
    /// idle when `last_activity` is more than `timeout` ago. Lets a caller state how long a resource
    /// has been idle instead of waiting for it to become so, which is what keeps a check of "this
    /// activity reset the clock" free of a race against `timeout`.
    pub fn last_active_at(timeout: Duration, last_activity: Instant) -> Self {
        Self {
            last_activity: Mutex::new(last_activity),
            timeout,
        }
    }

    /// Record that activity just occurred (resets the idle clock).
    pub fn record_activity(&self) {
        if let Ok(mut guard) = self.last_activity.lock() {
            *guard = Instant::now();
        }
    }

    /// Returns `true` if the elapsed time since the last activity is >= the configured timeout.
    pub fn should_shutdown(&self) -> bool {
        if let Ok(guard) = self.last_activity.lock() {
            guard.elapsed() >= self.timeout
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_not_idle_when_activity_is_recent() {
        // Given a tracker with a long timeout
        let tracker = IdleTimeoutTracker::new(Duration::from_secs(300));

        // When checked immediately
        // Then it is not yet idle
        assert!(!tracker.should_shutdown());
    }

    #[test]
    fn reports_idle_after_the_timeout_elapses() {
        // Given a tracker with a tiny timeout
        let tracker = IdleTimeoutTracker::new(Duration::from_millis(1));

        // When the timeout elapses
        std::thread::sleep(Duration::from_millis(5));

        // Then it reports idle
        assert!(tracker.should_shutdown());
    }

    #[test]
    fn a_tracker_last_active_longer_ago_than_its_timeout_is_already_idle() {
        // Given a tracker last active two minutes ago, with a one-minute timeout
        let tracker =
            IdleTimeoutTracker::last_active_at(Duration::from_secs(60), two_minutes_ago());

        // Then it is idle without anyone waiting for it
        assert!(tracker.should_shutdown());
    }

    #[test]
    fn recording_activity_resets_the_idle_clock() {
        // Given a tracker that has gone idle. Its timeout is a minute, so the check below cannot
        // race it: a timeout of milliseconds turns any scheduling delay between the reset and the
        // check into a reported failure to reset.
        let tracker =
            IdleTimeoutTracker::last_active_at(Duration::from_secs(60), two_minutes_ago());
        assert!(tracker.should_shutdown());

        // When activity is recorded
        tracker.record_activity();

        // Then it is no longer idle
        assert!(!tracker.should_shutdown());
    }

    fn two_minutes_ago() -> Instant {
        Instant::now()
            .checked_sub(Duration::from_secs(120))
            .expect("the clock reaches two minutes back")
    }
}
