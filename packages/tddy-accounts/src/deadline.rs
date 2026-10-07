//! The one place a link attempt's lifetime is dated and forgotten.
//!
//! Both halves of a link hold attempts that must not outlive the window the provider gave its code:
//! this crate's service, and the daemon's provider-facing linker. They share this so the two cannot
//! drift on what "expired" means.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::linking::LinkError;

/// When a window of `seconds` that opened at `now` closes.
///
/// Fails only for a window longer than the clock can represent, which no provider issues; an
/// attempt that cannot be dated is refused rather than kept forever.
///
/// # Errors
/// [`LinkError::Unavailable`] when the instant is not representable.
pub fn deadline_after(now: Instant, seconds: u64) -> Result<Instant, LinkError> {
    now.checked_add(Duration::from_secs(seconds))
        .ok_or_else(|| LinkError::Unavailable("the link window is not representable".to_string()))
}

/// Whether a window that closes at `deadline` has closed by `now`. The deadline itself is over.
#[must_use]
pub fn is_past(deadline: Instant, now: Instant) -> bool {
    now >= deadline
}

/// Drop every attempt whose window has closed by `now`.
pub fn forget_expired<V>(
    attempts: &mut HashMap<String, V>,
    now: Instant,
    deadline_of: impl Fn(&V) -> Instant,
) {
    attempts.retain(|_, attempt| !is_past(deadline_of(attempt), now));
}
