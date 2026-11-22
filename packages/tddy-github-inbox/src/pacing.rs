//! When a repository's feed may next ask GitHub: its poll interval, its rate limit, its failures.

use tddy_github::conditional::{ConditionalAnswer, GithubReadError, RateLimit};

use crate::LOW_QUOTA_REMAINING;

/// The wait between feed polls when GitHub names none.
pub(crate) const DEFAULT_POLL_INTERVAL_MS: u64 = 60_000;
/// The wait between pending-check polls.
pub(crate) const PENDING_CHECK_POLL_MS: u64 = 30_000;
/// How often the updated-pulls sweep runs.
pub(crate) const SWEEP_EVERY_MS: u64 = 15 * 60_000;
/// The sweep is optional catch-up: it runs only while the quota is comfortably above the floor.
const SWEEP_MIN_REMAINING: u32 = 4 * LOW_QUOTA_REMAINING;

/// One repository's schedule.
#[derive(Debug, Default)]
pub(crate) struct Pacing {
    feed_due_at: u64,
    pending_checks_due_at: u64,
    sweep_due_at: Option<u64>,
    rate_limit: Option<RateLimit>,
}

impl Pacing {
    pub(crate) fn observe_rate_limit(&mut self, rate_limit: Option<RateLimit>) {
        if rate_limit.is_some() {
            self.rate_limit = rate_limit;
        }
    }

    /// When the quota is below the floor, the unix-ms at which its window resets.
    pub(crate) fn exhausted_until_ms(&self) -> Option<u64> {
        self.rate_limit
            .filter(|limit| limit.remaining < LOW_QUOTA_REMAINING)
            .map(|limit| limit.reset_at_unix_s * 1000)
    }

    /// Whether the quota forbids a request at `now_ms`.
    pub(crate) fn paused_at(&self, now_ms: u64) -> bool {
        self.exhausted_until_ms()
            .is_some_and(|reset| now_ms < reset)
    }

    pub(crate) fn feed_due(&self, now_ms: u64) -> bool {
        now_ms >= self.feed_due_at && !self.paused_at(now_ms)
    }

    pub(crate) fn pending_checks_due(&self, now_ms: u64) -> bool {
        now_ms >= self.pending_checks_due_at && !self.paused_at(now_ms)
    }

    pub(crate) fn pending_checks_polled(&mut self, now_ms: u64) {
        self.pending_checks_due_at = now_ms + PENDING_CHECK_POLL_MS;
    }

    /// The first step of a repository starts the sweep's clock; the sweep is then due every
    /// [`SWEEP_EVERY_MS`], while the quota is comfortable.
    pub(crate) fn sweep_due(&mut self, now_ms: u64) -> bool {
        let due_at = *self.sweep_due_at.get_or_insert(now_ms + SWEEP_EVERY_MS);
        now_ms >= due_at
            && !self.paused_at(now_ms)
            && self
                .rate_limit
                .is_none_or(|limit| limit.remaining >= SWEEP_MIN_REMAINING)
    }

    pub(crate) fn swept(&mut self, now_ms: u64) {
        self.sweep_due_at = Some(now_ms + SWEEP_EVERY_MS);
    }

    /// Fold a feed answer in: wait out its poll interval, and the rate-limit window if the quota is low.
    pub(crate) fn feed_answered(&mut self, answer: &ConditionalAnswer, now_ms: u64) {
        self.observe_rate_limit(answer.rate_limit());
        let wait = answer
            .poll_interval_ms()
            .unwrap_or(DEFAULT_POLL_INTERVAL_MS);
        self.feed_due_at = (now_ms + wait).max(self.exhausted_until_ms().unwrap_or(0));
    }

    /// Fold a failed feed read in: wait out `Retry-After`, at least the default interval.
    pub(crate) fn feed_failed(&mut self, error: &GithubReadError, now_ms: u64) {
        self.observe_rate_limit(error.rate_limit);
        let wait = error
            .retry_after_s
            .map_or(DEFAULT_POLL_INTERVAL_MS, |s| s * 1000)
            .max(DEFAULT_POLL_INTERVAL_MS);
        self.feed_due_at = (now_ms + wait).max(self.exhausted_until_ms().unwrap_or(0));
    }

    /// The earliest moment a step has something to do, given whether checks are pending.
    pub(crate) fn next_due(&self, checks_pending: bool) -> u64 {
        let mut due = self.feed_due_at;
        if checks_pending {
            due = due.min(self.pending_checks_due_at);
        }
        if let Some(sweep) = self.sweep_due_at {
            due = due.min(sweep);
        }
        due.max(self.exhausted_until_ms().unwrap_or(0))
    }
}
