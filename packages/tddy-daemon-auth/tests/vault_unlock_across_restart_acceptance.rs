//! Acceptance: a daemon restart does not sign anybody out of their credentials.
//!
//! The credential vault opens from the operator's passphrase, and the daemon keeps no key of its
//! own — so after a restart it holds nothing that opens anybody's vault. What carries a session's
//! credentials across a restart without asking for the passphrase again is the **browser**: each
//! lineage that opened the vault holds an unlock key to a slot of its own, and its next session
//! refresh presents it. The daemon reopens the vault through that slot, rotates it, and hands back
//! the replacement.
//!
//! What an operator is entitled to, stated here:
//!
//! - after a restart, the first refresh reopens their vault and PR status reads with their token
//!   again — **with no passphrase and no new login**;
//! - the key rotates at every refresh, so one that has been presented opens nothing afterwards,
//!   yet two tabs sharing one key both keep a working one;
//! - signing out removes that lineage's slot, and the last lineage out closes the vault;
//! - a key for somebody else's vault, or one that no longer opens its slot, refreshes the session
//!   and opens nothing;
//! - neither the key nor the GitHub token is ever where it should not be.

mod support;

use std::sync::Arc;

use base64::Engine;
use support::{
    a_daemon, a_demo_daemon_retaining_credentials_in, call, state, the_auth_service,
    the_token_granted_at_exchange, ANOTHER_LOGIN, THE_LOGIN,
};
use tddy_credentials::CredentialStore;
use tddy_daemon_auth::github_pr_credentials::PrLookup;
use tddy_service::proto::auth::{
    ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse, VaultState,
};

#[tokio::test]
async fn after_a_restart_a_refresh_reopens_the_vault_and_pr_status_reads_with_the_stored_token() {
    // Given an operator who created their vault, and a daemon that has since restarted
    let daemon = a_daemon();
    let (signed_in, created) = daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();

    // When their browser refreshes its session, presenting the key it was handed
    let refreshed = after_restart
        .refresh(&signed_in.refresh_token, &created.vault_unlock_key)
        .await;

    // Then the vault is open and PR status reads with their token — no passphrase, no new login
    assert_eq!(
        (
            state(refreshed.vault_state),
            after_restart.pr_lookup_for(THE_LOGIN)
        ),
        (
            VaultState::Open,
            PrLookup::Perform(the_token_granted_at_exchange(1))
        )
    );
}

#[tokio::test]
async fn before_that_refresh_pr_status_says_to_unlock_the_credential_vault() {
    // Given an operator who created their vault, and a daemon that has since restarted
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();

    // When their still-valid access token asks for PR status before any refresh
    let lookup = after_restart.pr_lookup_for(THE_LOGIN);

    // Then it is unavailable, naming the remedy
    assert!(
        matches!(&lookup, PrLookup::Unavailable(reason) if reason.contains("unlock your credential vault")),
        "expected an unavailable lookup saying to unlock the credential vault, got {lookup:?}"
    );
}

#[tokio::test]
async fn a_first_login_that_has_not_chosen_a_passphrase_sees_pr_status_ask_for_one() {
    // Given an operator signed in for the first time, who has not chosen a passphrase yet
    let daemon = a_daemon();
    let running = daemon.running();
    running.sign_in().await;

    // When PR status is asked for
    let lookup = running.pr_lookup_for(THE_LOGIN);

    // Then it is unavailable, naming the remedy rather than "sign in again"
    assert!(
        matches!(&lookup, PrLookup::Unavailable(reason) if reason.contains("unlock your credential vault")),
        "expected an unavailable lookup saying to unlock the credential vault, got {lookup:?}"
    );
}

#[tokio::test]
async fn a_refresh_rotates_the_unlock_key_so_the_presented_one_opens_nothing() {
    // Given a lineage that has refreshed once since creating its vault
    let daemon = a_daemon();
    let running = daemon.running();
    let (signed_in, created) = running.sign_in_and_create_the_vault().await;
    let refreshed = running
        .refresh(&signed_in.refresh_token, &created.vault_unlock_key)
        .await;

    // When each key is tried against the vault
    let opens = |wire: &str| daemon.unlock_key_opens_the_vault(wire);

    // Then only the rotated one opens it
    assert_eq!(
        (
            opens(&created.vault_unlock_key),
            opens(&refreshed.vault_unlock_key)
        ),
        (false, true)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_tabs_refreshing_with_one_key_at_once_both_keep_a_working_key() {
    // Given one lineage whose key two tabs share, after a restart
    let daemon = a_daemon();
    let (signed_in, created) = daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = Arc::new(daemon.running());

    // When both tabs refresh at the same moment, presenting it
    let tab = |daemon: Arc<support::ARunningDaemon>| {
        let (refresh, key) = (
            signed_in.refresh_token.clone(),
            created.vault_unlock_key.clone(),
        );
        tokio::spawn(async move { daemon.refresh(&refresh, &key).await.vault_unlock_key })
    };
    let (first, second) = (
        tab(Arc::clone(&after_restart)),
        tab(Arc::clone(&after_restart)),
    );
    let (first, second) = (first.await.unwrap(), second.await.unwrap());

    // Then whichever answer the shared storage keeps, it opens the vault
    assert_eq!(
        (first == second, daemon.unlock_key_opens_the_vault(&second)),
        (true, true)
    );
}

#[tokio::test]
async fn signing_out_removes_the_lineages_unlock_slot() {
    // Given a lineage holding a key to its vault
    let daemon = a_daemon();
    let running = daemon.running();
    let (signed_in, created) = running.sign_in_and_create_the_vault().await;

    // When it signs out, handing back its key
    running
        .logout(&signed_in.session_token, &created.vault_unlock_key)
        .await;

    // Then that key opens nothing — the slot is gone, so a stolen copy is worthless too
    assert!(!daemon.unlock_key_opens_the_vault(&created.vault_unlock_key));
}

#[tokio::test]
async fn the_last_lineage_signing_out_closes_the_vault_on_the_daemon() {
    // Given an operator signed in from one browser, with their vault open
    let daemon = a_daemon();
    let running = daemon.running();
    let (signed_in, created) = running.sign_in_and_create_the_vault().await;

    // When that browser signs out
    running
        .logout(&signed_in.session_token, &created.vault_unlock_key)
        .await;

    // Then the daemon can no longer act on their GitHub credential with nobody signed in
    assert!(
        matches!(running.pr_lookup_for(THE_LOGIN), PrLookup::Unavailable(_)),
        "a vault must not stay open after its last lineage has signed out"
    );
}

#[tokio::test]
async fn a_refresh_presenting_another_users_key_opens_nothing_and_leaves_that_key_working() {
    // Given two operators, one of whom holds a key to their own vault
    let daemon = a_daemon();
    let running = daemon.running();
    let (_, operators_vault) = running.sign_in_and_create_the_vault().await;
    let somebody_else = running.sign_in_as(ANOTHER_LOGIN).await;

    // When the other one refreshes presenting the operator's key
    let refreshed = running
        .refresh(
            &somebody_else.refresh_token,
            &operators_vault.vault_unlock_key,
        )
        .await;

    // Then they are handed nothing, and the operator's key was not rotated out from under them
    assert_eq!(
        (
            refreshed.vault_unlock_key,
            daemon.unlock_key_opens_the_vault(&operators_vault.vault_unlock_key)
        ),
        (String::new(), true)
    );
}

#[tokio::test]
async fn a_refresh_whose_key_no_longer_opens_still_refreshes_the_session_and_reports_the_lock() {
    // Given a lineage whose slot was removed at a logout, and a restart
    let daemon = a_daemon();
    let running = daemon.running();
    let (signed_in, created) = running.sign_in_and_create_the_vault().await;
    running
        .logout(&signed_in.session_token, &created.vault_unlock_key)
        .await;
    let after_restart = daemon.running();

    // When a refresh still presents that key
    let refreshed = after_restart
        .refresh(&signed_in.refresh_token, &created.vault_unlock_key)
        .await;

    // Then the session is refreshed all the same, with no key, and the vault is reported locked
    assert_eq!(
        (
            refreshed.session_token.is_empty(),
            refreshed.vault_unlock_key,
            state(refreshed.vault_state)
        ),
        (false, String::new(), VaultState::Locked)
    );
}

#[tokio::test]
async fn the_vault_file_never_holds_the_unlock_key_it_handed_out() {
    // Given a lineage holding a key to its vault
    let daemon = a_daemon();
    let (_, created) = daemon.running().sign_in_and_create_the_vault().await;
    let key_material = created
        .vault_unlock_key
        .rsplit('.')
        .next()
        .expect("an unlock key carries its key material last")
        .to_string();

    // When the vault is read off the daemon's disk
    let on_disk = std::fs::read_to_string(CredentialStore::path_in(daemon.storage(), THE_LOGIN))
        .expect("the vault is on disk");

    // Then the key is not in it — the file holds the wrap, the browser holds the key
    assert!(
        !on_disk.contains(&key_material),
        "the vault file must hold only the wrapped slot, never the key that opens it"
    );
}

#[tokio::test]
async fn a_refresh_response_carries_no_github_token() {
    // Given a lineage holding a key to the vault its login's token is sealed in
    let daemon = a_daemon();
    let running = daemon.running();
    let (signed_in, created) = running.sign_in_and_create_the_vault().await;

    // When it refreshes
    let refreshed = running
        .refresh(&signed_in.refresh_token, &created.vault_unlock_key)
        .await;

    // Then nothing the browser receives carries the credential
    let client_visible = format!(
        "{} {} {:?} {}",
        decoded_parts(&refreshed.session_token),
        decoded_parts(&refreshed.refresh_token),
        refreshed.user,
        refreshed.vault_unlock_key
    );
    assert!(
        !client_visible.contains(&the_token_granted_at_exchange(1)),
        "the GitHub access token must never be returned to the client, found it in: {client_visible}"
    );
}

#[tokio::test]
async fn a_device_login_response_carries_no_github_token() {
    // Given an operator whose vault is open, so a device login's token is sealed at once
    let daemon = a_daemon();
    let running = daemon.running();
    running.sign_in_and_create_the_vault().await;

    // When they sign in by device code
    let completed = running.sign_in_by_device().await;

    // Then nothing the browser receives carries the credential GitHub granted that exchange
    let client_visible = format!(
        "{} {} {:?} {}",
        decoded_parts(&completed.session_token),
        decoded_parts(&completed.refresh_token),
        completed.user,
        completed.vault_unlock_key
    );
    assert!(
        !client_visible.contains(&the_token_granted_at_exchange(2)),
        "the GitHub access token must never be returned to the client, found it in: {client_visible}"
    );
}

#[tokio::test]
async fn a_stub_login_is_handed_no_unlock_key() {
    // Given a demo daemon — its logins keep no vault
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (config, _config_dir) = a_demo_daemon_retaining_credentials_in(&dir.path().join("auth"));
    let auth = the_auth_service(&config);

    // When the demo operator signs in
    let started: GetAuthUrlResponse = call(&auth, "GetAuthUrl", GetAuthUrlRequest {})
        .await
        .expect("a configured daemon hands out an authorize url");
    let signed_in: ExchangeCodeResponse = call(
        &auth,
        "ExchangeCode",
        ExchangeCodeRequest {
            code: "the-code".to_string(),
            state: started.state,
        },
    )
    .await
    .expect("the demo login succeeds");

    // Then there is no key, because there is no vault for it to open
    assert_eq!(signed_in.vault_unlock_key, "");
}

/// Everything a signed token is made of, decoded: `v2.<base64url(claims)>.<base64url(signature)>`.
/// An embedded credential would otherwise hide inside the base64.
fn decoded_parts(token: &str) -> String {
    token
        .split('.')
        .map(|part| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(part)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_else(|_| part.to_string())
        })
        .collect::<Vec<_>>()
        .join(" ")
}
