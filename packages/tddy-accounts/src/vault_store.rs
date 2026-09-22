//! The production [`AccountStore`]: the vaults a login opened, found by the session's subject.

use std::sync::Arc;

use tddy_credentials::{
    AccountId, CredentialRecord, ProviderId, SessionVault, SessionVaults, VaultError,
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

    /// The open vault the token's session may read.
    ///
    /// A signed-in subject whose vault is not open *here* — the daemon restarted and the browser
    /// has not refreshed its session yet — is [`AccountsError::Unavailable`], naming why. Not
    /// `Locked`: nothing says the key would fail, and `Locked` tells the person to re-link. Not an
    /// empty listing either: the vault may hold accounts this daemon simply cannot read yet.
    fn vault_for(&self, session_token: &str) -> Result<Arc<SessionVault>, AccountsError> {
        let subject = (self.subject_of)(session_token).ok_or(AccountsError::NoSuchSession)?;
        self.vaults.get(&subject).ok_or_else(|| {
            AccountsError::Unavailable(format!(
                "the credential store for {subject} is not open on this daemon; \
                 it opens when you sign in, or when your session next refreshes"
            ))
        })
    }
}

impl AccountStore for SessionVaultAccountStore {
    fn list(&self, session_token: &str) -> Result<Vec<CredentialRecord>, AccountsError> {
        self.vault_for(session_token)?
            .list(None)
            .map_err(refusal_of)
    }

    fn set_label(
        &self,
        session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
        label: &str,
    ) -> Result<CredentialRecord, AccountsError> {
        let vault = self.vault_for(session_token)?;
        // TODO(keyring): the read and the write are two vault operations, each serialised on its
        // own. A write to the same record landing between them (a link flow refreshing the secret)
        // is overwritten with the secret read here. `SessionVault` offers no read-modify-write.
        let mut record = vault
            .get(provider, account)
            .map_err(refusal_of)?
            .ok_or_else(|| {
                AccountsError::Unavailable(format!(
                    "no {provider} account {account} is linked, so there is nothing to rename"
                ))
            })?;
        record.label = label.to_string();
        vault.put(record.clone()).map_err(refusal_of)?;
        Ok(record)
    }

    fn remove(
        &self,
        session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
    ) -> Result<(), AccountsError> {
        self.vault_for(session_token)?
            .remove(provider, account)
            .map_err(refusal_of)
    }
}

/// `Locked` keeps its meaning; every other vault failure is unavailable, with its reason intact.
fn refusal_of(error: VaultError) -> AccountsError {
    match error {
        VaultError::Locked => AccountsError::Locked,
        other => AccountsError::Unavailable(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;

    use super::*;

    const ADA: &str = "ada";
    const ADAS_SESSION: &str = "session-token-for-ada";
    const ADAS_LOGIN_CREDENTIAL: &[u8] = b"gho_adas_login_credential";

    fn a_credential(account: &str, label: &str) -> CredentialRecord {
        CredentialRecord {
            provider: ProviderId::new("github"),
            account: AccountId::new(account),
            label: label.to_string(),
            secret: format!("shhh-{account}"),
            metadata: BTreeMap::new(),
            updated_at: 1_726_700_000,
        }
    }

    /// A daemon's vault registry over a fresh directory, and the resolver that knows Ada's token.
    fn a_daemon_where_ada_can_sign_in() -> (
        tempfile::TempDir,
        Arc<SessionVaults>,
        SessionVaultAccountStore,
    ) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let vaults = Arc::new(SessionVaults::new(dir.path()));
        let subject_of: SessionSubjectResolver =
            Arc::new(|token: &str| (token == ADAS_SESSION).then(|| ADA.to_string()));
        let store = SessionVaultAccountStore::new(Arc::clone(&vaults), subject_of);
        (dir, vaults, store)
    }

    fn signed_in_holding(
        vaults: &SessionVaults,
        records: Vec<CredentialRecord>,
    ) -> Arc<SessionVault> {
        let vault = vaults
            .unlock(ADA, ADAS_LOGIN_CREDENTIAL)
            .expect("Ada's vault opens at sign-in");
        for record in records {
            vault.put(record).expect("the record is retained");
        }
        vault
    }

    #[test]
    fn lists_what_the_signed_in_subjects_vault_holds() {
        // Given
        let (_dir, vaults, store) = a_daemon_where_ada_can_sign_in();
        signed_in_holding(
            &vaults,
            vec![a_credential("bob", "Bot"), a_credential("ada", "Work")],
        );

        // When
        let listing = store.list(ADAS_SESSION);

        // Then
        assert_eq!(
            listing.map(|records| records
                .into_iter()
                .map(|record| record.account.as_str().to_string())
                .collect::<Vec<_>>()),
            Ok(vec!["ada".to_string(), "bob".to_string()])
        );
    }

    #[test]
    fn a_token_no_session_owns_is_refused() {
        // Given
        let (_dir, vaults, store) = a_daemon_where_ada_can_sign_in();
        signed_in_holding(&vaults, vec![a_credential("ada", "Work")]);

        // When
        let listing = store.list("a-token-nobody-minted");

        // Then
        assert_eq!(listing, Err(AccountsError::NoSuchSession));
    }

    #[test]
    fn a_signed_in_subject_whose_vault_is_not_open_here_is_unavailable_rather_than_empty() {
        // Given a daemon that has not opened Ada's vault since it started
        let (_dir, _vaults, store) = a_daemon_where_ada_can_sign_in();

        // When
        let listing = store.list(ADAS_SESSION);

        // Then
        assert_eq!(
            listing.map_err(|refusal| matches!(refusal, AccountsError::Unavailable(_))),
            Err(true)
        );
    }

    #[test]
    fn renaming_changes_the_label_and_keeps_the_secret() {
        // Given
        let (_dir, vaults, store) = a_daemon_where_ada_can_sign_in();
        let vault = signed_in_holding(&vaults, vec![a_credential("ada", "Work")]);

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
                .map(|record| record.map(|record| (record.label, record.secret))),
            Ok(Some(("Personal".to_string(), "shhh-ada".to_string())))
        );
    }

    #[test]
    fn renaming_an_account_that_is_not_linked_is_refused() {
        // Given
        let (_dir, vaults, store) = a_daemon_where_ada_can_sign_in();
        signed_in_holding(&vaults, vec![]);

        // When
        let renamed = store.set_label(
            ADAS_SESSION,
            &ProviderId::new("github"),
            &AccountId::new("nobody"),
            "Ghost",
        );

        // Then
        assert_eq!(
            renamed.map_err(|refusal| matches!(refusal, AccountsError::Unavailable(_))),
            Err(true)
        );
    }

    #[test]
    fn removing_forgets_exactly_that_record() {
        // Given
        let (_dir, vaults, store) = a_daemon_where_ada_can_sign_in();
        let vault = signed_in_holding(
            &vaults,
            vec![a_credential("ada", "Work"), a_credential("bob", "Bot")],
        );

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
            vault.list(None).map(|records| records
                .into_iter()
                .map(|record| record.account.as_str().to_string())
                .collect::<Vec<_>>()),
            Ok(vec!["ada".to_string()])
        );
    }

    #[test]
    fn a_locked_vault_stays_locked_and_other_failures_keep_their_reason() {
        assert_eq!(
            (
                refusal_of(VaultError::Locked),
                refusal_of(VaultError::Io("disk full".to_string()))
            ),
            (
                AccountsError::Locked,
                AccountsError::Unavailable("disk full".to_string())
            )
        );
    }
}
