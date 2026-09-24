//! Acceptance: what it takes to decide who controls a credential vault, and what guessing costs.
//!
//! Choosing a vault's passphrase — creating it, or resetting a forgotten one — decides who can read
//! the credentials in it from then on. A session token is not enough proof for that: an access
//! token crosses a plain-http LAN origin and a copy of it may have been taken off the wire. What
//! proves the caller holds the GitHub account is a **fresh sign-in on this daemon**, whose token is
//! waiting in memory for the vault; so:
//!
//! - a create or a reset needs that waiting token, and an access token alone is refused;
//! - a reset is refused while the vault is open — nothing was forgotten;
//! - a sign-out by a lineage that never opened the vault drops the token it left waiting;
//! - wrong passphrases cost time: past a few free attempts, each unlock waits longer, and the
//!   refusal says how long;
//! - a passphrase outside the accepted lengths, or a refresh token where an access token belongs,
//!   is refused before anything is derived.

mod support;

use support::{a_daemon, state, A_NEW_PASSPHRASE, A_WRONG_PASSPHRASE, THE_LOGIN, THE_PASSPHRASE};
use tddy_credentials::{VaultError, MAX_PASSPHRASE_CHARS};
use tddy_github::FREE_WRONG_PASSPHRASES;
use tddy_rpc::Code;
use tddy_service::proto::auth::VaultState;

#[tokio::test]
async fn a_reset_presenting_only_an_access_token_after_a_restart_is_refused_and_changes_nothing() {
    // Given an operator's vault, a daemon restart, and a still-valid access token from before it —
    // but no sign-in since
    let daemon = a_daemon();
    let (signed_in, _) = daemon.running().sign_in_and_create_the_vault().await;
    let before = daemon.vault_on_disk(THE_LOGIN);
    let after_restart = daemon.running();

    // When that token asks for a reset
    let refused = after_restart
        .reset_vault(&signed_in.session_token, A_NEW_PASSPHRASE)
        .await
        .map(|_| ())
        .map_err(|status| (status.code, status.message));

    // Then only a fresh sign-in may choose a new passphrase, and not a byte of the vault moved
    assert_eq!(
        (
            refused,
            daemon.vault_on_disk(THE_LOGIN),
            daemon.files_in_storage().len()
        ),
        (
            Err((
                Code::FailedPrecondition,
                VaultError::NoFreshLogin.to_string()
            )),
            before,
            1
        )
    );
}

#[tokio::test]
async fn a_first_passphrase_presenting_only_an_access_token_after_a_restart_is_refused() {
    // Given an operator who signed in but never chose a passphrase, and then a daemon restart
    let daemon = a_daemon();
    let signed_in = daemon.running().sign_in().await;
    let after_restart = daemon.running();

    // When their still-valid access token asks to create the vault
    let refused = after_restart
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, true)
        .await
        .map(|_| ())
        .map_err(|status| (status.code, status.message));

    // Then nothing is created: the token that login left waiting did not survive the restart
    assert_eq!(
        (refused, daemon.files_in_storage()),
        (
            Err((
                Code::FailedPrecondition,
                VaultError::NoFreshLogin.to_string()
            )),
            Vec::<String>::new()
        )
    );
}

#[tokio::test]
async fn a_reset_while_the_vault_is_open_is_refused_and_changes_nothing() {
    // Given an operator whose vault is open, signed in again from a second browser
    let daemon = a_daemon();
    let running = daemon.running();
    running.sign_in_and_create_the_vault().await;
    let second_browser = running.sign_in().await;
    let before = daemon.vault_on_disk(THE_LOGIN);

    // When a reset is asked for
    let refused = running
        .reset_vault(&second_browser.session_token, A_NEW_PASSPHRASE)
        .await
        .map(|_| ())
        .map_err(|status| (status.code, status.message));

    // Then
    assert_eq!(
        (refused, daemon.vault_on_disk(THE_LOGIN)),
        (
            Err((
                Code::FailedPrecondition,
                VaultError::AlreadyOpen.to_string()
            )),
            before
        )
    );
}

#[tokio::test]
async fn a_reset_to_a_passphrase_shorter_than_the_minimum_is_refused_and_changes_nothing() {
    // Given a locked vault after a restart, and a fresh sign-in
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let before = daemon.vault_on_disk(THE_LOGIN);
    let after_restart = daemon.running();
    let signed_in = after_restart.sign_in().await;

    // When the operator resets it to a passphrase too short to derive a key from
    let refused = after_restart
        .reset_vault(&signed_in.session_token, "short")
        .await
        .map(|_| ())
        .map_err(|status| status.code);

    // Then
    assert_eq!(
        (
            refused,
            daemon.vault_on_disk(THE_LOGIN),
            daemon.files_in_storage().len()
        ),
        (Err(Code::InvalidArgument), before, 1)
    );
}

#[tokio::test]
async fn a_passphrase_longer_than_the_maximum_is_refused_before_anything_is_derived() {
    // Given an operator signed in for the first time
    let daemon = a_daemon();
    let running = daemon.running();
    let signed_in = running.sign_in().await;
    let far_too_long = "x".repeat(MAX_PASSPHRASE_CHARS + 1);

    // When they choose a passphrase longer than any person types
    let refused = running
        .unlock_vault(&signed_in.session_token, &far_too_long, true)
        .await
        .map(|_| ())
        .map_err(|status| status.code);

    // Then
    assert_eq!(
        (refused, daemon.files_in_storage()),
        (Err(Code::InvalidArgument), Vec::<String>::new())
    );
}

#[tokio::test]
async fn unlocking_with_a_refresh_token_rather_than_an_access_token_is_unauthenticated() {
    // Given a locked vault after a restart, and a fresh sign-in
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();
    let signed_in = after_restart.sign_in().await;

    // When the unlock presents the refresh token
    let refused = after_restart
        .unlock_vault(&signed_in.refresh_token, THE_PASSPHRASE, false)
        .await
        .map(|_| ())
        .map_err(|status| status.code);

    // Then a minting credential is not proof of a session
    assert_eq!(refused, Err(Code::Unauthenticated));
}

#[tokio::test]
async fn resetting_with_a_refresh_token_rather_than_an_access_token_is_unauthenticated() {
    // Given a locked vault after a restart, and a fresh sign-in
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let before = daemon.vault_on_disk(THE_LOGIN);
    let after_restart = daemon.running();
    let signed_in = after_restart.sign_in().await;

    // When the reset presents the refresh token
    let refused = after_restart
        .reset_vault(&signed_in.refresh_token, A_NEW_PASSPHRASE)
        .await
        .map(|_| ())
        .map_err(|status| status.code);

    // Then
    assert_eq!(
        (refused, daemon.vault_on_disk(THE_LOGIN)),
        (Err(Code::Unauthenticated), before)
    );
}

#[tokio::test]
async fn signing_out_without_an_unlock_key_drops_the_token_the_login_left_waiting() {
    // Given a sign-in after a restart that found the vault locked, its token waiting in memory
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();
    let signed_in = after_restart.sign_in().await;

    // When that lineage signs out without ever unlocking — it holds no unlock key
    after_restart.logout(&signed_in.session_token, "").await;

    // Then the live GitHub token no longer sits in the daemon's memory
    assert_eq!(
        (
            state(signed_in.vault_state),
            after_restart.vaults.holds_pending(THE_LOGIN)
        ),
        (VaultState::Locked, false)
    );
}

#[tokio::test]
async fn a_wrong_passphrase_within_the_free_attempts_does_not_hold_up_the_right_one() {
    // Given a locked vault after a restart, a fresh sign-in, and one mistyped passphrase
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();
    let signed_in = after_restart.sign_in().await;
    let _mistyped = after_restart
        .unlock_vault(&signed_in.session_token, A_WRONG_PASSPHRASE, false)
        .await;

    // When the right one follows at once
    let unlocked = after_restart
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, false)
        .await
        .map(|response| state(response.vault_state))
        .map_err(|status| status.code);

    // Then
    assert_eq!(unlocked, Ok(VaultState::Open));
}

#[tokio::test]
async fn past_the_free_wrong_passphrases_the_next_attempt_is_told_how_long_to_wait() {
    // Given a locked vault after a restart, a fresh sign-in, and every free attempt spent on a
    // wrong passphrase
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();
    let signed_in = after_restart.sign_in().await;
    for _ in 0..=FREE_WRONG_PASSPHRASES {
        let _wrong = after_restart
            .unlock_vault(&signed_in.session_token, A_WRONG_PASSPHRASE, false)
            .await;
    }

    // When the next attempt arrives at once — even with the right passphrase
    let refused = after_restart
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, false)
        .await
        .map(|_| ())
        .map_err(|status| (status.code, status.message.contains("retry after")));

    // Then it is refused without being tried, naming when to retry, and the vault stays locked
    assert_eq!(
        (refused, after_restart.vaults.get(THE_LOGIN).is_none()),
        (Err((Code::ResourceExhausted, true)), true)
    );
}
