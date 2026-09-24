//! How long a user waits before their vault's passphrase is tried again, after wrong ones.
//!
//! The passphrase is the one secret that opens a vault with no browser key, and Argon2id only makes
//! each guess cost something; it does not limit how many are made. So after a few wrong ones in a
//! row, each further attempt for that user waits twice as long as the last, up to a ceiling. The
//! count is kept **in memory only**, per user, and a right passphrase clears it. A daemon restart
//! clears it too — the restart itself costs an attacker more than the wait it forgives.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Wrong passphrases a user may give in a row before each further attempt has to wait.
pub const FREE_WRONG_PASSPHRASES: u32 = 3;

/// The wait after the first wrong passphrase past the free ones; each one after doubles it.
const FIRST_WAIT: Duration = Duration::from_secs(2);

/// The longest a user is ever asked to wait: a mistyping operator is never locked out for good.
const LONGEST_WAIT: Duration = Duration::from_secs(15 * 60);

/// Each user's run of wrong passphrases on this daemon.
#[derive(Default)]
pub(super) struct PassphraseBackoff {
    wrong: Mutex<HashMap<String, WrongRun>>,
}

/// How many wrong passphrases in a row, and when the last one was given.
struct WrongRun {
    count: u32,
    last: Instant,
}

impl PassphraseBackoff {
    /// How much longer `login` must wait before a passphrase of theirs is tried, at `now` —
    /// `None` when it may be tried at once.
    pub(super) fn retry_after(&self, login: &str, now: Instant) -> Option<Duration> {
        let wrong = self.wrong.lock().unwrap_or_else(PoisonError::into_inner);
        let run = wrong.get(login)?;
        let elapsed = now.saturating_duration_since(run.last);
        wait_after(run.count)
            .checked_sub(elapsed)
            .filter(|left| !left.is_zero())
    }

    /// `login` gave a wrong passphrase at `now`.
    pub(super) fn wrong(&self, login: &str, now: Instant) {
        let mut wrong = self.wrong.lock().unwrap_or_else(PoisonError::into_inner);
        let run = wrong.entry(login.to_string()).or_insert(WrongRun {
            count: 0,
            last: now,
        });
        run.count = run.count.saturating_add(1);
        run.last = now;
    }

    /// `login` gave the right passphrase: their run of wrong ones is over.
    pub(super) fn right(&self, login: &str) {
        self.wrong
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(login);
    }
}

/// The wait owed after `count` wrong passphrases in a row.
fn wait_after(count: u32) -> Duration {
    let Some(past_the_free) = count.checked_sub(FREE_WRONG_PASSPHRASES + 1) else {
        return Duration::ZERO;
    };
    FIRST_WAIT
        .checked_mul(2u32.saturating_pow(past_the_free))
        .map_or(LONGEST_WAIT, |wait| wait.min(LONGEST_WAIT))
}

#[cfg(test)]
mod tests {
    use super::*;

    const THE_OPERATOR: &str = "operator";
    const SOMEBODY_ELSE: &str = "somebody-else";

    /// A backoff that has seen `count` wrong passphrases from the operator, the last at `at`.
    fn after_wrong_passphrases(count: u32, at: Instant) -> PassphraseBackoff {
        let backoff = PassphraseBackoff::default();
        for _ in 0..count {
            backoff.wrong(THE_OPERATOR, at);
        }
        backoff
    }

    #[test]
    fn the_free_wrong_passphrases_cost_no_wait() {
        // Given
        let now = Instant::now();
        let backoff = after_wrong_passphrases(FREE_WRONG_PASSPHRASES, now);

        // When
        let wait = backoff.retry_after(THE_OPERATOR, now);

        // Then
        assert_eq!(wait, None);
    }

    #[test]
    fn the_first_wrong_passphrase_past_the_free_ones_costs_the_first_wait() {
        // Given
        let now = Instant::now();
        let backoff = after_wrong_passphrases(FREE_WRONG_PASSPHRASES + 1, now);

        // When
        let wait = backoff.retry_after(THE_OPERATOR, now);

        // Then
        assert_eq!(wait, Some(FIRST_WAIT));
    }

    #[test]
    fn each_further_wrong_passphrase_doubles_the_wait() {
        // Given
        let now = Instant::now();
        let backoff = after_wrong_passphrases(FREE_WRONG_PASSPHRASES + 3, now);

        // When
        let wait = backoff.retry_after(THE_OPERATOR, now);

        // Then
        assert_eq!(wait, Some(FIRST_WAIT * 4));
    }

    #[test]
    fn the_wait_never_exceeds_the_ceiling() {
        // Given
        let now = Instant::now();
        let backoff = after_wrong_passphrases(FREE_WRONG_PASSPHRASES + 60, now);

        // When
        let wait = backoff.retry_after(THE_OPERATOR, now);

        // Then
        assert_eq!(wait, Some(LONGEST_WAIT));
    }

    #[test]
    fn the_wait_counts_down_from_the_last_wrong_passphrase() {
        // Given a first wait owed since a moment ago
        let then = Instant::now();
        let backoff = after_wrong_passphrases(FREE_WRONG_PASSPHRASES + 1, then);

        // When half of it has passed, and then all of it
        let halfway = backoff.retry_after(THE_OPERATOR, then + FIRST_WAIT / 2);
        let over = backoff.retry_after(THE_OPERATOR, then + FIRST_WAIT);

        // Then
        assert_eq!((halfway, over), (Some(FIRST_WAIT / 2), None));
    }

    #[test]
    fn the_right_passphrase_ends_the_run() {
        // Given an operator who is owed a wait
        let now = Instant::now();
        let backoff = after_wrong_passphrases(FREE_WRONG_PASSPHRASES + 2, now);

        // When they give the right passphrase
        backoff.right(THE_OPERATOR);

        // Then their next attempt waits for nothing
        assert_eq!(backoff.retry_after(THE_OPERATOR, now), None);
    }

    #[test]
    fn one_users_wrong_passphrases_cost_another_nothing() {
        // Given an operator who is owed a wait
        let now = Instant::now();
        let backoff = after_wrong_passphrases(FREE_WRONG_PASSPHRASES + 5, now);

        // When somebody else unlocks their own vault
        let wait = backoff.retry_after(SOMEBODY_ELSE, now);

        // Then
        assert_eq!(wait, None);
    }
}
