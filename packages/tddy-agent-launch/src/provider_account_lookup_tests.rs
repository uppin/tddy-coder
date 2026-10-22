//! Unit tests: a provider account is found by its id, or by its one label — never by a guess.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-agent-def-route.md

use tddy_accounts::{endpoints_for, preset, AccountsError, ProviderAccount};
use tddy_credentials::{AccountId, CredentialRecord, SecretString};

use crate::provider_account_lookup::find_provider_account;
use crate::session_acting_identity::SessionIdentityRefusal;

fn a_meta_account(id: &str, label: &str) -> CredentialRecord {
    let meta = preset("meta").expect("the meta preset");
    ProviderAccount {
        account: AccountId::new(id),
        preset: meta,
        label: label.to_string(),
        endpoints: endpoints_for(meta, None, None).expect("meta endpoints"),
        api_key: SecretString::new("a-meta-key"),
    }
    .to_record(1_760_000_000)
}

fn found_id(held: &[CredentialRecord], wanted: &str) -> Result<String, SessionIdentityRefusal> {
    find_provider_account(held, wanted).map(|account| account.account.as_str().to_string())
}

#[test]
fn an_account_is_found_by_its_id() {
    // Given
    let held = [a_meta_account("meta-hackathon", "Meta hackathon")];

    // Then
    assert_eq!(
        found_id(&held, "meta-hackathon"),
        Ok("meta-hackathon".to_string())
    );
}

#[test]
fn an_account_is_found_by_its_label() {
    // Given
    let held = [a_meta_account("meta-hackathon", "Meta hackathon")];

    // Then
    assert_eq!(
        found_id(&held, "Meta hackathon"),
        Ok("meta-hackathon".to_string())
    );
}

#[test]
fn an_id_wins_over_another_accounts_label() {
    // Given one account's label is another's id
    let held = [
        a_meta_account("work", "personal"),
        a_meta_account("personal", "Personal"),
    ];

    // Then
    assert_eq!(found_id(&held, "personal"), Ok("personal".to_string()));
}

#[test]
fn a_label_two_accounts_share_is_refused() {
    // Given
    let held = [
        a_meta_account("meta-a", "Meta"),
        a_meta_account("meta-b", "Meta"),
    ];

    // When
    let refusal = found_id(&held, "Meta").expect_err("ambiguous");

    // Then
    assert_eq!(
        refusal,
        SessionIdentityRefusal::AmbiguousLabel("Meta".to_string())
    );
    assert!(refusal.to_string().contains("more than one"), "{refusal}");
}

#[test]
fn an_account_not_held_is_not_found() {
    // Given
    let held = [a_meta_account("meta-hackathon", "Meta hackathon")];

    // Then
    assert!(matches!(
        found_id(&held, "fireworks-main"),
        Err(SessionIdentityRefusal::Vault(
            AccountsError::NotFound { .. }
        ))
    ));
}
