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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::record::CredentialRecord;
use crate::secret::SecretString;
use crate::vault::{CredentialStore, SessionVault, UnlockKey, VaultError};

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
    /// Credentials logins produced while their vault was closed, by subject.
    pending: Mutex<HashMap<String, Vec<CredentialRecord>>>,
    /// The latest rotation of each `(subject, slot id)`, for the grace window. Also what serialises
    /// refreshes: a rotation and the answer to a refresh racing it happen under this lock.
    rotations: Mutex<HashMap<(String, String), Rotation>>,
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
            rotations: Mutex::new(HashMap::new()),
        }
    }

    /// Answer a retired unlock key for `grace` after its rotation rather than [`ROTATION_GRACE`].
    #[must_use]
    pub fn with_rotation_grace(mut self, grace: Duration) -> Self {
        self.rotation_grace = grace;
        self
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
        held(&self.pending)
            .get(subject)
            .is_some_and(|records| !records.is_empty())
    }

    /// Retain a credential a login produced for `subject`.
    ///
    /// With the vault open, `record` is sealed into it and the lineage is handed an unlock slot.
    /// With it closed, `record` is kept **in memory only**, never written in plaintext, and sealed
    /// when [`Self::unlock`], [`Self::create`] or [`Self::reset`] next opens the vault; the state
    /// says which of those the user is asked for. A failed write is an `Err` — it fails the login.
    pub fn retain(&self, subject: &str, record: CredentialRecord) -> Result<Retained, VaultError> {
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
        self.admit(subject, vault)
    }

    /// Create `subject`'s vault under a first passphrase, seal what was pending, and hand the
    /// lineage an unlock slot. A vault that already exists is [`VaultError::AlreadyInitialized`].
    pub fn create(
        &self,
        subject: &str,
        passphrase: &SecretString,
    ) -> Result<UnlockKey, VaultError> {
        let vault = CredentialStore::create(&self.path_for(subject), passphrase, subject)?;
        self.admit(subject, vault)
    }

    /// Set `subject`'s vault aside (renamed, never deleted) and create a fresh one under
    /// `new_passphrase`, sealing what was pending. Every unlock key to the old vault opens nothing.
    pub fn reset(&self, subject: &str, new_passphrase: &SecretString) -> Result<Reset, VaultError> {
        let (vault, set_aside) =
            CredentialStore::reset(&self.path_for(subject), new_passphrase, subject)?;
        held(&self.rotations).retain(|(rotated, _), _| rotated != subject);
        let unlock_key = self.admit(subject, vault)?;
        Ok(Reset {
            set_aside,
            unlock_key,
        })
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
                rotations.retain(|_, rotation| rotation.at.elapsed() <= grace);
                rotations.insert(
                    slot,
                    Rotation {
                        retired: unlock.duplicate(),
                        successor: successor.duplicate(),
                        at: Instant::now(),
                    },
                );
                self.register(subject, proven);
                Ok(successor)
            }
            Err(VaultError::Locked) => {
                let rotation = rotations
                    .get(&slot)
                    .filter(|rotation| rotation.at.elapsed() <= self.rotation_grace)
                    .filter(|rotation| rotation.retired.is_the_same_key_as(unlock))
                    .ok_or(VaultError::Locked)?;
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

    /// Seal what was waiting for `subject`'s newly opened `vault`, hand the lineage that opened it
    /// an unlock slot, and keep the handle.
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
        let records = pending.entry(subject.to_string()).or_default();
        records
            .retain(|held| (&held.provider, &held.account) != (&record.provider, &record.account));
        records.push(record);
    }

    /// Seal every credential waiting for `subject` into `vault`. What fails to seal is kept
    /// waiting, and the failure reported — nothing held in memory is dropped by a failed write.
    fn seal_pending(&self, subject: &str, vault: &SessionVault) -> Result<(), VaultError> {
        let mut waiting = held(&self.pending).remove(subject).unwrap_or_default();
        while let Some(record) = waiting.first() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{AccountId, ProviderId};

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
        let vaults = SessionVaults {
            rotation_grace: Duration::ZERO,
            ..vaults
        };
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
        // Given a lineage holding a key to a vault that is then reset
        let dir = tempfile::tempdir().unwrap();
        let (vaults, old_key) = a_daemon_with_a_created_vault(dir.path());
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
}
