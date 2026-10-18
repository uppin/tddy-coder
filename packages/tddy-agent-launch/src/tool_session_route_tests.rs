//! Unit tests: a tool session's route file, resolved from the owner's real vault for its agent.
//!
//! Changeset: docs/dev/1-WIP/2026-10-18-provider-route-tool-route.md

use std::sync::Arc;

use tddy_accounts::{endpoints_for, preset, ProviderAccount};
use tddy_core::provider_route::{RouteChoice, RouteSources};
use tddy_core::provider_route_file::route_from_json;
use tddy_credentials::{AccountId, CredentialStore, SecretString, SessionVaults};
use tddy_rpc::Code;

use crate::session_acting_identity::SessionAccountAccess;
use crate::tool_session_route::tool_session_route_json;

const ADA: &str = "ada";
const ADAS_SESSION: &str = "session-token-for-ada";
const ADAS_PASSPHRASE: &str = "correct horse battery staple";

fn ada_holding_a_meta_account() -> (tempfile::TempDir, SessionAccountAccess) {
    let dir = tempfile::tempdir().expect("dir");
    let passphrase = SecretString::new(ADAS_PASSPHRASE);
    let store =
        CredentialStore::create(&CredentialStore::path_in(dir.path(), ADA), &passphrase, ADA)
            .expect("vault");
    let meta = preset("meta").expect("meta");
    store
        .put(
            ProviderAccount {
                account: AccountId::new("meta-hackathon"),
                preset: meta,
                label: "Meta".to_string(),
                endpoints: endpoints_for(meta, None, None).expect("endpoints"),
                api_key: SecretString::new("a-meta-key"),
            }
            .to_record(1_760_000_000),
        )
        .expect("stored");
    let vaults = Arc::new(SessionVaults::new(dir.path()));
    vaults.unlock(ADA, &passphrase).expect("unlocked");
    let access = SessionAccountAccess::new(
        Some(vaults),
        Arc::new(|t: &str| (t == ADAS_SESSION).then(|| ADA.to_string())),
        ADAS_SESSION,
    );
    (dir, access)
}

fn on_meta() -> RouteSources {
    RouteSources {
        session: Some(RouteChoice::Account("meta-hackathon".to_string())),
        ..RouteSources::default()
    }
}

#[test]
fn a_codex_tool_session_on_meta_is_handed_a_route_file_with_its_provider() {
    // Given
    let (_dir, access) = ada_holding_a_meta_account();

    // When
    let json = tool_session_route_json(&access, "codex", &on_meta(), "muse-wonder-1.0")
        .expect("resolves")
        .expect("a route");

    // Then
    let route = route_from_json(&json).expect("a route file");
    assert!(route
        .args
        .iter()
        .any(|a| a.contains("wire_api=\"responses\"")));
    assert_eq!(route.secret_env[0].1.expose(), "a-meta-key");
}

#[test]
fn a_cursor_tool_session_with_an_account_is_refused() {
    // Given
    let (_dir, access) = ada_holding_a_meta_account();

    // When
    let refusal = tool_session_route_json(&access, "cursor", &on_meta(), "composer-2.5")
        .expect_err("refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(refusal.message().contains("Cursor"));
}

#[test]
fn a_specialized_agent_definition_with_an_account_is_refused() {
    // Given
    let (_dir, access) = ada_holding_a_meta_account();

    // When
    let refusal = tool_session_route_json(&access, "repo-explorer", &on_meta(), "m")
        .expect_err("a definition is its own endpoint");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
}

#[test]
fn a_tool_session_that_chooses_nothing_gets_no_route_file() {
    // Given
    let (_dir, access) = ada_holding_a_meta_account();

    // Then
    assert_eq!(
        tool_session_route_json(&access, "claude", &RouteSources::default(), "opus")
            .expect("no choice"),
        None
    );
}
