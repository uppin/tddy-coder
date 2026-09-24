//! The credentials sign-ins left waiting for a closed vault — held in memory only, each for a
//! limited time.
//!
//! A login over a closed vault holds its token here until an unlock, a create or a reset seals it;
//! the waiting token is also what permits a create or a reset at all (a fresh sign-in is the one
//! proof that the caller holds the account). Both lapse after the lifetime: a record past it is
//! dropped — its `SecretString` wipes itself as it goes — whenever the set is looked at, and by
//! [`PendingSignIns::expire`], which the daemon's sweep calls for the record nobody looks at.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::held;
use crate::record::CredentialRecord;

/// Where the pending set logs: a hold, an expiry, and a refusal because a sign-in expired.
pub(super) const LOG_TARGET: &str = "tddy_credentials::sessions";

/// How long a sign-in's credential waits in memory for its vault to open, unless the daemon
/// configures otherwise ([`super::SessionVaults::with_pending_lifetime`]).
///
/// The waiting credential is a live GitHub token, unsealed, and it is also what permits a first
/// passphrase or a reset (see [`super::SessionVaults::create`]); neither should outlive the moment
/// the operator could reasonably be choosing a passphrase.
pub const PENDING_LOGIN_LIFETIME: Duration = Duration::from_secs(600);

/// Where [`super::SessionVaults`] reads the time from when deciding a pending credential's age —
/// `Instant::now` in a daemon, a clock the test moves by hand in a test.
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// A credential waiting for its vault, and when the sign-in that produced it arrived.
pub(super) struct Pending {
    pub(super) record: CredentialRecord,
    since: Instant,
}

/// Every subject's waiting credentials, and which subjects' last one expired unused.
pub(super) struct PendingSignIns {
    /// By subject; each record expires after `lifetime`.
    waiting: Mutex<HashMap<String, Vec<Pending>>>,
    /// Subjects whose last pending credential expired unused, so a refusal can say so.
    expired: Mutex<HashSet<String>>,
    /// How long a pending credential waits — `None` for never.
    pub(super) lifetime: Option<Duration>,
    pub(super) clock: Clock,
}

impl Default for PendingSignIns {
    fn default() -> Self {
        Self {
            waiting: Mutex::new(HashMap::new()),
            expired: Mutex::new(HashSet::new()),
            lifetime: Some(PENDING_LOGIN_LIFETIME),
            clock: Arc::new(Instant::now),
        }
    }
}

impl PendingSignIns {
    /// Drop every credential past its lifetime, and say how many went.
    pub(super) fn expire(&self) -> usize {
        self.drop_expired(&mut held(&self.waiting))
    }

    /// Whether a credential for `subject`, still within its lifetime, is waiting.
    pub(super) fn holds(&self, subject: &str) -> bool {
        let mut waiting = held(&self.waiting);
        self.drop_expired(&mut waiting);
        waiting
            .get(subject)
            .is_some_and(|records| !records.is_empty())
    }

    /// Whether `subject`'s last waiting credential expired rather than being used or discarded.
    pub(super) fn expired_unused(&self, subject: &str) -> bool {
        held(&self.expired).contains(subject)
    }

    /// Drop everything waiting for `subject`.
    pub(super) fn discard(&self, subject: &str) {
        held(&self.waiting).remove(subject);
        held(&self.expired).remove(subject);
    }

    /// Keep `record` until `subject`'s vault opens, replacing one held for the same account.
    pub(super) fn hold(&self, subject: &str, record: CredentialRecord) {
        let mut waiting = held(&self.waiting);
        self.drop_expired(&mut waiting);
        held(&self.expired).remove(subject);
        match self.lifetime {
            Some(lifetime) => log::info!(
                target: LOG_TARGET,
                "holding the {} credential of a sign-in of '{subject}' in memory until their vault \
                 opens; it expires in {} s (at unix {})",
                record.provider.as_str(),
                lifetime.as_secs(),
                unix_seconds_in(lifetime)
            ),
            None => log::info!(
                target: LOG_TARGET,
                "holding the {} credential of a sign-in of '{subject}' in memory until their vault \
                 opens; it never expires",
                record.provider.as_str()
            ),
        }
        let records = waiting.entry(subject.to_string()).or_default();
        records.retain(|held| {
            (&held.record.provider, &held.record.account) != (&record.provider, &record.account)
        });
        records.push(Pending {
            record,
            since: (self.clock)(),
        });
    }

    /// Take everything still within its lifetime that waits for `subject`.
    pub(super) fn take(&self, subject: &str) -> Vec<Pending> {
        let mut waiting = held(&self.waiting);
        self.drop_expired(&mut waiting);
        waiting.remove(subject).unwrap_or_default()
    }

    /// Put back what [`Self::take`] took and could not seal, keeping when each sign-in arrived.
    pub(super) fn restore(&self, subject: &str, mut unsealed: Vec<Pending>) {
        held(&self.waiting)
            .entry(subject.to_string())
            .or_default()
            .append(&mut unsealed);
    }

    /// Drop from `waiting` every credential older than the lifetime, logging each; its secret is
    /// wiped as it drops. Returns how many went.
    fn drop_expired(&self, waiting: &mut HashMap<String, Vec<Pending>>) -> usize {
        let Some(lifetime) = self.lifetime else {
            return 0;
        };
        let now = (self.clock)();
        let mut dropped = 0;
        waiting.retain(|subject, records| {
            records.retain(|pending| {
                let age = now.saturating_duration_since(pending.since);
                if age < lifetime {
                    return true;
                }
                log::info!(
                    target: LOG_TARGET,
                    "dropped the {} credential a sign-in of '{subject}' left waiting for their \
                     vault: it waited {} s, its lifetime is {} s",
                    pending.record.provider.as_str(),
                    age.as_secs(),
                    lifetime.as_secs()
                );
                held(&self.expired).insert(subject.clone());
                dropped += 1;
                false
            });
            !records.is_empty()
        });
        dropped
    }
}

/// The Unix time `after` from now, for a log line; `0` on a clock before the epoch.
fn unix_seconds_in(after: Duration) -> u64 {
    std::time::SystemTime::now()
        .checked_add(after)
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs())
}
