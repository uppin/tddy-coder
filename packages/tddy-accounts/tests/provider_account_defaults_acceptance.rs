//! What `SetProviderAccountDefaults` does to a person's provider accounts, and how a default is
//! found again.
//!
//! Feature: docs/ft/daemon/provider-accounts.md (A person's default per harness)

use std::sync::{Arc, Mutex};

use tddy_accounts::{
    default_account_for, defaults_of, endpoints_for, preset, AccountStore, AccountsError,
    AccountsServiceImpl, ProviderAccount, ProviderAccountDefaults, RouteSupport, META_DEFAULT_FOR,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SecretString};
use tddy_rpc::{Code, Request};
use tddy_service::proto::accounts::{
    AccountsService, ListAccountsRequest, SetProviderAccountDefaultsRequest,
};

const ADAS_SESSION: &str = "session-token-for-ada";

fn an_account(preset_id: &str, id: &str) -> CredentialRecord {
    let preset = preset(preset_id).expect("preset");
    ProviderAccount {
        account: AccountId::new(id),
        preset,
        label: id.to_string(),
        endpoints: endpoints_for(preset, None, None).expect("endpoints"),
        api_key: SecretString::new(format!("key-{id}")),
    }
    .to_record(1_760_000_000)
}

fn defaulting(mut record: CredentialRecord, harnesses: &str) -> CredentialRecord {
    record
        .metadata
        .insert(META_DEFAULT_FOR.to_string(), harnesses.to_string());
    record
}

/// Ada's vault, in memory, applying defaults the way the service asks.
struct AdasVault(Mutex<Vec<CredentialRecord>>);

impl AccountStore for AdasVault {
    fn list(&self, token: &str) -> Result<Vec<CredentialRecord>, AccountsError> {
        (token == ADAS_SESSION)
            .then(|| self.0.lock().unwrap().clone())
            .ok_or(AccountsError::NoSuchSession)
    }
    fn set_label(
        &self,
        _: &str,
        p: &ProviderId,
        a: &AccountId,
        _: &str,
    ) -> Result<CredentialRecord, AccountsError> {
        Err(AccountsError::NotFound {
            provider: p.clone(),
            account: a.clone(),
        })
    }
    fn remove(&self, _: &str, _: &ProviderId, _: &AccountId) -> Result<(), AccountsError> {
        Ok(())
    }
}

impl ProviderAccountDefaults for AdasVault {
    fn set_provider_account_defaults(
        &self,
        _token: &str,
        account: &AccountId,
        harnesses: &[String],
    ) -> Result<Vec<CredentialRecord>, AccountsError> {
        let mut records = self.0.lock().unwrap();
        let mut changed = Vec::new();
        for record in records.iter_mut() {
            let mut held = defaults_of(record);
            let before = held.clone();
            if &record.account == account {
                held = harnesses.to_vec();
            } else {
                held.retain(|h| !harnesses.contains(h));
            }
            if held != before {
                if held.is_empty() {
                    record.metadata.remove(META_DEFAULT_FOR);
                } else {
                    record
                        .metadata
                        .insert(META_DEFAULT_FOR.to_string(), held.join(","));
                }
                changed.push(record.clone());
            }
        }
        Ok(changed)
    }
}

struct AnthropicMessagesOnly;
impl RouteSupport for AnthropicMessagesOnly {
    fn harness_routable(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn supports(&self, _: &str, _: &str, protocols: &[&str]) -> Result<(), String> {
        protocols
            .contains(&"anthropic-messages")
            .then_some(())
            .ok_or_else(|| "Claude Code needs Anthropic Messages".to_string())
    }
}

fn a_service(records: Vec<CredentialRecord>) -> (Arc<AdasVault>, AccountsServiceImpl<AdasVault>) {
    let vault = Arc::new(AdasVault(Mutex::new(records)));
    let service = AccountsServiceImpl::new(Arc::clone(&vault))
        .with_route_support(Arc::new(AnthropicMessagesOnly))
        .with_provider_account_defaults(Arc::clone(&vault) as Arc<dyn ProviderAccountDefaults>);
    (vault, service)
}

fn set_default(account: &str, harnesses: &[&str]) -> Request<SetProviderAccountDefaultsRequest> {
    Request::direct(SetProviderAccountDefaultsRequest {
        session_token: ADAS_SESSION.to_string(),
        account_id: account.to_string(),
        harnesses: harnesses.iter().map(|h| (*h).to_string()).collect(),
    })
}

#[tokio::test]
async fn setting_a_second_default_for_a_harness_clears_the_first() {
    // Given Meta is Ada's Claude CLI default
    let (vault, service) = a_service(vec![
        an_account("fireworks", "fw"),
        defaulting(an_account("meta", "meta"), "claude-cli"),
    ]);

    // When Fireworks becomes it
    let changed = service
        .set_provider_account_defaults(set_default("fw", &["claude-cli"]))
        .await
        .expect("set")
        .into_inner()
        .changed;

    // Then exactly one account holds the default, and both rows are reported
    let held = vault.0.lock().unwrap().clone();
    assert_eq!(
        default_account_for(&held, "claude-cli"),
        Some(AccountId::new("fw"))
    );
    let mut ids: Vec<String> = changed.into_iter().map(|a| a.account_id).collect();
    ids.sort();
    assert_eq!(ids, vec!["fw", "meta"]);
}

#[tokio::test]
async fn a_default_the_account_cannot_drive_is_refused() {
    // Given
    let (vault, service) = a_service(vec![an_account("baseten", "kimi")]);

    // When
    let refusal = service
        .set_provider_account_defaults(set_default("kimi", &["claude-cli"]))
        .await
        .expect_err("refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(refusal.message().contains("Anthropic Messages"));
    assert!(defaults_of(&vault.0.lock().unwrap()[0]).is_empty());
}

#[tokio::test]
async fn an_accounts_defaults_are_listed_on_its_row() {
    // Given
    let (_vault, service) = a_service(vec![defaulting(
        an_account("meta", "meta"),
        "claude-cli,codex",
    )]);

    // When
    let listed = service
        .list_accounts(Request::direct(ListAccountsRequest {
            session_token: ADAS_SESSION.to_string(),
        }))
        .await
        .expect("listed")
        .into_inner();

    // Then
    assert_eq!(
        listed.providers[0].accounts[0].default_for,
        vec!["claude-cli", "codex"]
    );
}

#[test]
fn no_account_is_the_default_for_a_harness_nobody_chose() {
    // Then
    assert_eq!(
        default_account_for(
            &[defaulting(an_account("meta", "meta"), "codex")],
            "claude-cli"
        ),
        None
    );
}
