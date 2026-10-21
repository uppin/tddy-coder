//! What `ConnectProviderAccount` and `ListProviderAccountModels` must answer.
//!
//! A model-provider account is connected by pasting a key. The key is checked by asking the
//! endpoint for its models **before** anything is stored, so a typo is caught here rather than at
//! the first session that tries to use it. The store and the probe are in-memory fakes: the vault's
//! cryptography and the network are other crates' concerns.
//!
//! Feature: docs/ft/daemon/provider-accounts.md

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_accounts::{
    unclaimed_account_id, AccountStore, AccountsError, AccountsServiceImpl, ProviderAccount,
    ProviderAccountStore, ProviderEndpoints, ProviderModelProbe,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SecretString};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::accounts::{
    AccountSummary, AccountsService, ConnectProviderAccountRequest, ListAccountsRequest,
    ListProviderAccountModelsRequest,
};

const ADAS_SESSION: &str = "session-token-for-ada";
const A_META_KEY: &str = "meta-key-that-must-never-be-echoed";
const META_MODELS: [&str; 2] = ["muse-spark-1.3", "muse-wonder-1.0"];

// ---------------------------------------------------------------------------------------------
// Fakes

/// A vault that answers out of memory for Ada's session, or refuses as configured.
struct AnInMemoryVault {
    records: Mutex<Vec<CredentialRecord>>,
    refusal: Option<AccountsError>,
}

fn a_vault() -> AnInMemoryVault {
    AnInMemoryVault {
        records: Mutex::new(Vec::new()),
        refusal: None,
    }
}

impl AnInMemoryVault {
    fn locked(mut self) -> Self {
        self.refusal = Some(AccountsError::Locked);
        self
    }

    fn answer_for(&self, session_token: &str) -> Option<AccountsError> {
        self.refusal
            .clone()
            .or_else(|| (session_token != ADAS_SESSION).then_some(AccountsError::NoSuchSession))
    }

    fn stored(&self) -> Vec<CredentialRecord> {
        self.records.lock().unwrap().clone()
    }
}

impl AccountStore for AnInMemoryVault {
    fn list(&self, session_token: &str) -> Result<Vec<CredentialRecord>, AccountsError> {
        self.answer_for(session_token)
            .map_or_else(|| Ok(self.stored()), Err)
    }

    fn set_label(
        &self,
        _session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
        _label: &str,
    ) -> Result<CredentialRecord, AccountsError> {
        Err(AccountsError::NotFound {
            provider: provider.clone(),
            account: account.clone(),
        })
    }

    fn remove(
        &self,
        _session_token: &str,
        _provider: &ProviderId,
        _account: &AccountId,
    ) -> Result<(), AccountsError> {
        Ok(())
    }
}

impl ProviderAccountStore for AnInMemoryVault {
    fn put_provider_account(
        &self,
        session_token: &str,
        account: &ProviderAccount,
    ) -> Result<CredentialRecord, AccountsError> {
        if let Some(refusal) = self.answer_for(session_token) {
            return Err(refusal);
        }
        let mut records = self.records.lock().unwrap();
        let account = ProviderAccount {
            account: unclaimed_account_id(&account.account, &records),
            ..account.clone()
        };
        let record = account.to_record(1_760_000_000);
        records.push(record.clone());
        Ok(record)
    }

    fn provider_account(
        &self,
        session_token: &str,
        account: &AccountId,
    ) -> Result<ProviderAccount, AccountsError> {
        if let Some(refusal) = self.answer_for(session_token) {
            return Err(refusal);
        }
        self.stored()
            .iter()
            .find(|record| &record.account == account)
            .and_then(ProviderAccount::from_record)
            .ok_or_else(|| AccountsError::NotFound {
                provider: ProviderId::new("provider-account"),
                account: account.clone(),
            })
    }
}

/// A provider endpoint that lists `models` for `accepted_key` and refuses every other key with
/// `refusal` — the provider's own words.
struct AProviderEndpoint {
    accepted_key: String,
    models: Vec<String>,
    refusal: String,
    asked: Mutex<Vec<(ProviderEndpoints, String)>>,
}

fn a_meta_endpoint() -> AProviderEndpoint {
    AProviderEndpoint {
        accepted_key: A_META_KEY.to_string(),
        models: META_MODELS.iter().map(|m| (*m).to_string()).collect(),
        refusal: "401 Unauthorized: invalid API key".to_string(),
        asked: Mutex::new(Vec::new()),
    }
}

#[async_trait]
impl ProviderModelProbe for AProviderEndpoint {
    async fn list_models(
        &self,
        endpoints: &ProviderEndpoints,
        api_key: &SecretString,
    ) -> Result<Vec<String>, String> {
        self.asked
            .lock()
            .unwrap()
            .push((endpoints.clone(), api_key.expose().to_string()));
        if api_key.expose() == self.accepted_key {
            Ok(self.models.clone())
        } else {
            Err(self.refusal.clone())
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Asking the service

struct World {
    vault: Arc<AnInMemoryVault>,
    endpoint: Arc<AProviderEndpoint>,
    service: AccountsServiceImpl<AnInMemoryVault>,
}

fn a_world(vault: AnInMemoryVault) -> World {
    let vault = Arc::new(vault);
    let endpoint = Arc::new(a_meta_endpoint());
    let service = AccountsServiceImpl::new(Arc::clone(&vault)).with_provider_accounts(
        Arc::clone(&vault) as Arc<dyn ProviderAccountStore>,
        Arc::clone(&endpoint) as Arc<dyn ProviderModelProbe>,
    );
    World {
        vault,
        endpoint,
        service,
    }
}

fn a_connect(preset: &str, label: &str, api_key: &str) -> ConnectProviderAccountRequest {
    ConnectProviderAccountRequest {
        session_token: ADAS_SESSION.to_string(),
        preset: preset.to_string(),
        label: label.to_string(),
        api_key: api_key.to_string(),
        ..ConnectProviderAccountRequest::default()
    }
}

impl World {
    async fn connect(
        &self,
        request: ConnectProviderAccountRequest,
    ) -> Result<AccountSummary, Status> {
        self.service
            .connect_provider_account(Request::direct(request))
            .await
            .map(|response| response.into_inner().account.expect("an account"))
    }

    async fn listed(&self) -> Vec<(String, Vec<AccountSummary>)> {
        self.service
            .list_accounts(Request::direct(ListAccountsRequest {
                session_token: ADAS_SESSION.to_string(),
            }))
            .await
            .expect("listing")
            .into_inner()
            .providers
            .into_iter()
            .map(|group| (group.provider, group.accounts))
            .collect()
    }

    async fn models_of(&self, account_id: &str) -> Result<Vec<String>, Status> {
        self.service
            .list_provider_account_models(Request::direct(ListProviderAccountModelsRequest {
                session_token: ADAS_SESSION.to_string(),
                account_id: account_id.to_string(),
            }))
            .await
            .map(|response| response.into_inner().model_ids)
    }
}

// ---------------------------------------------------------------------------------------------
// Connecting

#[tokio::test]
async fn connecting_a_meta_account_stores_a_sealed_record_listed_under_meta() {
    // Given
    let world = a_world(a_vault());

    // When
    let connected = world
        .connect(a_connect("meta", "Meta hackathon", A_META_KEY))
        .await
        .expect("a working key connects");

    // Then
    let listed = world.listed().await;
    assert_eq!(listed.len(), 1);
    let (provider, accounts) = &listed[0];
    assert_eq!(provider, "meta");
    assert_eq!(accounts.len(), 1);
    let account = &accounts[0];
    assert_eq!(account.account_id, connected.account_id);
    assert_eq!(account.label, "Meta hackathon");
    assert_eq!(account.preset, "meta");
    assert_eq!(
        account.wire_protocols,
        vec!["anthropic-messages", "openai-responses", "openai-chat"]
    );
    assert!(account.has_secret);
    assert!(!format!("{listed:?}{connected:?}").contains(A_META_KEY));
}

#[tokio::test]
async fn a_key_the_provider_rejects_is_refused_and_nothing_is_stored() {
    // Given
    let world = a_world(a_vault());

    // When
    let refusal = world
        .connect(a_connect("meta", "Typo", "not-the-key"))
        .await
        .expect_err("a rejected key is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(refusal
        .message()
        .contains("401 Unauthorized: invalid API key"));
    assert!(world.vault.stored().is_empty());
}

#[tokio::test]
async fn a_preset_with_an_operator_supplied_url_requires_one() {
    // Given
    let world = a_world(a_vault());

    // When
    let refusal = world
        .connect(a_connect("modal", "My Modal deployment", A_META_KEY))
        .await
        .expect_err("modal without a URL is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(world.endpoint.asked.lock().unwrap().is_empty());
    assert!(world.vault.stored().is_empty());
}

#[tokio::test]
async fn a_fixed_preset_refuses_a_url_override() {
    // Given
    let world = a_world(a_vault());
    let request = ConnectProviderAccountRequest {
        openai_base_url: "https://evil.example/v1".to_string(),
        ..a_connect("meta", "Meta", A_META_KEY)
    };

    // When
    let refusal = world.connect(request).await.expect_err("override refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(world.endpoint.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_base_url_for_a_protocol_the_preset_does_not_serve_is_refused() {
    // Given
    let world = a_world(a_vault());
    let request = ConnectProviderAccountRequest {
        anthropic_base_url: "https://llm.example.com".to_string(),
        openai_base_url: "https://modal.example/v1".to_string(),
        ..a_connect("modal", "My Modal deployment", A_META_KEY)
    };

    // When
    let refusal = world
        .connect(request)
        .await
        .expect_err("unserved URL refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(refusal.message().contains("anthropic-messages"));
    assert!(world.endpoint.asked.lock().unwrap().is_empty());
    assert!(world.vault.stored().is_empty());
}

#[tokio::test]
async fn a_second_account_with_the_same_label_gets_its_own_id() {
    // Given
    let world = a_world(a_vault());
    world
        .connect(a_connect("meta", "Meta hackathon", A_META_KEY))
        .await
        .expect("the first connect succeeds");

    // When
    let second = world
        .connect(a_connect("meta", "Meta hackathon", A_META_KEY))
        .await
        .expect("the second connect succeeds");

    // Then
    assert_eq!(second.account_id, "meta-hackathon-2");
    assert_eq!(world.vault.stored().len(), 2);
}

#[tokio::test]
async fn a_locked_vault_refuses_the_connect() {
    // Given
    let world = a_world(a_vault().locked());

    // When
    let refusal = world
        .connect(a_connect("meta", "Meta", A_META_KEY))
        .await
        .expect_err("a locked vault refuses");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(refusal.message().contains("locked"));
}

#[tokio::test]
async fn a_daemon_that_wired_no_provider_accounts_refuses_to_connect_one() {
    // Given
    let service = AccountsServiceImpl::new(Arc::new(a_vault()));

    // When
    let refusal = service
        .connect_provider_account(Request::direct(a_connect("meta", "Meta", A_META_KEY)))
        .await
        .expect_err("unwired");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
}

// ---------------------------------------------------------------------------------------------
// Listing an account's models

#[tokio::test]
async fn listing_an_accounts_models_asks_its_endpoint_with_its_key() {
    // Given
    let world = a_world(a_vault());
    let connected = world
        .connect(a_connect("meta", "Meta", A_META_KEY))
        .await
        .expect("connected");
    world.endpoint.asked.lock().unwrap().clear();

    // When
    let models = world
        .models_of(&connected.account_id)
        .await
        .expect("models");

    // Then
    assert_eq!(models, META_MODELS.to_vec());
    let asked = world.endpoint.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    let (endpoints, key) = &asked[0];
    assert_eq!(
        endpoints.openai_chat.as_deref(),
        Some("https://api.meta.ai/v1")
    );
    assert_eq!(key, A_META_KEY);
}

#[tokio::test]
async fn listing_the_models_of_an_account_the_vault_does_not_hold_is_not_found() {
    // Given
    let world = a_world(a_vault());

    // When
    let refusal = world
        .models_of("no-such-account")
        .await
        .expect_err("unknown");

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
}
