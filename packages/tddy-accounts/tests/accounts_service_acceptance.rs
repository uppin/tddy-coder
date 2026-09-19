//! What `accounts.AccountsService` must answer, over a store that is not the vault.
//!
//! The store is an in-memory fake rather than a real `SessionVault`: `#keyring` 3/9's vault can only
//! be obtained by sealing a file, so a test built on one would be exercising that crate's
//! cryptography and would fail for its `todo!()` rather than for anything this crate owes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use prost::Message;
use tddy_accounts::{build_accounts_entry, AccountStore, AccountsError, AccountsServiceImpl};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, FIRST_VERSION};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::accounts::{
    AccountsService, ListAccountsRequest, ListAccountsResponse, RemoveAccountRequest,
    RemoveAccountResponse, SetAccountLabelRequest, SetAccountLabelResponse,
};

const ADAS_SESSION: &str = "session-token-for-ada";

// ---------------------------------------------------------------------------------------------
// Builders

/// One credential, as the store hands it out. `secret` is deliberately recognisable: two tests look
/// for it in places it must never appear.
fn a_credential(provider: &str, account: &str, label: &str) -> CredentialRecord {
    let mut metadata = BTreeMap::new();
    metadata.insert("subject".to_string(), format!("{account}-at-{provider}"));

    CredentialRecord {
        provider: ProviderId::new(provider),
        account: AccountId::new(account),
        label: label.to_string(),
        secret: format!("shhh-{provider}-{account}"),
        metadata,
        updated_at: 1_726_700_000,
        version: FIRST_VERSION,
    }
}

/// A store that answers out of memory. Holds whatever records it was given, or one of the three
/// refusals, and mutates in place so a rename or a removal is observable in the next listing.
struct AnInMemoryAccountStore {
    session: String,
    records: Mutex<Vec<CredentialRecord>>,
    refusal: Option<AccountsError>,
}

fn an_account_store() -> AnInMemoryAccountStore {
    AnInMemoryAccountStore {
        session: ADAS_SESSION.to_string(),
        records: Mutex::new(Vec::new()),
        refusal: None,
    }
}

impl AnInMemoryAccountStore {
    fn holding(self, record: CredentialRecord) -> Self {
        self.records.lock().unwrap().push(record);
        self
    }

    fn locked(mut self) -> Self {
        self.refusal = Some(AccountsError::Locked);
        self
    }

    fn unreadable(mut self, reason: &str) -> Self {
        self.refusal = Some(AccountsError::Unavailable(reason.to_string()));
        self
    }

    /// The store's own order, which `ListAccounts` groups without re-sorting.
    fn sorted(&self) -> Vec<CredentialRecord> {
        let mut records = self.records.lock().unwrap().clone();
        records.sort_by(|left, right| {
            (left.provider.as_str(), left.account.as_str())
                .cmp(&(right.provider.as_str(), right.account.as_str()))
        });
        records
    }

    fn answer_for(&self, session_token: &str) -> Option<AccountsError> {
        self.refusal
            .clone()
            .or_else(|| (session_token != self.session).then_some(AccountsError::NoSuchSession))
    }
}

impl AccountStore for AnInMemoryAccountStore {
    fn list(&self, session_token: &str) -> Result<Vec<CredentialRecord>, AccountsError> {
        self.answer_for(session_token)
            .map_or_else(|| Ok(self.sorted()), Err)
    }

    fn set_label(
        &self,
        session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
        label: &str,
    ) -> Result<CredentialRecord, AccountsError> {
        self.answer_for(session_token).map_or_else(
            || {
                let mut records = self.records.lock().unwrap();
                let found = records
                    .iter_mut()
                    .find(|record| &record.provider == provider && &record.account == account)
                    .ok_or(AccountsError::Unavailable("no such account".to_string()))?;
                found.label = label.to_string();
                Ok(found.clone())
            },
            Err,
        )
    }

    fn remove(
        &self,
        session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
    ) -> Result<(), AccountsError> {
        self.answer_for(session_token).map_or_else(
            || {
                self.records
                    .lock()
                    .unwrap()
                    .retain(|record| &record.provider != provider || &record.account != account);
                Ok(())
            },
            Err,
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Asking the service

fn a_service(store: AnInMemoryAccountStore) -> AccountsServiceImpl<AnInMemoryAccountStore> {
    AccountsServiceImpl::new(Arc::new(store))
}

async fn listing_from(
    store: AnInMemoryAccountStore,
    session_token: &str,
) -> Result<ListAccountsResponse, Status> {
    a_service(store)
        .list_accounts(Request::new(ListAccountsRequest {
            session_token: session_token.to_string(),
        }))
        .await
        .map(tddy_rpc::Response::into_inner)
}

async fn rename_in(
    store: AnInMemoryAccountStore,
    provider: &str,
    account_id: &str,
    label: &str,
) -> Result<SetAccountLabelResponse, Status> {
    a_service(store)
        .set_account_label(Request::new(SetAccountLabelRequest {
            session_token: ADAS_SESSION.to_string(),
            provider: provider.to_string(),
            account_id: account_id.to_string(),
            label: label.to_string(),
        }))
        .await
        .map(tddy_rpc::Response::into_inner)
}

async fn removal_in(
    store: AnInMemoryAccountStore,
    provider: &str,
    account_id: &str,
) -> Result<RemoveAccountResponse, Status> {
    a_service(store)
        .remove_account(Request::new(RemoveAccountRequest {
            session_token: ADAS_SESSION.to_string(),
            provider: provider.to_string(),
            account_id: account_id.to_string(),
        }))
        .await
        .map(tddy_rpc::Response::into_inner)
}

/// Every provider name in the answer, and the account ids under each — the shape a screen draws.
fn grouping_of(response: &ListAccountsResponse) -> Vec<(String, Vec<String>)> {
    response
        .providers
        .iter()
        .map(|group| {
            (
                group.provider.clone(),
                group
                    .accounts
                    .iter()
                    .map(|account| account.account_id.clone())
                    .collect(),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Tests

#[tokio::test]
async fn listing_groups_the_stores_records_by_provider() {
    // Given
    let store = an_account_store()
        .holding(a_credential("github", "ada", "Ada at work"))
        .holding(a_credential("cloudflare", "zoe", "Zone admin"))
        .holding(a_credential("github", "bob", "Bob the bot"));

    // When
    let listing = listing_from(store, ADAS_SESSION).await;

    // Then
    assert_eq!(
        listing
            .map_err(|status| status.message)
            .map(|response| grouping_of(&response)),
        Ok(vec![
            ("cloudflare".to_string(), vec!["zoe".to_string()]),
            (
                "github".to_string(),
                vec!["ada".to_string(), "bob".to_string()]
            ),
        ])
    );
}

#[tokio::test]
async fn a_listing_carries_the_label_and_the_subject_and_not_the_secret() {
    // Given
    let store = an_account_store().holding(a_credential("github", "ada", "Ada at work"));

    // When
    let listing = listing_from(store, ADAS_SESSION).await;

    // Then
    assert_eq!(
        listing.map_err(|status| status.message).map(|response| {
            let account = response.providers[0].accounts[0].clone();
            (account.label, account.subject, account.has_secret)
        }),
        Ok(("Ada at work".to_string(), "ada-at-github".to_string(), true))
    );
}

#[tokio::test]
async fn no_byte_of_a_listing_on_the_wire_is_a_secret() {
    // Given
    let store = an_account_store()
        .holding(a_credential("github", "ada", "Ada at work"))
        .holding(a_credential("cloudflare", "zoe", "Zone admin"));

    // When
    let listing = listing_from(store, ADAS_SESSION).await;

    // Then
    assert_eq!(
        listing.map_err(|status| status.message).map(|response| {
            let wire = String::from_utf8_lossy(&response.encode_to_vec()).to_string();
            (
                wire.contains("shhh-github-ada"),
                wire.contains("shhh-cloudflare-zoe"),
            )
        }),
        Ok((false, false))
    );
}

#[tokio::test]
async fn a_store_that_opened_and_holds_nothing_is_not_reported_as_locked() {
    // Given
    let store = an_account_store();

    // When
    let listing = listing_from(store, ADAS_SESSION).await;

    // Then
    assert_eq!(
        listing
            .map_err(|status| status.message)
            .map(|response| (response.providers.len(), response.vault_locked)),
        Ok((0, false))
    );
}

#[tokio::test]
async fn a_locked_vault_is_reported_as_locked_rather_than_as_an_empty_one() {
    // Given
    let store = an_account_store()
        .holding(a_credential("github", "ada", "Ada at work"))
        .locked();

    // When
    let listing = listing_from(store, ADAS_SESSION).await;

    // Then
    assert_eq!(
        listing
            .map_err(|status| status.message)
            .map(|response| (response.providers.len(), response.vault_locked)),
        Ok((0, true))
    );
}

#[tokio::test]
async fn a_store_that_cannot_be_read_is_an_error_carrying_the_reason() {
    // Given
    let store = an_account_store().unreadable("the vault file is truncated");

    // When
    let listing = listing_from(store, ADAS_SESSION).await;

    // Then
    assert_eq!(
        listing
            .map(|response| response.vault_locked)
            .map_err(|status| (
                status.code,
                status.message.contains("the vault file is truncated")
            )),
        Err((Code::Internal, true))
    );
}

#[tokio::test]
async fn a_session_token_the_store_does_not_know_is_refused_rather_than_served_an_empty_listing() {
    // Given
    let store = an_account_store().holding(a_credential("github", "ada", "Ada at work"));

    // When
    let listing = listing_from(store, "a-token-nobody-minted").await;

    // Then
    assert_eq!(
        listing
            .map(|response| response.providers.len())
            .map_err(|status| status.code),
        Err(Code::Unauthenticated)
    );
}

#[tokio::test]
async fn renaming_an_account_changes_its_label_and_leaves_its_identity_alone() {
    // Given
    let store = an_account_store().holding(a_credential("github", "ada", "Ada at work"));

    // When
    let renamed = rename_in(store, "github", "ada", "Ada — personal").await;

    // Then
    assert_eq!(
        renamed.map_err(|status| status.message).map(|response| {
            let account = response.account.unwrap();
            (account.provider, account.account_id, account.label)
        }),
        Ok((
            "github".to_string(),
            "ada".to_string(),
            "Ada — personal".to_string()
        ))
    );
}

#[tokio::test]
async fn removing_an_account_leaves_every_other_one_in_place() {
    // Given
    let store = an_account_store()
        .holding(a_credential("github", "ada", "Ada at work"))
        .holding(a_credential("github", "bob", "Bob the bot"))
        .holding(a_credential("cloudflare", "zoe", "Zone admin"));

    // When
    let remaining = removal_in(store, "github", "bob").await;

    // Then
    assert_eq!(
        remaining.map_err(|status| status.message).map(|response| {
            grouping_of(&ListAccountsResponse {
                providers: response.providers,
                vault_locked: false,
            })
        }),
        Ok(vec![
            ("cloudflare".to_string(), vec!["zoe".to_string()]),
            ("github".to_string(), vec!["ada".to_string()]),
        ])
    );
}

#[test]
fn the_registry_entry_names_the_service_a_client_addresses() {
    // Given
    let service = a_service(an_account_store());

    // When
    let entry = build_accounts_entry(service);

    // Then
    assert_eq!(entry.name, "accounts.AccountsService");
}
