//! The vaults open right now, one per signed-in user — and the credentials waiting for one to open.
//!
//! A vault opens from its owner's passphrase (`UnlockVault` in the auth service) or, after a
//! daemon restart, from the unlock key a browser lineage presents at its session refresh. The reads
//! that need a stored secret (PR status, today) arrive later with nothing but a session token, so
//! the handle an unlock opens is kept here, by subject, for them to find.
//!
//! **A login does not open a vault.** GitHub mints a new access token at every exchange, so nothing
//! a login carries is stable enough to derive a key from. A login's token is sealed at once when
//! the vault is already open here; otherwise it is held **in memory** — never written in plaintext —
//! and sealed by whichever of [`SessionVaults::unlock`], [`SessionVaults::create`] or
//! [`SessionVaults::reset`] opens the vault next. Pending credentials are held per user, not per
//! session lineage: session tokens are stateless, so a daemon cannot tell two lineages of one user
//! apart, and the latest token for a provider and account replaces an earlier one.
//!
//! **When the handle is dropped.** When the last unlock slot is removed — the last lineage signed
//! out — no signed-in lineage can use the vault, and its data key leaves memory with the handle. A
//! handle whose file was replaced or removed underneath it is dropped the next time it is looked up.
//! TODO(keyring): a lineage that simply stops refreshing (its refresh token lapses after seven
//! days) never signs out, so its vault stays open until the daemon exits — see
//! docs/dev/todo/2026-09-23-credential-vault-open-past-its-last-session.md.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::record::CredentialRecord;
use crate::secret::SecretString;
use crate::vault::{CredentialStore, SessionVault, UnlockKey, VaultError};

/// Where this module logs: the credentials a sign-in left waiting, and when they expire.
const LOG_TARGET: &str = "tddy_credentials::sessions";

/// How long a sign-in's credential waits in memory for its vault to open, unless the daemon
/// configures otherwise ([`SessionVaults::with_pending_lifetime`]).
///
/// The waiting credential is a live GitHub token, unsealed, and it is also what permits a first
/// passphrase or a reset (see [`SessionVaults::create`]); neither should outlive the moment the
/// operator could reasonably be choosing a passphrase.
pub const PENDING_LOGIN_LIFETIME: Duration = Duration::from_secs(600);

/// Where [`SessionVaults`] reads the time from when deciding a pending credential's age —
/// `Instant::now` in a daemon, a clock the test moves by hand in a test.
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// How long the key a refresh just rotated away from still answers, with the key it was rotated to.
///
/// Two tabs of one browser share one stored unlock key and refresh on load together, and a
/// response can be lost on the way back. Either way a second refresh arrives presenting the key the
/// first one retired; within this window it is handed the same successor instead of nothing, so
/// both tabs end up holding the one key that opens the slot. A cross-tab lock in the page was the
/// alternative, and was not taken: the Web Locks API exists only in secure contexts, and the
/// dashboard is served over plain-http LAN origins.
pub const ROTATION_GRACE: Duration = Duration::from_secs(30);

/// Where one user's vault stands on this daemon, as a login or a refresh reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultState {
    /// Open in memory: credentials are readable and a new lineage is handed an unlock slot.
    Open,
    /// A vault file exists and nothing has opened it since this daemon started. Its passphrase, or
    /// a refresh presenting an unlock key, opens it.
    Locked,
    /// No vault file exists. Choosing a passphrase creates one.
    Uninitialized,
}

/// What a login's credential met: the vault's state, and — when it was open — the unlock key
/// handed to the signing-in lineage.
#[derive(Debug)]
pub struct Retained {
    pub state: VaultState,
    pub unlock_key: Option<UnlockKey>,
}

/// What a reset did: where the old vault was set aside (`None` when there was none), and the
/// unlock key to the fresh one.
#[derive(Debug)]
pub struct Reset {
    pub set_aside: Option<PathBuf>,
    pub unlock_key: UnlockKey,
}

/// Every user's open [`SessionVault`], keyed by subject, over one `auth_storage` directory — and
/// the credentials that arrived while a vault was closed, waiting for it to open.
pub struct SessionVaults {
    auth_storage_dir: PathBuf,
    rotation_grace: Duration,
    open: Mutex<HashMap<String, Arc<SessionVault>>>,
    /// Credentials logins produced while their vault was closed, by subject, with when each was
    /// held. Each expires after `pending_lifetime`; its secret is wiped as the record drops.
    pending: Mutex<HashMap<String, Vec<Pending>>>,
    /// How long a pending credential waits — `None` for never.
    pending_lifetime: Option<Duration>,
    /// Subjects whose last pending credential expired unused, so a refusal can say so.
    expired: Mutex<HashSet<String>>,
    clock: Clock,
    /// The latest rotation of each `(subject, slot id)`, for the grace window. Also what serialises
    /// refreshes: a rotation and the answer to a refresh racing it happen under this lock.
    rotations: Mutex<HashMap<(String, String), Rotation>>,
    /// Held across every change of which vaults are open and what waits for them: a login's
    /// look-up-then-retain, and an unlock's, create's or reset's seal-then-register. Without it a
    /// login could find a vault closed, an unlock open it and seal what was waiting, and the
    /// login's token then be left waiting beside an open vault while the login is told "open"
    /// with no key to it. Key derivation happens outside it, so a slow passphrase holds up nobody.
    ///
    /// Lock order: `rotations` before `transitions`, never the other way round.
    transitions: Mutex<()>,
}

/// A credential waiting for its vault, and when the sign-in that produced it arrived.
struct Pending {
    record: CredentialRecord,
    since: Instant,
}

/// One rotation of one slot: the key it retired, the key it handed out, and when.
struct Rotation {
    retired: UnlockKey,
    successor: UnlockKey,
    at: Instant,
}

/// A poisoned lock means a panic mid-update of an in-memory map; every entry is either whole or
/// absent, and a missing one only means a vault reads as closed, so the map is used as it stands.
fn held<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

impl SessionVaults {
    /// No vault open yet; each subject's file lives at [`CredentialStore::path_in`] under
    /// `auth_storage_dir`.
    #[must_use]
    pub fn new(auth_storage_dir: impl Into<PathBuf>) -> Self {
        Self {
            auth_storage_dir: auth_storage_dir.into(),
            rotation_grace: ROTATION_GRACE,
            open: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            pending_lifetime: Some(PENDING_LOGIN_LIFETIME),
            expired: Mutex::new(HashSet::new()),
            clock: Arc::new(Instant::now),
            rotations: Mutex::new(HashMap::new()),
            transitions: Mutex::new(()),
        }
    }

    /// Answer a retired unlock key for `grace` after its rotation rather than [`ROTATION_GRACE`].
    #[must_use]
    pub fn with_rotation_grace(mut self, grace: Duration) -> Self {
        self.rotation_grace = grace;
        self
    }

    /// Let a pending credential wait `lifetime` for its vault rather than
    /// [`PENDING_LOGIN_LIFETIME`] — `None` for as long as the daemon runs (builder).
    #[must_use]
    pub fn with_pending_lifetime(mut self, lifetime: Option<Duration>) -> Self {
        self.pending_lifetime = lifetime;
        self
    }

    /// Read the time from `clock` rather than [`Instant::now`] (builder).
    #[must_use]
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// How long a pending credential waits for its vault — `None` when it never expires.
    #[must_use]
    pub fn pending_lifetime(&self) -> Option<Duration> {
        self.pending_lifetime
    }

    /// Drop every pending credential older than its lifetime — the periodic sweep, so a token
    /// nobody touches does not wait in memory past its time — and say how many were dropped.
    /// Each lookup that consults the pending set expires what is due as well.
    pub fn expire_pending(&self) -> usize {
        self.drop_expired(&mut held(&self.pending))
    }

    /// Where `subject`'s vault lives.
    #[must_use]
    pub fn path_for(&self, subject: &str) -> PathBuf {
        CredentialStore::path_in(&self.auth_storage_dir, subject)
    }

    /// The directory every subject's vault lives in.
    #[must_use]
    pub fn auth_storage_dir(&self) -> &Path {
        &self.auth_storage_dir
    }

    /// Where `subject`'s vault stands right now.
    #[must_use]
    pub fn state(&self, subject: &str) -> VaultState {
        if self.get(subject).is_some() {
            VaultState::Open
        } else if self.path_for(subject).exists() {
            VaultState::Locked
        } else {
            VaultState::Uninitialized
        }
    }

    /// Whether a credential for `subject` is waiting in memory for their vault to open.
    #[must_use]
    pub fn holds_pending(&self, subject: &str) -> bool {
        let mut pending = held(&self.pending);
        self.drop_expired(&mut pending);
        pending
            .get(subject)
            .is_some_and(|records| !records.is_empty())
    }

    /// Drop every credential waiting in memory for `subject`'s vault — a sign-out by a lineage
    /// that never opened it.
    ///
    /// Pending credentials are per user, not per lineage, so this also drops one another lineage
    /// of the same user left waiting; that lineage was told its vault is closed, and signs in again
    /// for its token to be kept.
    pub fn discard_pending(&self, subject: &str) {
        let _transition = held(&self.transitions);
        held(&self.pending).remove(subject);
        held(&self.expired).remove(subject);
    }

    /// Retain a credential a login produced for `subject`.
    ///
    /// With the vault open, `record` is sealed into it and the lineage is handed an unlock slot.
    /// With it closed, `record` is kept **in memory only**, never written in plaintext, and sealed
    /// when [`Self::unlock`], [`Self::create`] or [`Self::reset`] next opens the vault; the state
    /// says which of those the user is asked for. A failed write is an `Err` — it fails the login.
    pub fn retain(&self, subject: &str, record: CredentialRecord) -> Result<Retained, VaultError> {
        let _transition = held(&self.transitions);
        if let Some(vault) = self.get(subject) {
            match vault
                .put(record.clone())
                .and_then(|()| vault.add_unlock_slot())
            {
                Ok(unlock_key) => {
                    return Ok(Retained {
                        state: VaultState::Open,
                        unlock_key: Some(unlock_key),
                    })
                }
                // Replaced underneath the handle between the lookup and the write.
                Err(VaultError::Locked) => self.close(subject, &vault),
                Err(failed) => return Err(failed),
            }
        }
        self.hold_pending(subject, record);
        Ok(Retained {
            state: self.state(subject),
            unlock_key: None,
        })
    }

    /// Open `subject`'s vault with its passphrase, seal what was pending, and hand the lineage an
    /// unlock slot. A wrong passphrase is [`VaultError::Locked`] and changes nothing.
    pub fn unlock(
        &self,
        subject: &str,
        passphrase: &SecretString,
    ) -> Result<UnlockKey, VaultError> {
        let vault =
            CredentialStore::open_with_passphrase(&self.path_for(subject), passphrase, subject)?;
        let _transition = held(&self.transitions);
        self.admit(subject, vault)
    }

    /// Create `subject`'s vault under a first passphrase, seal what was pending, and hand the
    /// lineage an unlock slot. A vault that already exists is [`VaultError::AlreadyInitialized`].
    ///
    /// Only while a fresh login's credential is waiting for it ([`VaultError::NoFreshLogin`]
    /// otherwise): choosing the passphrase is what decides who controls the vault, and a session
    /// token alone — a copy of which may have been taken off the wire — does not prove that the
    /// caller holds the account.
    pub fn create(
        &self,
        subject: &str,
        passphrase: &SecretString,
    ) -> Result<UnlockKey, VaultError> {
        let _transition = held(&self.transitions);
        self.require_a_fresh_login(subject)?;
        let vault = CredentialStore::create(&self.path_for(subject), passphrase, subject)?;
        self.admit(subject, vault)
    }

    /// Set `subject`'s vault aside (renamed, never deleted) and create a fresh one under
    /// `new_passphrase`, sealing what was pending. Every unlock key to the old vault opens nothing.
    ///
    /// Refused unless a fresh login's credential is waiting for the vault
    /// ([`VaultError::NoFreshLogin`], for the reason [`Self::create`] gives), refused while the
    /// vault is open here ([`VaultError::AlreadyOpen`] — nothing was forgotten), and refused rather
    /// than deleting anything once [`crate::MAX_SET_ASIDE_VAULTS`] old vaults are set aside.
    pub fn reset(&self, subject: &str, new_passphrase: &SecretString) -> Result<Reset, VaultError> {
        let reset = {
            let _transition = held(&self.transitions);
            if self.get(subject).is_some() {
                return Err(VaultError::AlreadyOpen);
            }
            self.require_a_fresh_login(subject)?;
            let (vault, set_aside) =
                CredentialStore::reset(&self.path_for(subject), new_passphrase, subject)?;
            Reset {
                unlock_key: self.admit(subject, vault)?,
                set_aside,
            }
        };
        // After `transitions` is released: `rotations` is always taken first.
        held(&self.rotations).retain(|(rotated, _), _| rotated != subject);
        Ok(reset)
    }

    /// Reopen a vault through the unlock slot `unlock` names — a session refresh, possibly the
    /// first contact since a daemon restart — and hand back the key the slot is rotated to.
    ///
    /// The presented key must open its slot even when the vault is already open here: holding a
    /// slot id is not possession of the key. A key that does not (a slot rotated since, logged out
    /// or evicted) is [`VaultError::Locked`], and nothing is registered or changed — unless it is
    /// the key this slot was rotated away from within the grace window, which is answered with the
    /// same successor.
    pub fn reopen(&self, unlock: &UnlockKey) -> Result<UnlockKey, VaultError> {
        let mut rotations = held(&self.rotations);
        let subject = unlock.subject();
        let path = self.path_for(subject);
        let slot = (subject.to_string(), unlock.slot_id().to_string());
        match CredentialStore::open_with_unlock_key(&path, unlock) {
            Ok(proven) => {
                let successor = proven.rotate_unlock_slot(unlock)?;
                let grace = self.rotation_grace;
                rotations.retain(|_, rotation| rotation.at.elapsed() < grace);
                rotations.insert(
                    slot,
                    Rotation {
                        retired: unlock.duplicate(),
                        successor: successor.duplicate(),
                        at: Instant::now(),
                    },
                );
                let _transition = held(&self.transitions);
                self.register(subject, proven);
                Ok(successor)
            }
            Err(VaultError::Locked) => {
                let rotation = rotations
                    .get(&slot)
                    .filter(|rotation| rotation.at.elapsed() < self.rotation_grace)
                    .filter(|rotation| rotation.retired.is_the_same_key_as(unlock))
                    .ok_or(VaultError::Locked)?;
                let _transition = held(&self.transitions);
                if self.get(subject).is_none() {
                    self.register(
                        subject,
                        CredentialStore::open_with_unlock_key(&path, &rotation.successor)?,
                    );
                }
                Ok(rotation.successor.duplicate())
            }
            Err(failed) => Err(failed),
        }
    }

    /// Remove the unlock slot `unlock` names — a logout. The key must still open its slot, so a
    /// slot id alone cannot remove somebody else's; a key that does not (the slot is already gone,
    /// or was rotated since) is [`VaultError::Locked`] and nothing is removed.
    ///
    /// When that was the vault's last slot, no signed-in lineage is left to use it, and the open
    /// handle — the data key in memory — is dropped with it.
    pub fn forget(&self, unlock: &UnlockKey) -> Result<(), VaultError> {
        let subject = unlock.subject();
        let vault = CredentialStore::open_with_unlock_key(&self.path_for(subject), unlock)?;
        vault.remove_unlock_slot(unlock.slot_id())?;
        held(&self.rotations).remove(&(subject.to_string(), unlock.slot_id().to_string()));
        let _transition = held(&self.transitions);
        if vault.unlock_slot_ids()?.is_empty() {
            held(&self.open).remove(subject);
        }
        Ok(())
    }

    /// `subject`'s open vault, or `None` when nothing since this daemon started has opened it.
    ///
    /// A handle whose file was replaced or removed underneath it is dropped rather than returned.
    #[must_use]
    pub fn get(&self, subject: &str) -> Option<Arc<SessionVault>> {
        let vault = held(&self.open).get(subject).cloned()?;
        if vault.is_stale() {
            self.close(subject, &vault);
            return None;
        }
        Some(vault)
    }

    /// [`VaultError::NoFreshLogin`] unless a login's credential is waiting for `subject`'s vault.
    fn require_a_fresh_login(&self, subject: &str) -> Result<(), VaultError> {
        if self.holds_pending(subject) {
            return Ok(());
        }
        if held(&self.expired).contains(subject) {
            log::warn!(
                target: LOG_TARGET,
                "refused to choose a credential vault passphrase for '{subject}': the GitHub \
                 sign-in that allowed it expired; they must sign in to GitHub again"
            );
        }
        Err(VaultError::NoFreshLogin)
    }

    /// Drop from `pending` every credential older than the lifetime, logging each; its secret is
    /// wiped as it drops. Returns how many went.
    fn drop_expired(&self, pending: &mut HashMap<String, Vec<Pending>>) -> usize {
        let Some(lifetime) = self.pending_lifetime else {
            return 0;
        };
        let now = (self.clock)();
        let mut dropped = 0;
        pending.retain(|subject, records| {
            records.retain(|waiting| {
                let age = now.saturating_duration_since(waiting.since);
                if age < lifetime {
                    return true;
                }
                log::info!(
                    target: LOG_TARGET,
                    "dropped the {} credential a sign-in of '{subject}' left waiting for their \
                     vault: it waited {} s, its lifetime is {} s",
                    waiting.record.provider.as_str(),
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

    /// Seal what was waiting for `subject`'s newly opened `vault`, hand the lineage that opened it
    /// an unlock slot, and keep the handle. Called holding `transitions`.
    fn admit(&self, subject: &str, vault: SessionVault) -> Result<UnlockKey, VaultError> {
        self.seal_pending(subject, &vault)?;
        let unlock_key = vault.add_unlock_slot()?;
        self.register(subject, vault);
        Ok(unlock_key)
    }

    fn register(&self, subject: &str, vault: SessionVault) {
        held(&self.open).insert(subject.to_string(), Arc::new(vault));
    }

    /// Drop `subject`'s handle if it is still `vault` — not one a concurrent unlock put there since.
    fn close(&self, subject: &str, vault: &Arc<SessionVault>) {
        let mut open = held(&self.open);
        if open
            .get(subject)
            .is_some_and(|held| Arc::ptr_eq(held, vault))
        {
            open.remove(subject);
        }
    }

    /// Keep `record` until `subject`'s vault opens, replacing one held for the same account.
    fn hold_pending(&self, subject: &str, record: CredentialRecord) {
        let mut pending = held(&self.pending);
        self.drop_expired(&mut pending);
        held(&self.expired).remove(subject);
        match self.pending_lifetime {
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
        let records = pending.entry(subject.to_string()).or_default();
        records.retain(|held| {
            (&held.record.provider, &held.record.account) != (&record.provider, &record.account)
        });
        records.push(Pending {
            record,
            since: (self.clock)(),
        });
    }

    /// Seal every credential waiting for `subject` into `vault`. What fails to seal is kept
    /// waiting, and the failure reported — nothing held in memory is dropped by a failed write.
    fn seal_pending(&self, subject: &str, vault: &SessionVault) -> Result<(), VaultError> {
        let mut waiting = {
            let mut pending = held(&self.pending);
            self.drop_expired(&mut pending);
            pending.remove(subject).unwrap_or_default()
        };
        while let Some(Pending { record, .. }) = waiting.first() {
            if let Err(failed) = vault.put(record.clone()) {
                held(&self.pending)
                    .entry(subject.to_string())
                    .or_default()
                    .append(&mut waiting);
                return Err(failed);
            }
            waiting.remove(0);
        }
        Ok(())
    }
}

/// The Unix time `after` from now, for a log line; `0` on a clock before the epoch.
fn unix_seconds_in(after: Duration) -> u64 {
    std::time::SystemTime::now()
        .checked_add(after)
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{AccountId, ProviderId};
    use crate::vault::MAX_SET_ASIDE_VAULTS;

    const THE_OPERATOR: &str = "operator";
    const THE_PASSPHRASE: &str = "correct horse battery staple";
    const A_WRONG_PASSPHRASE: &str = "incorrect horse battery staple";
    const A_NEW_PASSPHRASE: &str = "a passphrase chosen after forgetting";
    const THE_FIRST_TOKEN: &str = "gho_from_the_first_login";
    const A_LATER_TOKEN: &str = "gho_from_a_later_login";

    fn the_passphrase() -> SecretString {
        SecretString::new(THE_PASSPHRASE)
    }

    fn a_github_record(token: &str) -> CredentialRecord {
        CredentialRecord {
            provider: ProviderId::new("github"),
            account: AccountId::new(THE_OPERATOR),
            label: "Work".to_string(),
            secret: SecretString::new(token),
            metadata: Default::default(),
            updated_at: 1,
        }
    }

    fn the_stored_token(vaults: &SessionVaults) -> Option<String> {
        vaults
            .get(THE_OPERATOR)?
            .get(&ProviderId::new("github"), &AccountId::new(THE_OPERATOR))
            .ok()?
            .map(|record| record.secret.expose().to_string())
    }

    /// A daemon whose operator created their vault at a first login, holding its unlock key.
    fn a_daemon_with_a_created_vault(dir: &Path) -> (SessionVaults, UnlockKey) {
        let vaults = SessionVaults::new(dir);
        vaults
            .retain(THE_OPERATOR, a_github_record(THE_FIRST_TOKEN))
            .unwrap();
        let key = vaults.create(THE_OPERATOR, &the_passphrase()).unwrap();
        (vaults, key)
    }

    /// The daemon after a restart, with a fresh login's token waiting for the vault it found locked.
    fn a_restarted_daemon_with_a_login_waiting(dir: &Path) -> SessionVaults {
        let vaults = SessionVaults::new(dir);
        vaults
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .unwrap();
        vaults
    }

    /// `n` vaults of the operator's already set aside beside the live one, by earlier resets.
    fn vaults_already_set_aside(dir: &Path, n: usize) -> Vec<PathBuf> {
        let live = CredentialStore::path_in(dir, THE_OPERATOR);
        let stem = live.file_stem().unwrap().to_string_lossy().into_owned();
        (0..n)
            .map(|at| {
                let aside = dir.join(format!("{stem}.locked-{}.vault", 1_700_000_000 + at));
                std::fs::write(&aside, b"an old vault").unwrap();
                aside
            })
            .collect()
    }

    /// What `retained` told the login is true of the vault afterwards: reported open, it holds an
    /// unlock key and its token is sealed; reported closed, it holds no key.
    fn the_login_was_told_the_truth(vaults: &SessionVaults, retained: &Retained) -> bool {
        match retained.state {
            VaultState::Open => {
                retained.unlock_key.is_some()
                    && the_stored_token(vaults) == Some(A_LATER_TOKEN.to_string())
            }
            VaultState::Locked | VaultState::Uninitialized => retained.unlock_key.is_none(),
        }
    }

    #[test]
    fn a_first_login_finds_no_vault_and_writes_nothing() {
        // Given a daemon that has never held this operator's vault
        let dir = tempfile::tempdir().unwrap();
        let vaults = SessionVaults::new(dir.path());

        // When their first login's token is retained
        let retained = vaults
            .retain(THE_OPERATOR, a_github_record(THE_FIRST_TOKEN))
            .unwrap();

        // Then the vault is reported uninitialized, no file exists, and the token waits in memory
        assert_eq!(
            (
                retained.state,
                retained.unlock_key.is_none(),
                vaults.path_for(THE_OPERATOR).exists(),
                vaults.holds_pending(THE_OPERATOR)
            ),
            (VaultState::Uninitialized, true, false, true)
        );
    }

    #[test]
    fn creating_the_vault_seals_the_credential_that_was_waiting() {
        // Given a first login whose token is waiting for a vault
        let dir = tempfile::tempdir().unwrap();
        let vaults = SessionVaults::new(dir.path());
        vaults
            .retain(THE_OPERATOR, a_github_record(THE_FIRST_TOKEN))
            .unwrap();

        // When the operator chooses a passphrase
        vaults.create(THE_OPERATOR, &the_passphrase()).unwrap();

        // Then the token is sealed and readable, and nothing is left waiting
        assert_eq!(
            (
                the_stored_token(&vaults),
                vaults.holds_pending(THE_OPERATOR)
            ),
            (Some(THE_FIRST_TOKEN.to_string()), false)
        );
    }

    #[test]
    fn after_a_restart_a_login_with_a_new_token_finds_the_vault_locked_and_changes_nothing() {
        // Given a created vault, and a daemon that has since restarted
        let dir = tempfile::tempdir().unwrap();
        a_daemon_with_a_created_vault(dir.path());
        let before = std::fs::read(CredentialStore::path_in(dir.path(), THE_OPERATOR)).unwrap();
        let after_restart = SessionVaults::new(dir.path());

        // When a fresh login arrives with the different token GitHub minted for it
        let retained = after_restart
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .unwrap();

        // Then the vault is reported locked, and the file is exactly as it was
        assert_eq!(
            (
                retained.state,
                std::fs::read(after_restart.path_for(THE_OPERATOR)).ok()
            ),
            (VaultState::Locked, Some(before))
        );
    }

    #[test]
    fn after_a_restart_the_passphrase_opens_the_vault_a_new_token_found_locked() {
        // Given a login with a new token that found the vault locked after a restart
        let dir = tempfile::tempdir().unwrap();
        a_daemon_with_a_created_vault(dir.path());
        let after_restart = SessionVaults::new(dir.path());
        after_restart
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .unwrap();

        // When the operator gives their passphrase
        after_restart
            .unlock(THE_OPERATOR, &the_passphrase())
            .unwrap();

        // Then the same vault is open, holding the new token
        assert_eq!(
            the_stored_token(&after_restart),
            Some(A_LATER_TOKEN.to_string())
        );
    }

    #[test]
    fn a_wrong_passphrase_leaves_the_vault_locked_the_file_unchanged_and_the_token_waiting() {
        // Given a locked vault with a login's token waiting for it
        let dir = tempfile::tempdir().unwrap();
        a_daemon_with_a_created_vault(dir.path());
        let before = std::fs::read(CredentialStore::path_in(dir.path(), THE_OPERATOR)).unwrap();
        let after_restart = SessionVaults::new(dir.path());
        after_restart
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .unwrap();

        // When the wrong passphrase is given
        let refused = after_restart
            .unlock(THE_OPERATOR, &SecretString::new(A_WRONG_PASSPHRASE))
            .err();

        // Then
        assert_eq!(
            (
                refused,
                after_restart.state(THE_OPERATOR),
                std::fs::read(after_restart.path_for(THE_OPERATOR)).ok(),
                after_restart.holds_pending(THE_OPERATOR)
            ),
            (
                Some(VaultError::Locked),
                VaultState::Locked,
                Some(before),
                true
            )
        );
    }

    #[test]
    fn a_login_while_the_vault_is_open_needs_no_passphrase_and_is_handed_an_unlock_slot() {
        // Given an operator whose vault is open on this daemon
        let dir = tempfile::tempdir().unwrap();
        let (vaults, _first_lineage) = a_daemon_with_a_created_vault(dir.path());

        // When they sign in from a second browser, with the new token GitHub minted for it
        let retained = vaults
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .unwrap();

        // Then the vault stays open, the new lineage has its own key, and the new token is sealed
        assert_eq!(
            (
                retained.state,
                retained
                    .unlock_key
                    .map(|key| CredentialStore::open_with_unlock_key(
                        &vaults.path_for(THE_OPERATOR),
                        &key
                    )
                    .is_ok()),
                the_stored_token(&vaults)
            ),
            (
                VaultState::Open,
                Some(true),
                Some(A_LATER_TOKEN.to_string())
            )
        );
    }

    #[test]
    fn a_refresh_after_a_restart_reopens_the_vault_through_its_unlock_slot() {
        // Given a lineage holding an unlock key, and then a daemon restart
        let dir = tempfile::tempdir().unwrap();
        let (_before_restart, key) = a_daemon_with_a_created_vault(dir.path());
        let after_restart = SessionVaults::new(dir.path());

        // When the browser refreshes its session, presenting the key
        after_restart.reopen(&key).unwrap();

        // Then the vault is open again, with no passphrase
        assert_eq!(
            the_stored_token(&after_restart),
            Some(THE_FIRST_TOKEN.to_string())
        );
    }

    #[test]
    fn a_rotated_unlock_key_is_the_only_one_that_opens_the_slot() {
        // Given a lineage whose key was rotated at a refresh
        let dir = tempfile::tempdir().unwrap();
        let (vaults, presented) = a_daemon_with_a_created_vault(dir.path());
        let rotated = vaults.reopen(&presented).unwrap();

        // When each is presented to the vault
        let path = vaults.path_for(THE_OPERATOR);
        let opens = |key: &UnlockKey| CredentialStore::open_with_unlock_key(&path, key).is_ok();

        // Then only the rotated one opens it
        assert_eq!((opens(&presented), opens(&rotated)), (false, true));
    }

    #[test]
    fn a_retired_key_presented_within_the_grace_window_is_handed_the_same_successor() {
        // Given a lineage that refreshed once, and a second tab still holding the retired key
        let dir = tempfile::tempdir().unwrap();
        let (vaults, retired) = a_daemon_with_a_created_vault(dir.path());
        let successor = vaults.reopen(&retired).unwrap();

        // When the second tab's refresh presents the retired key moments later
        let answered = vaults.reopen(&retired).map(|key| key.to_wire());

        // Then both tabs end up holding the one key that opens the slot
        assert_eq!(answered, Ok(successor.to_wire()));
    }

    #[test]
    fn a_retired_key_presented_after_the_grace_window_opens_nothing() {
        // Given a daemon with no grace window, and a lineage that refreshed once
        let dir = tempfile::tempdir().unwrap();
        let (vaults, retired) = a_daemon_with_a_created_vault(dir.path());
        let vaults = vaults.with_rotation_grace(Duration::ZERO);
        vaults.reopen(&retired).unwrap();

        // When the retired key is presented again
        let answered = vaults.reopen(&retired).err();

        // Then
        assert_eq!(answered, Some(VaultError::Locked));
    }

    #[test]
    fn two_refreshes_racing_with_one_key_both_receive_the_same_working_successor() {
        // Given one unlock key, shared by two tabs that refresh at the same moment
        let dir = tempfile::tempdir().unwrap();
        let (vaults, shared) = a_daemon_with_a_created_vault(dir.path());
        let at_once = std::sync::Barrier::new(2);

        // When both present it
        let answers: Vec<String> = std::thread::scope(|scope| {
            let racers: Vec<_> = (0..2)
                .map(|_| {
                    let (vaults, shared, at_once) = (&vaults, &shared, &at_once);
                    scope.spawn(move || {
                        at_once.wait();
                        vaults.reopen(shared).unwrap().to_wire()
                    })
                })
                .collect();
            racers
                .into_iter()
                .map(|racer| racer.join().unwrap())
                .collect()
        });

        // Then neither lost its lineage: they hold one key, and it opens the slot
        let successor = UnlockKey::from_wire(&answers[0]).unwrap();
        assert_eq!(
            (
                answers[0] == answers[1],
                CredentialStore::open_with_unlock_key(&vaults.path_for(THE_OPERATOR), &successor)
                    .is_ok()
            ),
            (true, true)
        );
    }

    #[test]
    fn a_reset_sets_the_old_vault_aside_and_seals_the_waiting_token_into_a_fresh_one() {
        // Given a locked vault whose passphrase is forgotten, with a login's token waiting
        let dir = tempfile::tempdir().unwrap();
        a_daemon_with_a_created_vault(dir.path());
        let old_bytes = std::fs::read(CredentialStore::path_in(dir.path(), THE_OPERATOR)).unwrap();
        let after_restart = SessionVaults::new(dir.path());
        after_restart
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .unwrap();

        // When the operator resets it under a new passphrase
        let reset = after_restart
            .reset(THE_OPERATOR, &SecretString::new(A_NEW_PASSPHRASE))
            .unwrap();

        // Then the old file is kept aside intact, and the fresh vault holds the waiting token
        assert_eq!(
            (
                reset.set_aside.and_then(|aside| std::fs::read(aside).ok()),
                the_stored_token(&after_restart)
            ),
            (Some(old_bytes), Some(A_LATER_TOKEN.to_string()))
        );
    }

    #[test]
    fn after_a_reset_an_unlock_key_to_the_old_vault_opens_nothing() {
        // Given a lineage holding a key to a vault that is reset after a restart and a fresh login
        let dir = tempfile::tempdir().unwrap();
        let (_before_restart, old_key) = a_daemon_with_a_created_vault(dir.path());
        let vaults = a_restarted_daemon_with_a_login_waiting(dir.path());
        vaults
            .reset(THE_OPERATOR, &SecretString::new(A_NEW_PASSPHRASE))
            .unwrap();

        // When that lineage refreshes
        let answered = vaults.reopen(&old_key).err();

        // Then
        assert_eq!(answered, Some(VaultError::Locked));
    }

    #[test]
    fn a_handle_whose_file_was_replaced_underneath_it_is_dropped_and_the_vault_reads_as_locked() {
        // Given an open vault whose file another process resets underneath this daemon
        let dir = tempfile::tempdir().unwrap();
        let (vaults, _key) = a_daemon_with_a_created_vault(dir.path());
        CredentialStore::reset(
            &vaults.path_for(THE_OPERATOR),
            &SecretString::new(A_NEW_PASSPHRASE),
            THE_OPERATOR,
        )
        .unwrap();

        // When the next login's token arrives
        let retained = vaults
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .map(|retained| retained.state);

        // Then the stale handle is not trusted — the vault on disk is locked, not "failed"
        assert_eq!(retained, Ok(VaultState::Locked));
    }

    #[test]
    fn the_last_lineage_signing_out_drops_the_open_vault() {
        // Given an operator signed in from one browser
        let dir = tempfile::tempdir().unwrap();
        let (vaults, only_lineage) = a_daemon_with_a_created_vault(dir.path());

        // When that browser signs out
        vaults.forget(&only_lineage).unwrap();

        // Then the daemon no longer holds the key to their credentials
        assert_eq!(
            (
                vaults.get(THE_OPERATOR).is_none(),
                vaults.state(THE_OPERATOR)
            ),
            (true, VaultState::Locked)
        );
    }

    #[test]
    fn a_lineage_signing_out_while_another_is_signed_in_keeps_the_vault_open() {
        // Given an operator signed in from two browsers
        let dir = tempfile::tempdir().unwrap();
        let (vaults, first_lineage) = a_daemon_with_a_created_vault(dir.path());
        vaults
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .unwrap();

        // When one of them signs out
        vaults.forget(&first_lineage).unwrap();

        // Then
        assert_eq!(vaults.state(THE_OPERATOR), VaultState::Open);
    }

    #[test]
    fn holds_nothing_for_a_subject_no_login_has_opened() {
        // Given
        let dir = tempfile::tempdir().unwrap();

        // When
        let vaults = SessionVaults::new(dir.path());

        // Then
        assert_eq!(vaults.state(THE_OPERATOR), VaultState::Uninitialized);
    }

    #[test]
    fn a_first_passphrase_with_no_login_waiting_is_refused_and_creates_nothing() {
        // Given a daemon holding no credential from a login for this operator
        let dir = tempfile::tempdir().unwrap();
        let vaults = SessionVaults::new(dir.path());

        // When a first passphrase is chosen anyway
        let refused = vaults.create(THE_OPERATOR, &the_passphrase()).err();

        // Then only a fresh sign-in can create the vault, and nothing is written
        assert_eq!(
            (refused, vaults.path_for(THE_OPERATOR).exists()),
            (Some(VaultError::NoFreshLogin), false)
        );
    }

    #[test]
    fn a_reset_with_no_login_waiting_is_refused_and_leaves_the_vault_as_it_was() {
        // Given a locked vault after a restart, and no login since
        let dir = tempfile::tempdir().unwrap();
        a_daemon_with_a_created_vault(dir.path());
        let before = std::fs::read(CredentialStore::path_in(dir.path(), THE_OPERATOR)).unwrap();
        let after_restart = SessionVaults::new(dir.path());

        // When a reset is asked for
        let refused = after_restart
            .reset(THE_OPERATOR, &SecretString::new(A_NEW_PASSPHRASE))
            .err();

        // Then
        assert_eq!(
            (
                refused,
                std::fs::read(after_restart.path_for(THE_OPERATOR)).ok()
            ),
            (Some(VaultError::NoFreshLogin), Some(before))
        );
    }

    #[test]
    fn a_reset_while_the_vault_is_open_is_refused() {
        // Given a vault open on this daemon, and a second login's token sealed straight into it
        let dir = tempfile::tempdir().unwrap();
        let (vaults, _key) = a_daemon_with_a_created_vault(dir.path());
        vaults
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .unwrap();

        // When a reset is asked for
        let refused = vaults
            .reset(THE_OPERATOR, &SecretString::new(A_NEW_PASSPHRASE))
            .err();

        // Then
        assert_eq!(refused, Some(VaultError::AlreadyOpen));
    }

    #[test]
    fn a_reset_with_the_set_aside_vaults_at_their_cap_is_refused_and_deletes_nothing() {
        // Given a locked vault with as many old vaults set aside as are kept, and a fresh login
        let dir = tempfile::tempdir().unwrap();
        a_daemon_with_a_created_vault(dir.path());
        let before = std::fs::read(CredentialStore::path_in(dir.path(), THE_OPERATOR)).unwrap();
        let set_aside = vaults_already_set_aside(dir.path(), MAX_SET_ASIDE_VAULTS);
        let vaults = a_restarted_daemon_with_a_login_waiting(dir.path());

        // When a reset is asked for
        let refused = vaults
            .reset(THE_OPERATOR, &SecretString::new(A_NEW_PASSPHRASE))
            .err();

        // Then it is refused, the live vault is untouched, and every set-aside one is still there
        assert_eq!(
            (
                refused,
                std::fs::read(vaults.path_for(THE_OPERATOR)).ok(),
                set_aside.iter().all(|aside| aside.exists())
            ),
            (
                Some(VaultError::TooManySetAside {
                    kept: MAX_SET_ASIDE_VAULTS
                }),
                Some(before),
                true
            )
        );
    }

    #[test]
    fn a_login_racing_an_unlock_is_told_the_truth_about_the_vault() {
        // Given a locked vault after a restart, with a first login's token waiting for it
        let dir = tempfile::tempdir().unwrap();
        a_daemon_with_a_created_vault(dir.path());
        let vaults = SessionVaults::new(dir.path());
        vaults
            .retain(THE_OPERATOR, a_github_record(THE_FIRST_TOKEN))
            .unwrap();
        let at_once = std::sync::Barrier::new(2);

        // When a second login and the passphrase arrive at the same moment
        let retained = std::thread::scope(|scope| {
            let unlocking = scope.spawn(|| {
                at_once.wait();
                vaults.unlock(THE_OPERATOR, &the_passphrase()).unwrap();
            });
            let logging_in = scope.spawn(|| {
                at_once.wait();
                vaults
                    .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
                    .unwrap()
            });
            unlocking.join().unwrap();
            logging_in.join().unwrap()
        });

        // Then whichever won, the login was never told "open" without a key and a sealed token
        assert!(
            the_login_was_told_the_truth(&vaults, &retained),
            "the login was told {:?} with a key: {}, token sealed: {:?}",
            retained.state,
            retained.unlock_key.is_some(),
            the_stored_token(&vaults)
        );
    }

    #[test]
    fn discarding_a_users_waiting_credentials_leaves_nothing_for_the_vault_to_seal() {
        // Given a first login's token waiting in memory for a vault
        let dir = tempfile::tempdir().unwrap();
        let vaults = SessionVaults::new(dir.path());
        vaults
            .retain(THE_OPERATOR, a_github_record(THE_FIRST_TOKEN))
            .unwrap();

        // When the operator signs out without ever opening the vault
        vaults.discard_pending(THE_OPERATOR);

        // Then the token is gone from memory, and it no longer counts as a fresh login
        assert_eq!(
            (
                vaults.holds_pending(THE_OPERATOR),
                vaults.create(THE_OPERATOR, &the_passphrase()).err()
            ),
            (false, Some(VaultError::NoFreshLogin))
        );
    }

    #[test]
    fn a_retired_key_replayed_within_the_grace_window_after_logout_opens_nothing() {
        // Given a lineage that refreshed once and then signed out with the key it was handed
        let dir = tempfile::tempdir().unwrap();
        let (vaults, retired) = a_daemon_with_a_created_vault(dir.path());
        let successor = vaults.reopen(&retired).unwrap();
        vaults.forget(&successor).unwrap();

        // When the retired key is replayed moments later, well inside the grace window
        let answered = vaults.reopen(&retired).err();

        // Then the logout ended the lineage: the grace window does not bring it back
        assert_eq!(
            (answered, vaults.state(THE_OPERATOR)),
            (Some(VaultError::Locked), VaultState::Locked)
        );
    }

    /// A clock that moves only when the test moves it.
    struct AHandDrivenClock(Mutex<Instant>);

    impl AHandDrivenClock {
        fn new() -> Arc<Self> {
            Arc::new(Self(Mutex::new(Instant::now())))
        }

        fn advance(&self, by: Duration) {
            *self.0.lock().unwrap() += by;
        }

        fn as_clock(self: &Arc<Self>) -> Clock {
            let clock = Arc::clone(self);
            Arc::new(move || *clock.0.lock().unwrap())
        }
    }

    const A_TEN_MINUTE_LIFETIME: Duration = Duration::from_secs(600);

    /// A daemon whose pending sign-ins wait `lifetime`, reading the time from `clock`.
    fn a_daemon_on(
        dir: &Path,
        clock: &Arc<AHandDrivenClock>,
        lifetime: Option<Duration>,
    ) -> SessionVaults {
        SessionVaults::new(dir)
            .with_clock(clock.as_clock())
            .with_pending_lifetime(lifetime)
    }

    #[test]
    fn a_sign_in_past_its_lifetime_no_longer_allows_a_first_passphrase() {
        // Given a first sign-in whose token has waited longer than a pending sign-in may
        let dir = tempfile::tempdir().unwrap();
        let clock = AHandDrivenClock::new();
        let vaults = a_daemon_on(dir.path(), &clock, Some(A_TEN_MINUTE_LIFETIME));
        vaults
            .retain(THE_OPERATOR, a_github_record(THE_FIRST_TOKEN))
            .unwrap();
        clock.advance(A_TEN_MINUTE_LIFETIME + Duration::from_secs(1));

        // When the operator chooses a first passphrase
        let refused = vaults.create(THE_OPERATOR, &the_passphrase()).err();

        // Then only a new sign-in may, nothing waits in memory, and the vault is still uncreated
        assert_eq!(
            (
                refused,
                vaults.holds_pending(THE_OPERATOR),
                vaults.state(THE_OPERATOR)
            ),
            (
                Some(VaultError::NoFreshLogin),
                false,
                VaultState::Uninitialized
            )
        );
    }

    #[test]
    fn a_sign_in_within_its_lifetime_still_allows_a_first_passphrase() {
        // Given a first sign-in a moment short of its lifetime
        let dir = tempfile::tempdir().unwrap();
        let clock = AHandDrivenClock::new();
        let vaults = a_daemon_on(dir.path(), &clock, Some(A_TEN_MINUTE_LIFETIME));
        vaults
            .retain(THE_OPERATOR, a_github_record(THE_FIRST_TOKEN))
            .unwrap();
        clock.advance(A_TEN_MINUTE_LIFETIME - Duration::from_secs(1));

        // When the operator chooses a first passphrase
        vaults.create(THE_OPERATOR, &the_passphrase()).unwrap();

        // Then the waiting token is sealed
        assert_eq!(the_stored_token(&vaults), Some(THE_FIRST_TOKEN.to_string()));
    }

    #[test]
    fn an_expired_sign_ins_token_is_not_sealed_by_a_later_unlock() {
        // Given a locked vault after a restart, and a sign-in whose token then waited too long
        let dir = tempfile::tempdir().unwrap();
        a_daemon_with_a_created_vault(dir.path());
        let clock = AHandDrivenClock::new();
        let vaults = a_daemon_on(dir.path(), &clock, Some(A_TEN_MINUTE_LIFETIME));
        vaults
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .unwrap();
        clock.advance(A_TEN_MINUTE_LIFETIME + Duration::from_secs(1));

        // When the passphrase opens the vault
        vaults.unlock(THE_OPERATOR, &the_passphrase()).unwrap();

        // Then the expired token was dropped, not sealed: the vault still holds the first one
        assert_eq!(the_stored_token(&vaults), Some(THE_FIRST_TOKEN.to_string()));
    }

    #[test]
    fn a_reset_after_the_sign_in_expired_is_refused_and_the_vault_stays_locked() {
        // Given a locked vault after a restart, and a sign-in whose token then waited too long
        let dir = tempfile::tempdir().unwrap();
        a_daemon_with_a_created_vault(dir.path());
        let clock = AHandDrivenClock::new();
        let vaults = a_daemon_on(dir.path(), &clock, Some(A_TEN_MINUTE_LIFETIME));
        vaults
            .retain(THE_OPERATOR, a_github_record(A_LATER_TOKEN))
            .unwrap();
        clock.advance(A_TEN_MINUTE_LIFETIME + Duration::from_secs(1));

        // When a reset is asked for
        let refused = vaults
            .reset(THE_OPERATOR, &SecretString::new(A_NEW_PASSPHRASE))
            .err();

        // Then
        assert_eq!(
            (refused, vaults.state(THE_OPERATOR)),
            (Some(VaultError::NoFreshLogin), VaultState::Locked)
        );
    }

    #[test]
    fn with_no_lifetime_a_pending_sign_in_never_expires() {
        // Given a daemon whose pending sign-ins never expire, and a sign-in a year ago
        let dir = tempfile::tempdir().unwrap();
        let clock = AHandDrivenClock::new();
        let vaults = a_daemon_on(dir.path(), &clock, None);
        vaults
            .retain(THE_OPERATOR, a_github_record(THE_FIRST_TOKEN))
            .unwrap();
        clock.advance(Duration::from_secs(365 * 24 * 60 * 60));

        // When the operator chooses a first passphrase
        vaults.create(THE_OPERATOR, &the_passphrase()).unwrap();

        // Then the token is sealed
        assert_eq!(the_stored_token(&vaults), Some(THE_FIRST_TOKEN.to_string()));
    }

    #[test]
    fn a_sweep_removes_an_expired_sign_in_nobody_touched() {
        // Given a sign-in whose token nobody has touched since its lifetime ran out
        let dir = tempfile::tempdir().unwrap();
        let clock = AHandDrivenClock::new();
        let vaults = a_daemon_on(dir.path(), &clock, Some(A_TEN_MINUTE_LIFETIME));
        vaults
            .retain(THE_OPERATOR, a_github_record(THE_FIRST_TOKEN))
            .unwrap();
        clock.advance(A_TEN_MINUTE_LIFETIME + Duration::from_secs(1));

        // When the sweep runs, twice
        let swept = (vaults.expire_pending(), vaults.expire_pending());

        // Then the first sweep dropped it, and there was nothing left for the second
        assert_eq!(swept, (1, 0));
    }

    #[test]
    fn a_sweep_leaves_a_sign_in_within_its_lifetime() {
        // Given a sign-in still within its lifetime
        let dir = tempfile::tempdir().unwrap();
        let clock = AHandDrivenClock::new();
        let vaults = a_daemon_on(dir.path(), &clock, Some(A_TEN_MINUTE_LIFETIME));
        vaults
            .retain(THE_OPERATOR, a_github_record(THE_FIRST_TOKEN))
            .unwrap();
        clock.advance(A_TEN_MINUTE_LIFETIME / 2);

        // When the sweep runs
        let swept = vaults.expire_pending();

        // Then
        assert_eq!((swept, vaults.holds_pending(THE_OPERATOR)), (0, true));
    }
}
