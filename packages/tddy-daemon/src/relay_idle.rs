//! Idle-timeout tracker for relay daemon mode.
//!
//! Tracks the last activity time and reports when the daemon should shut down
//! due to inactivity.

/// Tracks the last recorded activity instant and compares it against a timeout.
pub struct IdleTimeoutTracker {
    last_activity: std::sync::Mutex<std::time::Instant>,
    timeout: std::time::Duration,
}

impl IdleTimeoutTracker {
    /// Create a new tracker with the given idle timeout.
    ///
    /// `last_activity` is initialized to `Instant::now()`.
    pub fn new(timeout: std::time::Duration) -> Self {
        Self {
            last_activity: std::sync::Mutex::new(std::time::Instant::now()),
            timeout,
        }
    }

    /// Record that activity just occurred (resets the idle clock).
    pub fn record_activity(&self) {
        if let Ok(mut guard) = self.last_activity.lock() {
            *guard = std::time::Instant::now();
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
