//! The daemon's [`CoderCredentialSource`]: a Coder account's credentials, read from the vault
//! through [`CoderAccounts::credentials_for`].
//!
//! The one place `tddy-accounts`' account and `tddy-remote-shell`'s are converted: the listing
//! and the shell take the latter, the vault holds the former.

use std::sync::Arc;

use tddy_accounts::{AccountsError, CoderAccount, CoderAccountError, CoderAccounts};
use tddy_credentials::{AccountId, VaultState};
use tddy_host_service::coder_derived_hosts::{CoderAccountRef, CoderAccountSource};
use tddy_host_service::coder_workspaces::{
    CoderCredentialError, CoderCredentialSource, CoderCredentials,
};

/// [`CoderCredentialSource`] over the daemon's Coder accounts.
pub struct VaultCoderCredentialSource {
    accounts: Arc<CoderAccounts>,
}

impl VaultCoderCredentialSource {
    #[must_use]
    pub fn new(accounts: Arc<CoderAccounts>) -> Self {
        Self { accounts }
    }
}

/// [`CoderAccountSource`] over the daemon's vaults: a subject's Coder accounts, with their
/// labels, as the derived-hosts listing asks for them.
pub struct VaultCoderAccountSource {
    vaults: Arc<tddy_credentials::SessionVaults>,
}

impl VaultCoderAccountSource {
    #[must_use]
    pub fn new(vaults: Arc<tddy_credentials::SessionVaults>) -> Self {
        Self { vaults }
    }
}

impl CoderAccountSource for VaultCoderAccountSource {
    fn accounts_of(&self, subject: &str) -> Result<Vec<CoderAccountRef>, CoderCredentialError> {
        match self.vaults.state(subject) {
            VaultState::Locked => return Err(CoderCredentialError::VaultLocked),
            VaultState::Uninitialized => {
                return Err(CoderCredentialError::Unavailable(
                    "no credential vault exists for this account's owner on this daemon yet"
                        .to_string(),
                ))
            }
            VaultState::Open => {}
        }
        let vault = self.vaults.use_open(subject).ok_or_else(|| {
            CoderCredentialError::Unavailable(
                "the credential vault closed while it was being read; try again".to_string(),
            )
        })?;
        let records = vault
            .list(None)
            .map_err(|error| CoderCredentialError::Unavailable(error.to_string()))?;
        Ok(records
            .iter()
            .filter_map(CoderAccount::from_record)
            .map(|account| CoderAccountRef::new(account.account_id.to_string(), account.label))
            .collect())
    }
}

/// The daemon's Coder credential source over `vaults`, resolved as the user a session token
/// names: what a session host reads a Coder target's account through.
#[must_use]
pub fn coder_credential_source(
    vaults: Arc<tddy_credentials::SessionVaults>,
    user_resolver: tddy_daemon_kernel::SessionUserResolver,
) -> Arc<dyn CoderCredentialSource> {
    Arc::new(VaultCoderCredentialSource::new(Arc::new(
        CoderAccounts::new(
            vaults,
            user_resolver,
            Arc::new(
                crate::coder_deployment_probe::HttpCoderDeploymentProbe::new(reqwest::Client::new()),
            ),
        ),
    )))
}

/// The shells the Worktree tab's actions reach a target session's worktree through, resolved as a
/// session start resolves them: a Coder target through the daemon's `vaults`, or, for a daemon
/// started without Coder accounts, refused while an ssh target stays reachable.
#[must_use]
pub fn worktree_target_shells(
    vaults: Option<Arc<tddy_credentials::SessionVaults>>,
    user_resolver: tddy_daemon_kernel::SessionUserResolver,
) -> Arc<dyn tddy_worktree_service::target_worktrees::TargetShells> {
    let credentials: Arc<dyn CoderCredentialSource> = match vaults {
        Some(vaults) => coder_credential_source(vaults, user_resolver),
        None => Arc::new(tddy_agent_launch::coder_target::NoCoderAccounts),
    };
    Arc::new(tddy_agent_launch::target_shells::SessionTargetShells::new(
        credentials,
    ))
}

/// Wire a host service for Coder: its accounts read from `vaults` as the user a session token
/// resolves to, and its listings asked of each account's deployment over HTTP. Kept here, with
/// the credential source it installs, rather than in the host mounting that calls it.
pub fn wire_coder(
    host_service: tddy_host_service::HostServiceImpl,
    vaults: Arc<tddy_credentials::SessionVaults>,
    user_resolver: tddy_daemon_kernel::SessionUserResolver,
) -> tddy_host_service::HostServiceImpl {
    let coder_accounts = Arc::new(CoderAccounts::new(
        Arc::clone(&vaults),
        user_resolver,
        Arc::new(
            crate::coder_deployment_probe::HttpCoderDeploymentProbe::new(reqwest::Client::new()),
        ),
    ));
    host_service
        .with_coder_accounts(Arc::new(VaultCoderAccountSource::new(vaults)))
        .with_coder_credentials(Arc::new(VaultCoderCredentialSource::new(coder_accounts)))
        .with_coder_workspaces(Arc::new(
            crate::coder_workspace_source::HttpCoderWorkspaceSource::new(reqwest::Client::new()),
        ))
}

impl CoderCredentialSource for VaultCoderCredentialSource {
    fn credentials_for(
        &self,
        subject: &str,
        account_id: &str,
    ) -> Result<CoderCredentials, CoderCredentialError> {
        let held = self
            .accounts
            .credentials_for(subject, &AccountId::new(account_id))
            .map_err(credential_error_of)?;
        Ok(CoderCredentials::new(held.url.as_str(), held.token))
    }
}

/// An account read's refusal in the listing's words: a locked vault says so, an account the
/// vault does not hold names itself, and anything else is this daemon failing to read it.
fn credential_error_of(error: CoderAccountError) -> CoderCredentialError {
    match error {
        CoderAccountError::Vault(AccountsError::Locked) => CoderCredentialError::VaultLocked,
        CoderAccountError::NotFound(account_id) => {
            CoderCredentialError::NoSuchAccount(account_id.to_string())
        }
        CoderAccountError::Vault(AccountsError::NotFound { account, .. }) => {
            CoderCredentialError::NoSuchAccount(account.to_string())
        }
        CoderAccountError::Vault(AccountsError::Unavailable(reason)) => {
            CoderCredentialError::Unavailable(reason)
        }
        CoderAccountError::Vault(AccountsError::Uninitialized) => {
            CoderCredentialError::Unavailable(
                "no credential vault exists for this account's owner on this daemon yet"
                    .to_string(),
            )
        }
        CoderAccountError::Vault(AccountsError::NoSuchSession) => {
            CoderCredentialError::Unavailable(
                "the session names no signed-in user on this daemon".to_string(),
            )
        }
        CoderAccountError::InvalidUrl(url) => CoderCredentialError::Unavailable(format!(
            "the stored deployment URL is unusable: {url}"
        )),
        CoderAccountError::MissingToken => {
            CoderCredentialError::Unavailable("the stored account holds no token".to_string())
        }
        CoderAccountError::Rejected(reason) | CoderAccountError::Unreachable(reason) => {
            CoderCredentialError::Unavailable(reason)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn a_locked_vault_is_a_locked_vault() {
        // Given / When
        let mapped = credential_error_of(CoderAccountError::Vault(AccountsError::Locked));

        // Then
        assert_eq!(mapped, CoderCredentialError::VaultLocked);
    }

    #[test]
    fn an_account_the_vault_does_not_hold_names_itself() {
        // Given / When
        let mapped = credential_error_of(CoderAccountError::NotFound(AccountId::new(
            "coder-3f1c9a52@coder.example.com",
        )));

        // Then
        assert_eq!(
            mapped,
            CoderCredentialError::NoSuchAccount("coder-3f1c9a52@coder.example.com".to_string())
        );
    }

    const ADA: &str = "ada";
    const ADAS_PASSPHRASE: &str = "correct horse battery staple";

    fn adas_coder_record() -> tddy_credentials::CredentialRecord {
        let account = CoderAccount::for_user(
            tddy_accounts::CoderDeploymentUrl::parse("https://coder.example.com")
                .expect("a deployment URL"),
            &tddy_accounts::CoderUser {
                id: "3f1c9a52".to_string(),
                username: "ada".to_string(),
            },
            "Work",
        );
        account.into_record(
            tddy_credentials::SecretString::new("adas-coder-session-token"),
            1_760_000_000,
        )
    }

    fn a_vault_holding_adas_coder_account(
        unlock: bool,
    ) -> (tempfile::TempDir, VaultCoderAccountSource) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let vault = tddy_credentials::CredentialStore::create(
            &tddy_credentials::CredentialStore::path_in(dir.path(), ADA),
            &tddy_credentials::SecretString::new(ADAS_PASSPHRASE),
            ADA,
        )
        .expect("Ada's vault is created");
        vault
            .put(adas_coder_record())
            .expect("the record is retained");
        drop(vault);
        let vaults = Arc::new(tddy_credentials::SessionVaults::new(dir.path()));
        if unlock {
            vaults
                .unlock(ADA, &tddy_credentials::SecretString::new(ADAS_PASSPHRASE))
                .expect("the passphrase opens the vault");
        }
        let source = VaultCoderAccountSource::new(vaults);
        (dir, source)
    }

    #[test]
    fn the_account_source_lists_the_subjects_coder_accounts_with_their_labels() {
        // Given Ada's open vault holding her Coder account
        let (_dir, source) = a_vault_holding_adas_coder_account(true);

        // When
        let accounts = source.accounts_of(ADA);

        // Then
        assert_eq!(
            accounts,
            Ok(vec![CoderAccountRef::new(
                "coder-3f1c9a52@coder.example.com",
                "Work"
            )])
        );
    }

    #[test]
    fn the_account_source_refuses_a_locked_vault_as_locked() {
        // Given Ada's vault, sealed and not unlocked
        let (_dir, source) = a_vault_holding_adas_coder_account(false);

        // When
        let accounts = source.accounts_of(ADA);

        // Then
        assert_eq!(accounts, Err(CoderCredentialError::VaultLocked));
    }

    #[test]
    fn any_other_refusal_is_this_daemon_failing_to_read_the_account() {
        // Given / When
        let mapped = credential_error_of(CoderAccountError::Vault(AccountsError::Unavailable(
            "the vault is on a disk that went away".to_string(),
        )));

        // Then
        assert!(matches!(mapped, CoderCredentialError::Unavailable(_)));
    }
}
