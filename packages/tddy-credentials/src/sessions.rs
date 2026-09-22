//! The vaults open right now, one per signed-in user.
//!
//! A login is the only moment the daemon holds the credential a vault's login slot is derived
//! from, and the reads that need a stored secret (PR status, today) arrive later, carrying nothing
//! but a session token. So the handle a login opens is kept here, by subject, for the reads to find.
//!
//! **After a daemon restart** this registry is empty, and there is still no daemon-held key. What
//! refills it is the browser: a login hands each session lineage an [`UnlockKey`] to its own
//! unlock slot, and that lineage's next session refresh presents it — [`SessionVaults::reopen`]
//! opens the vault through the slot, registers it, and rotates the slot so the presented key opens
//! nothing afterwards. Between the restart and that refresh, a credential-backed read finds no
//! handle and reports itself unavailable until the refresh, not until a new login.
//!
//! TODO(keyring): an entry is not evicted when a session ends. Session tokens are stateless, so the
//! daemon learns of no ending; an entry lives until the daemon exits. A logout removes its
//! lineage's unlock slot, not the entry — other browsers of the same user may still be using it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::vault::{CredentialStore, SessionVault, UnlockKey, VaultError};

/// Every user's open [`SessionVault`], keyed by subject, over one `auth_storage` directory.
pub struct SessionVaults {
    auth_storage_dir: PathBuf,
    open: Mutex<HashMap<String, Arc<SessionVault>>>,
}

impl SessionVaults {
    /// No vault open yet; each subject's file lives at [`CredentialStore::path_in`] under
    /// `auth_storage_dir`.
    #[must_use]
    pub fn new(auth_storage_dir: impl Into<PathBuf>) -> Self {
        Self {
            auth_storage_dir: auth_storage_dir.into(),
            open: Mutex::new(HashMap::new()),
        }
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

    /// Open `subject`'s vault for a login that produced `ikm`, re-wrap it under that credential,
    /// and keep the handle for the session's reads.
    ///
    /// When the subject already has a vault open — they are signed in elsewhere — that handle is
    /// the one re-wrapped, which is what lets a **rotated** credential (a device login after a
    /// callback login, a re-approval) carry the vault over instead of locking it. Otherwise the
    /// file is opened from `ikm`, created only when absent; one sealed under anything else is
    /// [`VaultError::Locked`] and is left exactly as it was.
    pub fn unlock(&self, subject: &str, ikm: &[u8]) -> Result<Arc<SessionVault>, VaultError> {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let vault = match open.get(subject) {
            Some(vault) => Arc::clone(vault),
            None => Arc::new(CredentialStore::open_or_create(
                &self.path_for(subject),
                ikm,
                subject,
            )?),
        };
        vault.rewrap(ikm)?;
        open.insert(subject.to_string(), Arc::clone(&vault));
        Ok(vault)
    }

    /// Reopen a vault through the unlock slot `unlock` names — a session refresh, possibly the
    /// first contact since a daemon restart — and hand back the key the slot is rotated to.
    ///
    /// The presented key must open its slot even when the vault is already open here: holding a
    /// slot id is not possession of the key. A key that does not (a slot rotated since, logged out
    /// or evicted) is [`VaultError::Locked`], and nothing is registered or changed.
    pub fn reopen(&self, unlock: &UnlockKey) -> Result<UnlockKey, VaultError> {
        let subject = unlock.subject();
        let proven = CredentialStore::open_with_unlock_key(&self.path_for(subject), unlock)?;
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let vault = Arc::clone(
            open.entry(subject.to_string())
                .or_insert_with(|| Arc::new(proven)),
        );
        vault.rotate_unlock_slot(unlock.slot_id())
    }

    /// Remove the unlock slot `unlock` names — a logout. The key must still open its slot, so a
    /// slot id alone cannot remove somebody else's; a key that does not (the slot is already gone,
    /// or was rotated since) is [`VaultError::Locked`] and nothing is removed.
    pub fn forget(&self, unlock: &UnlockKey) -> Result<(), VaultError> {
        CredentialStore::open_with_unlock_key(&self.path_for(unlock.subject()), unlock)?
            .remove_unlock_slot(unlock.slot_id())
    }

    /// `subject`'s open vault, or `None` when nothing since this daemon started has opened it.
    #[must_use]
    pub fn get(&self, subject: &str) -> Option<Arc<SessionVault>> {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(subject)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{AccountId, CredentialRecord, ProviderId};

    const THE_OPERATOR: &str = "operator";

    fn a_record() -> CredentialRecord {
        CredentialRecord {
            provider: ProviderId::new("github"),
            account: AccountId::new(THE_OPERATOR),
            label: "Work".to_string(),
            secret: "gho_live".to_string(),
            metadata: Default::default(),
            updated_at: 1,
        }
    }

    #[test]
    fn a_rotated_credential_carries_an_open_vault_over_instead_of_locking_it() {
        // Given a signed-in operator whose vault holds a credential
        let dir = tempfile::tempdir().unwrap();
        let vaults = SessionVaults::new(dir.path());
        vaults
            .unlock(THE_OPERATOR, b"gho_from_the_callback_login")
            .unwrap()
            .put(a_record())
            .unwrap();

        // When they sign in again with a different credential while still signed in
        vaults
            .unlock(THE_OPERATOR, b"gho_from_a_device_login")
            .unwrap();

        // Then the vault now opens under the new credential, records intact
        let reopened = CredentialStore::open_or_create(
            &vaults.path_for(THE_OPERATOR),
            b"gho_from_a_device_login",
            THE_OPERATOR,
        )
        .and_then(|vault| vault.get(&ProviderId::new("github"), &AccountId::new(THE_OPERATOR)));
        assert_eq!(reopened, Ok(Some(a_record())));
    }

    #[test]
    fn a_refresh_after_a_restart_reopens_the_vault_through_its_unlock_slot() {
        // Given a login that handed its browser an unlock key, and then a daemon restart
        let dir = tempfile::tempdir().unwrap();
        let before_restart = SessionVaults::new(dir.path());
        let vault = before_restart
            .unlock(THE_OPERATOR, b"gho_the_login_credential")
            .unwrap();
        vault.put(a_record()).unwrap();
        let unlock = vault.add_unlock_slot().unwrap();
        drop((vault, before_restart));
        let after_restart = SessionVaults::new(dir.path());

        // When the browser refreshes its session, presenting the key
        after_restart.reopen(&unlock).unwrap();

        // Then the vault is open again, with no login
        assert_eq!(
            after_restart
                .get(THE_OPERATOR)
                .map(|vault| vault.get(&ProviderId::new("github"), &AccountId::new(THE_OPERATOR))),
            Some(Ok(Some(a_record())))
        );
    }

    #[test]
    fn a_rotated_unlock_key_is_the_only_one_that_opens_the_slot() {
        // Given a lineage whose key was rotated at a refresh
        let dir = tempfile::tempdir().unwrap();
        let vaults = SessionVaults::new(dir.path());
        let presented = vaults
            .unlock(THE_OPERATOR, b"gho_the_login_credential")
            .unwrap()
            .add_unlock_slot()
            .unwrap();
        let rotated = vaults.reopen(&presented).unwrap();

        // When each is presented to a fresh daemon
        let path = vaults.path_for(THE_OPERATOR);
        let opens = |key: &UnlockKey| CredentialStore::open_with_unlock_key(&path, key).is_ok();

        // Then only the rotated one opens it
        assert_eq!((opens(&presented), opens(&rotated)), (false, true));
    }

    #[test]
    fn holds_nothing_for_a_subject_no_login_has_opened() {
        let dir = tempfile::tempdir().unwrap();
        assert!(SessionVaults::new(dir.path()).get(THE_OPERATOR).is_none());
    }
}
