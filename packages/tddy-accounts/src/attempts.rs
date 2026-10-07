//! Link attempts the service has begun and not finished, and when each may next be polled.
//!
//! A poll presents only the `link_id`, and a link belongs to the session that began it. Attempts are
//! bounded in time (see [`crate::deadline`]) so an attempt nobody polls to a terminal answer does not
//! stay for the daemon's life, and each is **rate-limited here, server-side**: a client that polls
//! early is answered `PENDING` without the provider being asked, so a client that ignores the
//! interval cannot make this daemon hammer the provider.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use tddy_credentials::ProviderId;

use crate::deadline::{deadline_after, forget_expired, is_past};
use crate::linking::{LinkChallenge, LinkError};

/// What a poll may do, decided before the provider is consulted.
#[derive(Debug)]
pub(crate) enum PollAdmission {
    /// Ask the provider. Carries the provider the attempt was begun against.
    Go { provider: ProviderId },
    /// Earlier than the attempt's interval allows: answer `PENDING`, provider untouched.
    TooSoon { interval_seconds: u64 },
    /// The window closed. The attempt has been forgotten.
    Expired,
}

struct Attempt {
    session_token: String,
    provider: ProviderId,
    /// When the provider's code stops being redeemable, as the challenge said at begin.
    deadline: Instant,
    /// The provider's last-known minimum between polls — widened by a `slow_down`.
    interval_seconds: u64,
    /// The earliest instant the provider may be asked again. The first poll is not held back: the
    /// client's own timer paces it, and refusing it would answer `PENDING` to a link already
    /// approved.
    next_poll_at: Instant,
}

#[derive(Default)]
pub(crate) struct Attempts {
    inner: Mutex<HashMap<String, Attempt>>,
}

impl Attempts {
    fn lock(&self) -> Result<MutexGuard<'_, HashMap<String, Attempt>>, LinkError> {
        self.inner
            .lock()
            .map_err(|_| LinkError::Unavailable("link attempts are unavailable".to_string()))
    }

    /// Remember the attempt `challenge` began, forgetting any whose window has closed.
    pub(crate) fn register(
        &self,
        challenge: &LinkChallenge,
        session_token: String,
        provider: ProviderId,
        now: Instant,
    ) -> Result<(), LinkError> {
        let deadline = deadline_after(now, challenge.expires_in_seconds)?;
        let mut attempts = self.lock()?;
        forget_expired(&mut attempts, now, |attempt| attempt.deadline);
        attempts.insert(
            challenge.link_id.clone(),
            Attempt {
                session_token,
                provider,
                deadline,
                interval_seconds: challenge.interval_seconds,
                next_poll_at: now,
            },
        );
        Ok(())
    }

    /// Decide whether `session_token` may poll `link_id` now. When it may, the next poll is already
    /// scheduled, so two polls arriving together cannot both reach the provider.
    pub(crate) fn admit_poll(
        &self,
        link_id: &str,
        session_token: &str,
        now: Instant,
    ) -> Result<PollAdmission, LinkError> {
        let mut attempts = self.lock()?;
        let attempt = attempts
            .get_mut(link_id)
            .filter(|attempt| attempt.session_token == session_token)
            .ok_or(LinkError::NoSuchLink)?;
        if is_past(attempt.deadline, now) {
            attempts.remove(link_id);
            return Ok(PollAdmission::Expired);
        }
        let admission = if now < attempt.next_poll_at {
            PollAdmission::TooSoon {
                interval_seconds: attempt.interval_seconds,
            }
        } else {
            attempt.next_poll_at = deadline_after(now, attempt.interval_seconds)?;
            PollAdmission::Go {
                provider: attempt.provider.clone(),
            }
        };
        forget_expired(&mut attempts, now, |attempt| attempt.deadline);
        Ok(admission)
    }

    /// The provider answered `Pending` with `interval_seconds`, which may be wider than before.
    pub(crate) fn reschedule(
        &self,
        link_id: &str,
        interval_seconds: u64,
        now: Instant,
    ) -> Result<(), LinkError> {
        let next_poll_at = deadline_after(now, interval_seconds)?;
        if let Some(attempt) = self.lock()?.get_mut(link_id) {
            attempt.interval_seconds = interval_seconds;
            attempt.next_poll_at = next_poll_at;
        }
        Ok(())
    }

    /// The attempt reached a terminal answer.
    pub(crate) fn finish(&self, link_id: &str) -> Result<(), LinkError> {
        self.lock()?.remove(link_id);
        Ok(())
    }
}
