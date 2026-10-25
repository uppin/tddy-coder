//! Acceptance: `github.dev_vault_without_passphrase` — a development daemon whose credential
//! vaults need no passphrase.
//!
//! `./web-dev` and `./desktop-dev` sign in with stub logins. Their vaults were prompted for like
//! any other, and a prompt that came after a restart could not be answered at all: choosing a first
//! passphrase needs a fresh sign-in on this daemon run, and a session that merely refreshed has
//! none. With the switch on, the daemon creates and opens each vault under the empty passphrase
//! itself, at sign-in and at every refresh, and reports it **open** — so nothing ever prompts.
//!
//! It is a development exclusion, fenced as one:
//!
//! - **only with `stub: true`** — a daemon whose logins carry real GitHub tokens does not start
//!   with it, so no real credential is ever sealed under a key anyone can derive;
//! - a vault that already exists under a **real passphrase** is not opened, reset or replaced: it
//!   stays locked and asks for that passphrase, as it would without the switch.

mod support;

use support::{a_demo_daemon_configured_with, call, state, the_auth_service, THE_PASSPHRASE};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::ServiceEntry;
use tddy_service::proto::auth::{
    ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
    RefreshSessionRequest, RefreshSessionResponse, UnlockVaultRequest, UnlockVaultResponse,
    VaultState,
};

const THE_SWITCH: &str = "  dev_vault_without_passphrase: true\n";

#[tokio::test]
async fn a_stub_sign_in_finds_its_vault_open_without_a_passphrase() {
    // Given a dev daemon with the switch on, that has never held this operator's vault
    let storage = tempfile::tempdir().expect("a temporary directory");
    let (config, _dir) = a_demo_daemon_configured_with(storage.path(), THE_SWITCH);

    // When the operator signs in
    let signed_in = sign_in_through(&the_auth_service(&config)).await;

    // Then the vault exists, is open, and the lineage holds a key to it — nothing to prompt for
    assert_eq!(
        (
            state(signed_in.vault_state),
            signed_in.vault_unlock_key.is_empty(),
            vault_files_in(storage.path()).len()
        ),
        (VaultState::Open, false, 1)
    );
}

#[tokio::test]
async fn after_a_restart_a_refresh_reopens_the_vault_without_its_unlock_key() {
    // Given an operator who signed in, and a daemon that has since restarted
    let storage = tempfile::tempdir().expect("a temporary directory");
    let (config, _dir) = a_demo_daemon_configured_with(storage.path(), THE_SWITCH);
    let signed_in = sign_in_through(&the_auth_service(&config)).await;
    let restarted = the_auth_service(&config);

    // When their session refreshes presenting no unlock key (a browser that never kept one)
    let refreshed = refresh_through(&restarted, &signed_in.refresh_token, "").await;

    // Then the vault is open again, and the lineage is handed a key to it
    assert_eq!(
        (
            state(refreshed.vault_state),
            refreshed.vault_unlock_key.is_empty()
        ),
        (VaultState::Open, false)
    );
}

/// The trap the switch exists for: a session signed in before the daemon kept vaults (or more than
/// a pending sign-in's lifetime ago) refreshes into "no vault yet", and a first passphrase is then
/// refused for want of a fresh sign-in.
#[tokio::test]
async fn a_refresh_creates_the_vault_for_a_session_signed_in_before_there_was_one() {
    // Given an operator signed in on a run without the switch, who chose no passphrase
    let storage = tempfile::tempdir().expect("a temporary directory");
    let (before, _before_dir) = a_demo_daemon_configured_with(storage.path(), "");
    let signed_in = sign_in_through(&the_auth_service(&before)).await;
    assert_eq!(state(signed_in.vault_state), VaultState::Uninitialized);

    // When the daemon restarts with the switch on, and their session refreshes
    let (after, _after_dir) = a_demo_daemon_configured_with(storage.path(), THE_SWITCH);
    let refreshed = refresh_through(&the_auth_service(&after), &signed_in.refresh_token, "").await;

    // Then the vault is created and open — no fresh sign-in, no passphrase
    assert_eq!(
        (
            state(refreshed.vault_state),
            vault_files_in(storage.path()).len()
        ),
        (VaultState::Open, 1)
    );
}

#[tokio::test]
async fn a_vault_with_a_real_passphrase_stays_locked_and_untouched() {
    // Given an operator who created their vault under a real passphrase, on a run without the
    // switch
    let storage = tempfile::tempdir().expect("a temporary directory");
    let (before, _before_dir) = a_demo_daemon_configured_with(storage.path(), "");
    let auth = the_auth_service(&before);
    let signed_in = sign_in_through(&auth).await;
    let _: UnlockVaultResponse = call(
        &auth,
        "UnlockVault",
        UnlockVaultRequest {
            session_token: signed_in.session_token.clone(),
            passphrase: THE_PASSPHRASE.to_string(),
            create: true,
        },
    )
    .await
    .expect("a first passphrase creates the vault");
    let vault_before = vault_bytes_in(storage.path());

    // When the daemon restarts with the switch on, and their session refreshes without a key
    let (after, _after_dir) = a_demo_daemon_configured_with(storage.path(), THE_SWITCH);
    let refreshed = refresh_through(&the_auth_service(&after), &signed_in.refresh_token, "").await;

    // Then the vault is still locked — its passphrase opens it, nothing reset or replaced it
    assert_eq!(
        (
            state(refreshed.vault_state),
            vault_bytes_in(storage.path())
        ),
        (VaultState::Locked, vault_before)
    );
}

#[test]
fn a_daemon_signing_in_with_real_github_does_not_start_with_the_switch() {
    // Given a daemon configured for real GitHub logins, with the switch on
    let storage = tempfile::tempdir().expect("a temporary directory");
    let config = a_daemon_from(&format!(
        "auth_storage: \"{}\"\ngithub:\n  client_id: \"the-app\"\n{THE_SWITCH}",
        storage.path().display()
    ));

    // When auth is wired at startup
    let built = tddy_daemon_auth::build_auth_entries(
        &config,
        "127.0.0.1",
        8080,
        tddy_daemon_livekit::room_tokens::room_token_minter(None),
    );

    // Then the daemon refuses to start, naming the setting and what it needs
    let error = built
        .err()
        .expect("a real-GitHub daemon with the switch is refused")
        .to_string();
    assert!(
        error.contains("dev_vault_without_passphrase") && error.contains("stub: true"),
        "the refusal names the setting and the stub it needs: {error}"
    );
}

/// Complete a whole stub sign-in through one served `auth.AuthService`.
async fn sign_in_through(auth: &ServiceEntry) -> ExchangeCodeResponse {
    let started: GetAuthUrlResponse = call(auth, "GetAuthUrl", GetAuthUrlRequest {})
        .await
        .expect("a configured daemon hands out an authorize url");
    call(
        auth,
        "ExchangeCode",
        ExchangeCodeRequest {
            code: "the-code".to_string(),
            state: started.state,
        },
    )
    .await
    .expect("the demo login succeeds")
}

async fn refresh_through(
    auth: &ServiceEntry,
    refresh_token: &str,
    vault_unlock_key: &str,
) -> RefreshSessionResponse {
    call(
        auth,
        "RefreshSession",
        RefreshSessionRequest {
            refresh_token: refresh_token.to_string(),
            vault_unlock_key: vault_unlock_key.to_string(),
        },
    )
    .await
    .expect("a valid refresh token extends the session")
}

fn a_daemon_from(yaml: &str) -> DaemonConfig {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).expect("the config is written");
    DaemonConfig::load(&path).expect("the config loads")
}

/// The credential vaults in `storage` — not the daemon's signing key, which lives there too.
fn vault_files_in(storage: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(storage)
        .expect("the storage directory is readable")
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.starts_with("credentials-") && name.ends_with(".vault"))
        .collect()
}

/// The one vault in `storage`, byte for byte.
fn vault_bytes_in(storage: &std::path::Path) -> Vec<u8> {
    let files = vault_files_in(storage);
    assert_eq!(files.len(), 1, "exactly one vault: {files:?}");
    std::fs::read(storage.join(&files[0])).expect("the vault is readable")
}
