//! The daemon's [`CoderCredentialSource`]: a Coder account's credentials, read from the vault
//! through [`CoderAccounts::credentials_for`].
//!
//! The one place `tddy-accounts`' account and `tddy-remote-shell`'s are converted: the listing
//! and the shell take the latter, the vault holds the former.

use std::sync::Arc;

use tddy_accounts::{AccountsError, CoderAccountError, CoderAccounts};
use tddy_credentials::AccountId;
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

/// Wire a host service for Coder: its accounts read from `vaults` as the user a session token
/// resolves to, and its listings asked of each account's deployment over HTTP. Kept here, with
/// the credential source it installs, rather than in the host mounting that calls it.
pub fn wire_coder(
    host_service: tddy_host_service::HostServiceImpl,
    vaults: Arc<tddy_credentials::SessionVaults>,
    user_resolver: tddy_daemon_kernel::SessionUserResolver,
) -> tddy_host_service::HostServiceImpl {
    let coder_accounts = Arc::new(CoderAccounts::new(
        vaults,
        user_resolver,
        Arc::new(
            crate::coder_deployment_probe::HttpCoderDeploymentProbe::new(reqwest::Client::new()),
        ),
    ));
    host_service
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
