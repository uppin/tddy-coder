//! The production [`AccountStore`]: the vaults a daemon holds, found by the session's subject.

use std::sync::Arc;

use tddy_credentials::{
    AccountId, CredentialRecord, ProviderId, SessionVault, SessionVaults, VaultError, VaultState,
};

use crate::store::{AccountStore, AccountsError};

/// Resolve a `session_token` to the subject whose vault it may open, or `None` when the token names
/// no live session.
///
/// Injected rather than implemented here: verifying a session token is the daemon's job, and doing
/// it in this crate would pull in the auth stack this crate is kept free of. The shape matches the
/// daemon's own session-token resolver, so the daemon can hand that in unchanged.
pub type SessionSubjectResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// An [`AccountStore`] over the [`SessionVaults`] a daemon keeps open, one per signed-in subject.
pub struct SessionVaultAccountStore {
    vaults: Arc<SessionVaults>,
    subject_of: SessionSubjectResolver,
}

impl SessionVaultAccountStore {
    #[must_use]
    pub fn new(vaults: Arc<SessionVaults>, subject_of: SessionSubjectResolver) -> Self {
        Self { vaults, subject_of }
    }

    /// The subject the token's session belongs to, and its vault, open on this daemon.
    ///
    /// A vault that is not open is never an empty listing: a [`VaultState::Locked`] one may hold
    /// accounts its passphrase would show, and an [`VaultState::Uninitialized`] one does not exist
    /// to be empty. Reading through [`SessionVaults::use_open`] counts as a use, so a person
    /// looking at their accounts keeps the vault open for another idle lifetime.
    fn vault_for(&self, session_token: &str) -> Result<(String, Arc<SessionVault>), AccountsError> {
        let subject = (self.subject_of)(session_token).ok_or(AccountsError::NoSuchSession)?;
        if let Some(refusal) = refusal_for_closed(self.vaults.state(&subject)) {
            return Err(refusal);
        }
        match self.vaults.use_open(&subject) {
            Some(vault) => Ok((subject, vault)),
            // Closed between the two look-ups — idle eviction, the last sign-out, or its file
            // replaced. Whatever it is now is the answer; it was open a moment ago, and only a
            // reopen racing this read would make it read open again.
            None => Err(
                refusal_for_closed(self.vaults.state(&subject)).unwrap_or_else(|| {
                    AccountsError::Unavailable(
                        "the credential vault closed and reopened while it was being read; \
                         try again"
                            .to_string(),
                    )
                }),
            ),
        }
    }
}

/// What a vault in `state` refuses with — `None` when it is open and can be read.
fn refusal_for_closed(state: VaultState) -> Option<AccountsError> {
    match state {
        VaultState::Open => None,
        VaultState::Locked => Some(AccountsError::Locked),
        VaultState::Uninitialized => Some(AccountsError::Uninitialized),
    }
}

impl AccountStore for SessionVaultAccountStore {
    fn list(&self, session_token: &str) -> Result<Vec<CredentialRecord>, AccountsError> {
        let (subject, vault) = self.vault_for(session_token)?;
        vault
            .list(None)
            .map_err(|error| refusal_of(&subject, error))
    }

    fn set_label(
        &self,
        session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
        label: &str,
    ) -> Result<CredentialRecord, AccountsError> {
        let (subject, vault) = self.vault_for(session_token)?;
        // TODO(keyring): the read and the write are two separately serialised vault operations, so
        // a write to this record landing between them is overwritten with the secret read here.
        // See docs/dev/todo/2026-10-04-keyring-accounts-rename-lost-update.md.
        let mut record = vault
            .get(provider, account)
            .map_err(|error| refusal_of(&subject, error))?
            .ok_or_else(|| AccountsError::NotFound {
                provider: provider.clone(),
                account: account.clone(),
            })?;
        record.label = label.to_string();
        vault
            .put(record.clone())
            .map_err(|error| refusal_of(&subject, error))?;
        Ok(record)
    }

    fn remove(
        &self,
        session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
    ) -> Result<(), AccountsError> {
        let (subject, vault) = self.vault_for(session_token)?;
        vault
            .remove(provider, account)
            .map_err(|error| refusal_of(&subject, error))
    }
}

/// What a person is told when the vault could not be read or written. Deliberately path-free.
const STORE_UNREADABLE: &str = "the credential store could not be read or written on this daemon";

/// `Locked` keeps its meaning. `Io` names server-side detail (file paths, OS errors), so the client
/// gets [`STORE_UNREADABLE`] and the log gets the full error with the subject it belongs to. Every
/// other variant's text is a fixed sentence written for the person — none of them names a path —
/// so it is passed on verbatim: it tells them, or the operator, which remedy applies.
fn refusal_of(subject: &str, error: VaultError) -> AccountsError {
    match error {
        VaultError::Locked => AccountsError::Locked,
        VaultError::Io(_) => {
            log::error!(
                target: "tddy_accounts",
                "credential store for {subject} could not be read or written: {error}"
            );
            AccountsError::Unavailable(STORE_UNREADABLE.to_string())
        }
        told @ (VaultError::FormatMismatch { .. }
        | VaultError::Uninitialized
        | VaultError::AlreadyInitialized
        | VaultError::NoFreshLogin
        | VaultError::AlreadyOpen
        | VaultError::TooManySetAside { .. }
        | VaultError::Crypto) => AccountsError::Unavailable(told.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use pretty_assertions::assert_eq;
    use tddy_credentials::{CredentialStore, SecretString};

    use super::*;

    const ADA: &str = "ada";
    const ADAS_SESSION: &str = "session-token-for-ada";
    const ADAS_PASSPHRASE: &str = "correct horse battery staple";

    fn a_credential(account: &str, label: &str) -> CredentialRecord {
        CredentialRecord {
            provider: ProviderId::new("github"),
            account: AccountId::new(account),
            label: label.to_string(),
            secret: SecretString::new(format!("shhh-{account}")),
            metadata: BTreeMap::new(),
            updated_at: 1_726_700_000,
        }
    }

    fn adas_passphrase() -> SecretString {
        SecretString::new(ADAS_PASSPHRASE)
    }

    /// Ada chose a passphrase on an earlier run of the daemon, and her vault holds `records`.
    fn ada_created_her_vault_holding(dir: &Path, records: Vec<CredentialRecord>) {
        let vault =
            CredentialStore::create(&CredentialStore::path_in(dir, ADA), &adas_passphrase(), ADA)
                .expect("Ada's vault is created");
        for record in records {
            vault.put(record).expect("the record is retained");
        }
    }

    /// A daemon's vault registry over `dir`, and the store whose resolver knows Ada's token.
    fn a_daemon_over(dir: &Path) -> (Arc<SessionVaults>, SessionVaultAccountStore) {
        let vaults = Arc::new(SessionVaults::new(dir));
        let subject_of: SessionSubjectResolver =
            Arc::new(|token: &str| (token == ADAS_SESSION).then(|| ADA.to_string()));
        let store = SessionVaultAccountStore::new(Arc::clone(&vaults), subject_of);
        (vaults, store)
    }

    /// A daemon on which Ada has unlocked her vault, holding `records`, with its open handle.
    fn ada_unlocked_her_vault_holding(
        records: Vec<CredentialRecord>,
    ) -> (
        tempfile::TempDir,
        Arc<SessionVault>,
        SessionVaultAccountStore,
    ) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        ada_created_her_vault_holding(dir.path(), records);
        let (vaults, store) = a_daemon_over(dir.path());
        vaults
            .unlock(ADA, &adas_passphrase())
            .expect("Ada's passphrase opens her vault");
        let vault = vaults.get(ADA).expect("Ada's vault is open");
        (dir, vault, store)
    }

    fn accounts_in(
        listing: Result<Vec<CredentialRecord>, AccountsError>,
    ) -> Result<Vec<String>, AccountsError> {
        listing.map(|records| {
            records
                .into_iter()
                .map(|record| record.account.as_str().to_string())
                .collect()
        })
    }

    #[test]
    fn lists_what_the_signed_in_subjects_open_vault_holds() {
        // Given
        let (_dir, _vault, store) = ada_unlocked_her_vault_holding(vec![
            a_credential("bob", "Bot"),
            a_credential("ada", "Work"),
        ]);

        // When
        let listing = store.list(ADAS_SESSION);

        // Then
        assert_eq!(
            accounts_in(listing),
            Ok(vec!["ada".to_string(), "bob".to_string()])
        );
    }

    #[test]
    fn a_token_no_session_owns_is_refused() {
        // Given
        let (_dir, _vault, store) =
            ada_unlocked_her_vault_holding(vec![a_credential("ada", "Work")]);

        // When
        let listing = store.list("a-token-nobody-minted");

        // Then
        assert_eq!(listing, Err(AccountsError::NoSuchSession));
    }

    #[test]
    fn a_subject_with_no_vault_file_is_uninitialized_rather_than_empty() {
        // Given a daemon on which Ada has never chosen a passphrase
        let dir = tempfile::tempdir().expect("a temporary directory");
        let (_vaults, store) = a_daemon_over(dir.path());

        // When
        let listing = store.list(ADAS_SESSION);

        // Then
        assert_eq!(listing, Err(AccountsError::Uninitialized));
    }

    #[test]
    fn a_vault_that_exists_but_is_not_open_on_this_daemon_is_locked_rather_than_empty() {
        // Given Ada's vault, created earlier, and a daemon that has not opened it since it started
        let dir = tempfile::tempdir().expect("a temporary directory");
        ada_created_her_vault_holding(dir.path(), vec![a_credential("ada", "Work")]);
        let (_vaults, store) = a_daemon_over(dir.path());

        // When
        let listing = store.list(ADAS_SESSION);

        // Then
        assert_eq!(listing, Err(AccountsError::Locked));
    }

    #[test]
    fn renaming_in_a_vault_that_is_not_open_is_refused_as_locked() {
        // Given
        let dir = tempfile::tempdir().expect("a temporary directory");
        ada_created_her_vault_holding(dir.path(), vec![a_credential("ada", "Work")]);
        let (_vaults, store) = a_daemon_over(dir.path());

        // When
        let renamed = store.set_label(
            ADAS_SESSION,
            &ProviderId::new("github"),
            &AccountId::new("ada"),
            "Personal",
        );

        // Then
        assert_eq!(renamed, Err(AccountsError::Locked));
    }

    #[test]
    fn removing_from_a_vault_that_does_not_exist_is_refused_as_uninitialized() {
        // Given
        let dir = tempfile::tempdir().expect("a temporary directory");
        let (_vaults, store) = a_daemon_over(dir.path());

        // When
        let removed = store.remove(
            ADAS_SESSION,
            &ProviderId::new("github"),
            &AccountId::new("ada"),
        );

        // Then
        assert_eq!(removed, Err(AccountsError::Uninitialized));
    }

    #[test]
    fn renaming_changes_the_label_and_keeps_the_secret() {
        // Given
        let (_dir, vault, store) =
            ada_unlocked_her_vault_holding(vec![a_credential("ada", "Work")]);

        // When
        store
            .set_label(
                ADAS_SESSION,
                &ProviderId::new("github"),
                &AccountId::new("ada"),
                "Personal",
            )
            .expect("the rename is retained");

        // Then
        assert_eq!(
            vault
                .get(&ProviderId::new("github"), &AccountId::new("ada"))
                .map(|record| record
                    .map(|record| (record.label, record.secret.expose().to_string()))),
            Ok(Some(("Personal".to_string(), "shhh-ada".to_string())))
        );
    }

    #[test]
    fn renaming_an_account_that_is_not_linked_is_refused_as_not_found() {
        // Given
        let (_dir, _vault, store) = ada_unlocked_her_vault_holding(vec![]);

        // When
        let renamed = store.set_label(
            ADAS_SESSION,
            &ProviderId::new("github"),
            &AccountId::new("nobody"),
            "Ghost",
        );

        // Then
        assert_eq!(
            renamed,
            Err(AccountsError::NotFound {
                provider: ProviderId::new("github"),
                account: AccountId::new("nobody"),
            })
        );
    }

    #[test]
    fn removing_forgets_exactly_that_record() {
        // Given
        let (_dir, vault, store) = ada_unlocked_her_vault_holding(vec![
            a_credential("ada", "Work"),
            a_credential("bob", "Bot"),
        ]);

        // When
        store
            .remove(
                ADAS_SESSION,
                &ProviderId::new("github"),
                &AccountId::new("bob"),
            )
            .expect("the removal is retained");

        // Then
        assert_eq!(
            accounts_in(vault.list(None).map_err(|error| refusal_of(ADA, error))),
            Ok(vec!["ada".to_string()])
        );
    }

    #[test]
    fn a_locked_vault_stays_locked() {
        // Given
        let failure = VaultError::Locked;

        // When
        let refusal = refusal_of(ADA, failure);

        // Then
        assert_eq!(refusal, AccountsError::Locked);
    }

    #[test]
    fn an_io_failure_reaches_the_client_without_the_path_it_names() {
        // Given
        let failure = VaultError::Io(
            "/var/lib/tddy/auth/credentials-ada.vault: permission denied".to_string(),
        );

        // When
        let refusal = refusal_of(ADA, failure);

        // Then
        assert_eq!(
            refusal,
            AccountsError::Unavailable(STORE_UNREADABLE.to_string())
        );
    }

    #[test]
    fn a_failure_meant_for_the_person_keeps_its_reason() {
        // Given
        let failure = VaultError::TooManySetAside { kept: 5 };
        let reason = failure.to_string();

        // When
        let refusal = refusal_of(ADA, failure);

        // Then
        assert_eq!(refusal, AccountsError::Unavailable(reason));
    }
}
