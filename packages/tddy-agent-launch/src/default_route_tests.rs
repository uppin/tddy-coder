//! Unit tests: a start that names no account runs on the owner's default for that harness.
//!
//! Docs: packages/tddy-agent-launch/docs/module-layout.md (Provider route of a session)

use std::sync::Arc;

use tddy_accounts::{endpoints_for, preset, ProviderAccount, META_DEFAULT_FOR};
use tddy_coding_agent_specs::{routed_agent, CodingAgentRegistry};
use tddy_core::provider_route::{RouteChoice, RouteSources};
use tddy_credentials::{AccountId, CredentialStore, SecretString, SessionVaults};

use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route::{
    pty_route_env, resolve_session_route, session_route_sources_with_default,
};
use tddy_rpc::Code;
use tddy_service::proto::session::StartSessionRequest;

const ADA: &str = "ada";
const ADAS_SESSION: &str = "session-token-for-ada";
const PASSPHRASE: &str = "correct horse battery staple";

fn ada_whose_meta_account_is_the_claude_cli_default() -> (tempfile::TempDir, SessionAccountAccess) {
    ada_whose_meta_account_is_the_default_for("claude-cli")
}

fn ada_whose_meta_account_is_the_default_for(
    harness: &str,
) -> (tempfile::TempDir, SessionAccountAccess) {
    let dir = tempfile::tempdir().expect("dir");
    let passphrase = SecretString::new(PASSPHRASE);
    let store =
        CredentialStore::create(&CredentialStore::path_in(dir.path(), ADA), &passphrase, ADA)
            .expect("vault");
    let meta = preset("meta").expect("meta");
    let mut record = ProviderAccount {
        account: AccountId::new("meta-hackathon"),
        preset: meta,
        label: "Meta".to_string(),
        endpoints: endpoints_for(meta, None, None).expect("endpoints"),
        api_key: SecretString::new("a-meta-key"),
    }
    .to_record(1_760_000_000);
    record
        .metadata
        .insert(META_DEFAULT_FOR.to_string(), harness.to_string());
    store.put(record).expect("stored");
    let vaults = Arc::new(SessionVaults::new(dir.path()));
    vaults.unlock(ADA, &passphrase).expect("unlocked");
    let access = SessionAccountAccess::new(
        Some(vaults),
        Arc::new(|t: &str| (t == ADAS_SESSION).then(|| ADA.to_string())),
        ADAS_SESSION,
    );
    (dir, access)
}

/// What a claude-cli start's route is built from: the built-in `claude` spec.
fn the_claude() -> tddy_core::provider_route::RoutedAgent {
    let registry = CodingAgentRegistry::builtin().expect("the built-ins load");
    routed_agent(registry.get("claude").expect("claude is built in"))
}

fn value_of<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[test]
fn a_claude_cli_start_without_an_account_uses_the_persons_default() {
    // Given
    let (_dir, access) = ada_whose_meta_account_is_the_claude_cli_default();
    let sources = RouteSources {
        user_default: access
            .default_provider_account("claude-cli")
            .expect("readable"),
        ..RouteSources::default()
    };

    // When
    let route = resolve_session_route(&access, &the_claude(), &sources, "muse-spark-1.3")
        .expect("resolves")
        .expect("the default applies");

    // Then
    assert_eq!(
        value_of(&pty_route_env(&route), "ANTHROPIC_BASE_URL"),
        Some("https://api.meta.ai")
    );
}

#[test]
fn an_explicit_built_in_choice_overrides_the_default() {
    // Given
    let (_dir, access) = ada_whose_meta_account_is_the_claude_cli_default();
    let sources = RouteSources {
        session: Some(RouteChoice::BuiltIn),
        user_default: access
            .default_provider_account("claude-cli")
            .expect("readable"),
        ..RouteSources::default()
    };

    // Then
    assert_eq!(
        resolve_session_route(&access, &the_claude(), &sources, "opus").expect("resolves"),
        None
    );
}

#[test]
fn a_harness_with_no_default_has_none() {
    // Given
    let (_dir, access) = ada_whose_meta_account_is_the_claude_cli_default();

    // Then
    assert_eq!(
        access.default_provider_account("codex").expect("readable"),
        None
    );
}

/// Ada's vault from [`ada_whose_meta_account_is_the_claude_cli_default`], after a daemon restart
/// left it locked.
fn ada_whose_vault_is_locked() -> (tempfile::TempDir, SessionAccountAccess) {
    let (dir, _unlocked) = ada_whose_meta_account_is_the_claude_cli_default();
    let access = SessionAccountAccess::new(
        Some(Arc::new(SessionVaults::new(dir.path()))),
        Arc::new(|t: &str| (t == ADAS_SESSION).then(|| ADA.to_string())),
        ADAS_SESSION,
    );
    (dir, access)
}

fn a_start(built_in: bool) -> StartSessionRequest {
    StartSessionRequest {
        session_token: ADAS_SESSION.to_string(),
        provider_built_in: built_in,
        ..StartSessionRequest::default()
    }
}

#[test]
fn a_start_naming_no_account_reads_the_default_into_its_sources() {
    // Given
    let (_dir, access) = ada_whose_meta_account_is_the_claude_cli_default();

    // When
    let sources = session_route_sources_with_default(&a_start(false), "claude-cli", &access)
        .expect("readable");

    // Then
    assert_eq!(sources.user_default.as_deref(), Some("meta-hackathon"));
}

#[test]
fn a_start_naming_no_account_is_refused_while_the_vault_is_locked() {
    // Given
    let (_dir, access) = ada_whose_vault_is_locked();

    // When
    let refusal = session_route_sources_with_default(&a_start(false), "claude-cli", &access)
        .expect_err("the default cannot be read");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
}

#[test]
fn an_explicit_built_in_start_does_not_need_the_vault_unlocked() {
    // Given
    let (_dir, access) = ada_whose_vault_is_locked();

    // When
    let sources = session_route_sources_with_default(&a_start(true), "claude-cli", &access)
        .expect("an explicit choice needs no default");

    // Then
    assert_eq!(sources.session, Some(RouteChoice::BuiltIn));
    assert_eq!(sources.user_default, None);
}

#[test]
fn a_default_stored_under_either_id_form_applies_to_its_own_form() {
    // Given Ada's Meta account is the default for `claude-cli` (the session type) and `claude`
    // (the agent): both spellings are written by the accounts screen, and none is migrated
    let (_dir, access) = ada_whose_meta_account_is_the_claude_cli_default();
    let (_other_dir, other) = ada_whose_meta_account_is_the_default_for("claude");

    // Then
    assert_eq!(
        (
            access
                .default_provider_account("claude-cli")
                .expect("reads"),
            access.default_provider_account("claude").expect("reads"),
            other.default_provider_account("claude").expect("reads"),
            other.default_provider_account("claude-cli").expect("reads"),
        ),
        (
            Some("meta-hackathon".to_string()),
            None,
            Some("meta-hackathon".to_string()),
            None,
        )
    );
}
