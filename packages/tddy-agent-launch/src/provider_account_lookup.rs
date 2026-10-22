//! Finding a provider account in what a vault holds, by its id or by its label.

use tddy_accounts::{AccountsError, ProviderAccount};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId};

use crate::session_acting_identity::SessionIdentityRefusal;

/// The provider account `id_or_label` names among `held`: the account with that id, else the one
/// account carrying that label. Two accounts sharing the label are refused rather than one being
/// picked; none is `NotFound`.
pub(crate) fn find_provider_account(
    held: &[CredentialRecord],
    id_or_label: &str,
) -> Result<ProviderAccount, SessionIdentityRefusal> {
    let accounts: Vec<ProviderAccount> = held
        .iter()
        .filter_map(ProviderAccount::from_record)
        .collect();
    if let Some(account) = accounts.iter().find(|a| a.account.as_str() == id_or_label) {
        return Ok(account.clone());
    }
    let mut labelled = accounts.into_iter().filter(|a| a.label == id_or_label);
    match (labelled.next(), labelled.next()) {
        (Some(account), None) => Ok(account),
        (Some(_), Some(_)) => Err(SessionIdentityRefusal::AmbiguousLabel(
            id_or_label.to_string(),
        )),
        (None, _) => Err(SessionIdentityRefusal::Vault(AccountsError::NotFound {
            provider: ProviderId::new("provider"),
            account: AccountId::new(id_or_label),
        })),
    }
}
