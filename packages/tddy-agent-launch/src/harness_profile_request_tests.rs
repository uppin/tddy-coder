//! Unit tests: a start naming a harness profile becomes the start the profile stands for, on the
//! account the starter's vault holds under the profile's id or label.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-agent-def-route.md

use std::sync::Arc;

use tddy_accounts::{endpoints_for, preset, ProviderAccount};
use tddy_credentials::{AccountId, CredentialStore, SecretString, SessionVaults};
use tddy_discovery::harness_profile::HarnessProfile;
use tddy_rpc::Code;
use tddy_service::proto::session::StartSessionRequest;

use crate::harness_profile_request::request_on_profile;
use crate::session_acting_identity::SessionAccountAccess;

const ADA: &str = "ada";
const ADAS_SESSION: &str = "session-token-for-ada";
const ADAS_PASSPHRASE: &str = "correct horse battery staple";

fn a_meta_account(id: &str, label: &str) -> ProviderAccount {
    let meta = preset("meta").expect("the meta preset");
    ProviderAccount {
        account: AccountId::new(id),
        preset: meta,
        label: label.to_string(),
        endpoints: endpoints_for(meta, None, None).expect("meta endpoints"),
        api_key: SecretString::new("a-meta-key"),
    }
}

/// A daemon on which Ada's unlocked vault holds `accounts`.
fn ada_holds(accounts: &[ProviderAccount]) -> (tempfile::TempDir, SessionAccountAccess) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let passphrase = SecretString::new(ADAS_PASSPHRASE);
    let store =
        CredentialStore::create(&CredentialStore::path_in(dir.path(), ADA), &passphrase, ADA)
            .expect("Ada's vault is created");
    for account in accounts {
        store
            .put(account.to_record(1_760_000_000))
            .expect("the record is retained");
    }
    let vaults = Arc::new(SessionVaults::new(dir.path()));
    vaults.unlock(ADA, &passphrase).expect("Ada's vault opens");
    let access = SessionAccountAccess::new(
        Some(vaults),
        Arc::new(|t: &str| (t == ADAS_SESSION).then(|| ADA.to_string())),
        ADAS_SESSION,
    );
    (dir, access)
}

fn a_profile(harness: &str, provider_account: &str) -> HarnessProfile {
    HarnessProfile {
        name: "claude-on-meta".to_string(),
        label: None,
        harness: harness.to_string(),
        provider_account: provider_account.to_string(),
        model: "muse-wonder-1.0".to_string(),
    }
}

fn a_start_as_the_profile() -> StartSessionRequest {
    StartSessionRequest {
        session_token: ADAS_SESSION.to_string(),
        agent: "claude-on-meta".to_string(),
        ..Default::default()
    }
}

#[test]
fn a_cli_profile_starts_its_session_type_on_its_account_and_model() {
    // Given
    let (_dir, access) = ada_holds(&[a_meta_account("meta-hackathon", "Meta hackathon")]);

    // When
    let req = request_on_profile(
        a_start_as_the_profile(),
        &a_profile("claude-cli", "Meta hackathon"),
        &access,
    )
    .expect("the profile starts");

    // Then the label is carried as the account's id
    assert_eq!(req.session_type, "claude-cli");
    assert_eq!(req.agent, "");
    assert_eq!(req.model, "muse-wonder-1.0");
    assert_eq!(req.provider_account_id, "meta-hackathon");
}

#[test]
fn a_tool_session_profile_starts_its_agent() {
    // Given
    let (_dir, access) = ada_holds(&[a_meta_account("meta-hackathon", "Meta hackathon")]);

    // When
    let req = request_on_profile(
        a_start_as_the_profile(),
        &a_profile("codex", "meta-hackathon"),
        &access,
    )
    .expect("the profile starts");

    // Then
    assert_eq!(req.session_type, "");
    assert_eq!(req.agent, "codex");
}

#[test]
fn the_requests_own_model_is_kept() {
    // Given
    let (_dir, access) = ada_holds(&[a_meta_account("meta-hackathon", "Meta hackathon")]);
    let start = StartSessionRequest {
        model: "muse-spark".to_string(),
        ..a_start_as_the_profile()
    };

    // When
    let req = request_on_profile(start, &a_profile("claude-cli", "meta-hackathon"), &access)
        .expect("the profile starts");

    // Then
    assert_eq!(req.model, "muse-spark");
}

#[test]
fn a_session_level_account_wins_over_the_profiles() {
    // Given
    let (_dir, access) = ada_holds(&[
        a_meta_account("meta-hackathon", "Meta hackathon"),
        a_meta_account("meta-work", "Meta work"),
    ]);
    let start = StartSessionRequest {
        provider_account_id: "meta-work".to_string(),
        ..a_start_as_the_profile()
    };

    // When
    let req = request_on_profile(start, &a_profile("claude-cli", "meta-hackathon"), &access)
        .expect("the profile starts");

    // Then
    assert_eq!(req.provider_account_id, "meta-work");
}

#[test]
fn a_session_that_chose_built_in_runs_the_profiles_harness_on_its_vendor() {
    // Given Ada holds nothing the profile names: it is never looked up
    let (_dir, access) = ada_holds(&[]);
    let start = StartSessionRequest {
        provider_built_in: true,
        ..a_start_as_the_profile()
    };

    // When
    let req = request_on_profile(start, &a_profile("claude-cli", "meta-hackathon"), &access)
        .expect("the profile starts");

    // Then
    assert_eq!(req.session_type, "claude-cli");
    assert_eq!(req.provider_account_id, "");
    assert!(req.provider_built_in);
}

#[test]
fn a_profile_naming_an_account_the_person_does_not_hold_is_refused_with_the_reason() {
    // Given
    let (_dir, access) = ada_holds(&[a_meta_account("meta-hackathon", "Meta hackathon")]);

    // When
    let refusal = request_on_profile(
        a_start_as_the_profile(),
        &a_profile("claude-cli", "fireworks-main"),
        &access,
    )
    .expect_err("no such account");

    // Then
    assert_eq!(refusal.code, Code::NotFound);
    assert!(
        refusal.message.contains("claude-on-meta") && refusal.message.contains("fireworks-main"),
        "{}",
        refusal.message
    );
}

#[test]
fn a_profile_naming_no_harness_is_refused() {
    // Given
    let (_dir, access) = ada_holds(&[]);

    // When
    let refusal = request_on_profile(
        a_start_as_the_profile(),
        &a_profile("vim", "meta-hackathon"),
        &access,
    )
    .expect_err("vim is no harness");

    // Then
    assert_eq!(refusal.code, Code::InvalidArgument);
}
