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

use crate::cli_route_launch::{resume_cli_route, start_cli_route};
use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route::{
    pty_route_env, resolve_session_route, resume_route, resume_route_env, route_sources_on_resume,
    sandboxed_start_route, session_route_sources, start_route,
};
use std::os::unix::fs::PermissionsExt;
use tddy_core::cli_harness::cli_harness;
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
fn a_sandboxed_start_on_an_account_runs_on_that_account() {
    // Given Ada holds her Meta account in an unlocked vault and a session directory to start in
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);
    let sessions = tempfile::tempdir().expect("a sessions directory");
    let session_dir = sessions.path().join("session");

    // When a sandboxed claude-cli start chooses that account
    let route = sandboxed_start_route(
        &access,
        Harness::ClaudeCli,
        &on_account("meta-hackathon"),
        "muse-wonder-1.0",
        &session_dir,
    )
    .expect("the route resolves")
    .expect("a route, not the vendor");

    // Then the jail carries Meta's endpoint and model, and the key only as a secret
    assert_eq!(
        value_of(&route.env, "ANTHROPIC_BASE_URL"),
        Some("https://api.meta.ai")
    );
    assert_eq!(
        value_of(&route.env, "ANTHROPIC_MODEL"),
        Some("muse-wonder-1.0")
    );
    assert_eq!(
        route
            .secret_env
            .iter()
            .find(|(name, _)| name == "ANTHROPIC_AUTH_TOKEN")
            .map(|(_, secret)| secret.expose()),
        Some(META_KEY)
    );
    assert!(!route.env.iter().any(|(_, value)| value == META_KEY));
}

#[test]
fn a_sandboxed_start_that_chooses_nothing_runs_on_the_vendor() {
    // Given Ada holds an account but the session chooses none, and a session directory to start in
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);
    let sessions = tempfile::tempdir().expect("a sessions directory");
    let session_dir = sessions.path().join("session");

    // When / Then
    assert_eq!(
        sandboxed_start_route(
            &access,
            Harness::ClaudeCli,
            &RouteSources::default(),
            "muse-wonder-1.0",
            &session_dir,
        )
        .expect("a start that chooses nothing reads nothing from the vault"),
        None
    );
    assert!(!session_dir.exists());
}

#[test]
fn a_resumed_sandboxed_session_reapplies_its_account_as_a_route() {
    // Given a sandboxed session started on Ada's Meta account, recorded in its directory
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
    let route = resume_route(&access, Harness::ClaudeCli, session_dir.path())
        .expect("the record reads and the account resolves")
        .expect("a route, not the vendor");

    // Then the relaunched jail runs on the same account and model
    assert_eq!(
        value_of(&route.env, "ANTHROPIC_BASE_URL"),
        Some("https://api.meta.ai")
    );
    assert_eq!(
        value_of(&route.env, "ANTHROPIC_MODEL"),
        Some("muse-wonder-1.0")
    );
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

#[test]
fn a_routed_claude_cli_start_records_its_account_in_the_session_dir_it_creates() {
    // Given Ada holds her Meta account in an unlocked vault
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);
    let sessions_base = tempfile::tempdir().expect("a sessions base");

    // When a claude-cli start on it creates its session directory
    let (session_dir, route) = crate::claude_cli_spawn::create_routed_session_dir(
        sessions_base.path(),
        "sess-1",
        &access,
        &on_account("meta-hackathon"),
        "muse-wonder-1.0",
    )
    .expect("the start proceeds");

    // Then the directory exists and records the account the route runs on
    assert!(route.is_some());
    assert_eq!(
        read_session_provider_account(&session_dir).expect("the record reads"),
        Some(SessionProviderAccount {
            account_id: "meta-hackathon".to_string(),
            model: "muse-wonder-1.0".to_string(),
        })
    );
}

#[test]
fn a_refused_claude_cli_start_creates_no_session_dir() {
    // Given Ada's vault is locked
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], false);
    let sessions_base = tempfile::tempdir().expect("a sessions base");

    // When a claude-cli start chooses her account
    let refusal = crate::claude_cli_spawn::create_routed_session_dir(
        sessions_base.path(),
        "sess-1",
        &access,
        &on_account("meta-hackathon"),
        "muse-wonder-1.0",
    )
    .expect_err("a locked vault refuses");

    // Then nothing was created on disk
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert_eq!(
        std::fs::read_dir(sessions_base.path())
            .expect("the base reads")
            .count(),
        0
    );
}

#[test]
fn a_claude_cli_start_on_its_vendor_creates_its_session_dir_with_no_record() {
    // Given
    let (_dir, access) = a_daemon_where_ada_holds(&[], true);
    let sessions_base = tempfile::tempdir().expect("a sessions base");

    // When
    let (session_dir, route) = crate::claude_cli_spawn::create_routed_session_dir(
        sessions_base.path(),
        "sess-1",
        &access,
        &RouteSources::default(),
        "opus",
    )
    .expect("the start proceeds");

    // Then
    assert!(route.is_none());
    assert!(session_dir.is_dir());
    assert_eq!(
        read_session_provider_account(&session_dir).expect("absent reads"),
        None
    );
}

// ---------------------------------------------------------------------------
// A CLI harness that routes through files and arguments (OpenCode)
// ---------------------------------------------------------------------------

#[test]
fn an_opencode_start_on_a_meta_account_writes_its_config_and_launches_with_it() {
    // Given Ada holds her Meta account, and OpenCode's session directory is empty
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);
    let session_dir = tempfile::tempdir().expect("a session directory");
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When an opencode-cli start chooses the account
    let launch = start_cli_route(
        &access,
        opencode,
        &on_account("meta-hackathon"),
        "muse-wonder-1.0",
        session_dir.path(),
    )
    .expect("the route resolves")
    .expect("a route, not the vendor");

    // Then opencode.json is in the session directory, owner-only, and OPENCODE_CONFIG names it
    let config = session_dir.path().join("opencode.json");
    let mode = std::fs::metadata(&config)
        .expect("written")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(
        value_of(&launch.env, "OPENCODE_CONFIG"),
        Some(config.to_str().expect("utf-8"))
    );
    // And the key is in the child's environment and nowhere on disk
    assert_eq!(
        value_of(&launch.env, "TDDY_OPENCODE_PROVIDER_KEY"),
        Some(META_KEY)
    );
    assert!(!std::fs::read_to_string(&config)
        .expect("readable")
        .contains(META_KEY));
    // And the model is selected on the generated provider
    assert_eq!(launch.args, vec!["--model", "tddy-route/muse-wonder-1.0"]);
}

#[test]
fn an_opencode_start_that_chooses_nothing_writes_nothing_and_records_nothing() {
    // Given
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);
    let session_dir = tempfile::tempdir().expect("a session directory");
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When
    let launch = start_cli_route(
        &access,
        opencode,
        &RouteSources::default(),
        "anthropic/claude-sonnet-4",
        session_dir.path(),
    )
    .expect("no choice is not an error");

    // Then OpenCode runs on the person's own setup
    assert_eq!(launch, None);
    assert_eq!(
        std::fs::read_dir(session_dir.path()).expect("dir").count(),
        0
    );
}

#[test]
fn an_opencode_start_records_its_account_and_model_but_never_its_key() {
    // Given
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);
    let session_dir = tempfile::tempdir().expect("a session directory");
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When
    start_cli_route(
        &access,
        opencode,
        &on_account("meta-hackathon"),
        "muse-wonder-1.0",
        session_dir.path(),
    )
    .expect("the route resolves");

    // Then
    assert_eq!(
        read_session_provider_account(session_dir.path()).expect("readable"),
        Some(SessionProviderAccount {
            account_id: "meta-hackathon".to_string(),
            model: "muse-wonder-1.0".to_string(),
        })
    );
    let record =
        std::fs::read_to_string(session_dir.path().join(SESSION_PROVIDER_ACCOUNT_FILENAME))
            .expect("the record");
    assert!(!record.contains(META_KEY));
}

#[test]
fn an_opencode_start_on_an_account_that_serves_neither_openai_protocol_is_refused() {
    // Given an account that serves only Anthropic Messages
    let anthropic = preset("anthropic-compatible").expect("the anthropic-compatible preset");
    let account = ProviderAccount {
        account: AccountId::new("anthropic-direct"),
        preset: anthropic,
        label: "Anthropic".to_string(),
        endpoints: endpoints_for(anthropic, Some("https://llm.example.com"), None)
            .expect("anthropic endpoints"),
        api_key: SecretString::new("an-anthropic-key"),
    };
    let (_dir, access) = a_daemon_where_ada_holds(&[account], true);
    let session_dir = tempfile::tempdir().expect("a session directory");
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When
    let refusal = start_cli_route(
        &access,
        opencode,
        &on_account("anthropic-direct"),
        "claude-sonnet-4",
        session_dir.path(),
    )
    .expect_err("the harness cannot run on the account");

    // Then it is refused before anything is written
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert_eq!(
        std::fs::read_dir(session_dir.path()).expect("dir").count(),
        0
    );
}

#[test]
fn a_resumed_opencode_session_regenerates_its_config_from_the_vault_as_it_stands() {
    // Given a session recorded on the Meta account
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
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When it resumes
    let launch = resume_cli_route(&access, opencode, session_dir.path()).expect("re-resolved");

    // Then the config is written again and the key comes from the vault
    assert!(session_dir.path().join("opencode.json").is_file());
    assert_eq!(
        value_of(&launch.env, "TDDY_OPENCODE_PROVIDER_KEY"),
        Some(META_KEY)
    );
    assert_eq!(launch.args, vec!["--model", "tddy-route/muse-wonder-1.0"]);
}

#[test]
fn a_resumed_opencode_session_on_its_own_setup_gets_no_route() {
    // Given a session directory with no account record
    let (_dir, access) = a_daemon_where_ada_holds(&[adas_meta_account()], true);
    let session_dir = tempfile::tempdir().expect("a session directory");
    let opencode = cli_harness("opencode-cli").expect("registered");

    // When / Then
    assert_eq!(
        resume_cli_route(&access, opencode, session_dir.path()).expect("no record"),
        crate::cli_route_launch::CliRouteLaunch::default()
    );
}
