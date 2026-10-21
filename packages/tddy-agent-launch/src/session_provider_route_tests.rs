//! Unit tests: a session's provider route, resolved at its start from the owner's real vault.
//!
//! The vault is a real sealed one on a temporary directory — the same fixture the GitHub identity
//! tests use — because what is pinned here is that the route reads **the session owner's** vault,
//! with that vault's refusals. The account record is written through `tddy_accounts`'s own codec
//! (#provider-route 1/9), so these tests turn green only once that codec exists.
//!
//! Feature: docs/ft/daemon/claude-cli-session.md (Running on a provider account)

use std::sync::Arc;

use tddy_accounts::{endpoints_for, preset, ProviderAccount};
use tddy_core::provider_route::{Harness, RouteChoice, RouteSources};
use tddy_credentials::{AccountId, CredentialStore, SecretString, SessionVaults};
use tddy_rpc::Code;
use tddy_service::proto::session::StartSessionRequest;

use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route::{
    pty_route_env, refuse_route_in_sandbox, resolve_session_route, resume_route_env,
    route_sources_on_resume, session_route_sources, start_route,
};
use tddy_core::{
    read_session_provider_account, write_session_provider_account, SessionProviderAccount,
    SESSION_PROVIDER_ACCOUNT_FILENAME,
};

const ADA: &str = "ada";
const ADAS_SESSION: &str = "session-token-for-ada";
const ADAS_PASSPHRASE: &str = "correct horse battery staple";
const META_KEY: &str = "a-meta-key";

fn adas_meta_account() -> ProviderAccount {
    let meta = preset("meta").expect("the meta preset");
    ProviderAccount {
        account: AccountId::new("meta-hackathon"),
        preset: meta,
        label: "Meta hackathon".to_string(),
        endpoints: endpoints_for(meta, None, None).expect("meta endpoints"),
        api_key: SecretString::new(META_KEY),
    }
}

/// A daemon on which Ada's vault holds `accounts`, unlocked (`unlocked: true`) or not.
fn a_daemon_where_ada_holds(
    accounts: &[ProviderAccount],
    unlocked: bool,
) -> (tempfile::TempDir, SessionAccountAccess) {
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
    if unlocked {
        vaults
            .unlock(ADA, &passphrase)
            .expect("Ada's passphrase opens her vault");
    }
    let access = SessionAccountAccess::new(
        Some(vaults),
        Arc::new(|t: &str| (t == ADAS_SESSION).then(|| ADA.to_string())),
        ADAS_SESSION,
    );
    (dir, access)
}

fn on_account(id: &str) -> RouteSources {
    RouteSources {
        session: Some(RouteChoice::Account(id.to_string())),
        ..RouteSources::default()
    }
}

fn value_of<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[test]
fn a_claude_cli_start_on_a_meta_account_launches_with_the_route_env() {
    // Given Ada holds her Meta account in an unlocked vault
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);

    // When a claude-cli start chooses it
    let route = resolve_session_route(
        &access,
        Harness::ClaudeCli,
        &on_account("meta-hackathon"),
        "muse-wonder-1.0",
    )
    .expect("the route resolves")
    .expect("a route, not the vendor");
    let env = pty_route_env(&route);

    // Then the PTY gets Meta's Messages root, the key and the pinned model
    assert_eq!(
        value_of(&env, "ANTHROPIC_BASE_URL"),
        Some("https://api.meta.ai")
    );
    assert_eq!(value_of(&env, "ANTHROPIC_AUTH_TOKEN"), Some(META_KEY));
    assert_eq!(value_of(&env, "ANTHROPIC_MODEL"), Some("muse-wonder-1.0"));
    assert_eq!(
        value_of(&env, "CLAUDE_CODE_SUBAGENT_MODEL"),
        Some("muse-wonder-1.0")
    );
}

#[test]
fn a_start_that_chooses_nothing_runs_on_the_vendor() {
    // Given
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);

    // When
    let route = resolve_session_route(
        &access,
        Harness::ClaudeCli,
        &RouteSources::default(),
        "opus",
    )
    .expect("no choice is not an error");

    // Then
    assert_eq!(route, None);
}

#[test]
fn a_locked_vault_refuses_a_routed_start_before_anything_spawns() {
    // Given Ada's vault exists and is not unlocked on this daemon
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], false);

    // When
    let refusal = resolve_session_route(
        &access,
        Harness::ClaudeCli,
        &on_account("meta-hackathon"),
        "muse-wonder-1.0",
    )
    .expect_err("a locked vault refuses");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(refusal.message().contains("locked"));
}

#[test]
fn an_account_the_vault_does_not_hold_is_refused() {
    // Given
    let (_dir, access) = a_daemon_where_ada_holds(&[], true);

    // When
    let refusal = resolve_session_route(
        &access,
        Harness::ClaudeCli,
        &on_account("meta-hackathon"),
        "muse-wonder-1.0",
    )
    .expect_err("unknown account");

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
}

#[test]
fn a_harness_that_cannot_run_on_the_account_is_refused_with_the_reason() {
    // Given
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);

    // When
    let refusal = resolve_session_route(
        &access,
        Harness::CursorCli,
        &on_account("meta-hackathon"),
        "composer-2.5",
    )
    .expect_err("cursor is never routed");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(refusal.message().contains("Cursor"));
}

#[test]
fn a_start_request_names_its_session_level_choice() {
    // Given
    let on_meta = StartSessionRequest {
        provider_account_id: "meta-hackathon".to_string(),
        ..StartSessionRequest::default()
    };
    let built_in = StartSessionRequest {
        provider_built_in: true,
        ..StartSessionRequest::default()
    };

    // Then
    assert_eq!(
        session_route_sources(&on_meta).session,
        Some(RouteChoice::Account("meta-hackathon".to_string()))
    );
    assert_eq!(
        session_route_sources(&built_in).session,
        Some(RouteChoice::BuiltIn)
    );
    assert_eq!(
        session_route_sources(&StartSessionRequest::default()).session,
        None
    );
}

#[test]
fn a_resumed_claude_cli_session_reapplies_its_account() {
    // Given a session started on Ada's Meta account, recorded in its directory
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);
    let session_dir = tempfile::tempdir().expect("a session directory");
    write_session_provider_account(
        session_dir.path(),
        &SessionProviderAccount {
            account_id: "meta-hackathon".to_string(),
            model: "muse-wonder-1.0".to_string(),
        },
    )
    .expect("recorded at start");

    // When it resumes
    let (sources, model) = route_sources_on_resume(session_dir.path()).expect("the record reads");
    let route = resolve_session_route(
        &access,
        Harness::ClaudeCli,
        &sources,
        model.as_deref().expect("the recorded model"),
    )
    .expect("resolves")
    .expect("a route");

    // Then it runs on the same account and model
    let env = pty_route_env(&route);
    assert_eq!(
        value_of(&env, "ANTHROPIC_BASE_URL"),
        Some("https://api.meta.ai")
    );
    assert_eq!(value_of(&env, "ANTHROPIC_MODEL"), Some("muse-wonder-1.0"));
}

#[test]
fn a_resumed_session_with_no_record_runs_on_the_vendor() {
    // Given
    let session_dir = tempfile::tempdir().expect("a session directory");

    // When
    let (sources, model) = route_sources_on_resume(session_dir.path()).expect("absent reads");

    // Then
    assert_eq!(sources, RouteSources::default());
    assert_eq!(model, None);
}

#[test]
fn a_routed_start_records_its_account_and_model_but_never_its_key() {
    // Given Ada holds her Meta account in an unlocked vault
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);
    let session_dir = tempfile::tempdir().expect("a session directory");

    // When a claude-cli start chooses it and records it in the session directory
    let start = start_route(
        &access,
        Harness::ClaudeCli,
        &on_account("meta-hackathon"),
        "muse-wonder-1.0",
    )
    .expect("the route resolves")
    .expect("a route, not the vendor");
    start.record_in(session_dir.path()).expect("recorded");

    // Then the PTY gets the key, and the record names the account and model only
    assert_eq!(value_of(&start.env, "ANTHROPIC_AUTH_TOKEN"), Some(META_KEY));
    assert_eq!(
        read_session_provider_account(session_dir.path()).expect("the record reads"),
        Some(SessionProviderAccount {
            account_id: "meta-hackathon".to_string(),
            model: "muse-wonder-1.0".to_string(),
        })
    );
    let text = std::fs::read_to_string(session_dir.path().join(SESSION_PROVIDER_ACCOUNT_FILENAME))
        .expect("the record exists");
    assert!(!text.contains(META_KEY));
}

#[test]
fn a_start_on_an_account_with_no_model_is_refused() {
    // Given
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);

    // When
    let refusal = start_route(
        &access,
        Harness::ClaudeCli,
        &on_account("meta-hackathon"),
        "  ",
    )
    .expect_err("an account with no model pinned is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(refusal.message().contains("needs a model"));
}

#[test]
fn a_sandboxed_start_on_an_account_is_refused_rather_than_run_on_the_vendor() {
    // When / Then
    let refusal =
        refuse_route_in_sandbox(&on_account("meta-hackathon")).expect_err("not in a sandbox yet");
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(refusal.message().contains("sandbox"));

    assert!(refuse_route_in_sandbox(&RouteSources::default()).is_ok());
    assert!(refuse_route_in_sandbox(&RouteSources {
        session: Some(RouteChoice::BuiltIn),
        ..RouteSources::default()
    })
    .is_ok());
}

#[test]
fn a_resume_whose_account_was_removed_is_refused_rather_than_run_on_the_vendor() {
    // Given a session recorded on Ada's Meta account, which her vault no longer holds
    let (_dir, access) = a_daemon_where_ada_holds(&[], true);
    let session_dir = tempfile::tempdir().expect("a session directory");
    write_session_provider_account(
        session_dir.path(),
        &SessionProviderAccount {
            account_id: "meta-hackathon".to_string(),
            model: "muse-wonder-1.0".to_string(),
        },
    )
    .expect("recorded at start");

    // When it resumes
    let refusal = resume_route_env(&access, Harness::ClaudeCli, session_dir.path())
        .expect_err("a removed account refuses the resume");

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
}

#[test]
fn a_resumed_session_on_its_vendor_gets_no_route_env() {
    // Given a session directory with no account record
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);
    let session_dir = tempfile::tempdir().expect("a session directory");

    // When / Then
    assert_eq!(
        resume_route_env(&access, Harness::ClaudeCli, session_dir.path()).expect("no record"),
        Vec::<(String, String)>::new()
    );
}
