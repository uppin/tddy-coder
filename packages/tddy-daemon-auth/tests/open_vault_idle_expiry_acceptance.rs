//! Acceptance: an open credential vault nothing uses is closed, and its data key dropped.
//!
//! An unlocked vault keeps its data key in memory so PR status can read the stored GitHub token.
//! Session tokens are stateless, so the daemon learns of no session ending but a logout, and a
//! lineage that simply stops coming back would keep the vault open until the daemon exits. So a
//! vault nothing has used for `github.open_vault_idle_ttl_seconds` (seven days unless configured;
//! `0` for never) is closed:
//!
//! - afterwards the vault is exactly as after a restart: PR status is unavailable with the reason
//!   a restart gives, and the next refresh presenting an unlock key reopens it;
//! - a use — a credential read, a login, an unlock, a refresh — keeps it open another lifetime;
//! - a sweep closes one nobody looks at, and there is a sweep even when only vaults expire;
//! - startup names the idle lifetime (and warns when it is `0`), and each closing is logged with
//!   the login and how long the vault sat unused — never a token or a passphrase.
//!
//! Time moves by hand: the vaults read a clock the test drives, and the sweep's ticks are tokio's
//! paused clock. Nothing sleeps.

mod support;

use std::sync::Arc;
use std::time::Duration;

use support::captured_log::{captured_log, everything, logged};
use support::{
    a_daemon, state, the_token_granted_at_exchange, AHandDrivenClock, A_NEW_PASSPHRASE, THE_LOGIN,
    THE_PASSPHRASE,
};
use tddy_credentials::{
    AccountId, CredentialRecord, ProviderId, SecretString, SessionVaults, VaultState as Registry,
};
use tddy_daemon_auth::github_pr_credentials::PrLookup;
use tddy_daemon_auth::vault_lifetimes::{
    credential_vaults_in, spawn_credential_sweep, sweep_period, VaultLifetimes,
};
use tddy_daemon_kernel::config::GitHubConfig;
use tddy_service::proto::auth::VaultState;

const AN_HOUR: Duration = Duration::from_secs(60 * 60);

#[tokio::test]
async fn pr_status_for_a_vault_closed_for_idleness_is_unavailable_as_after_a_restart() {
    // Given an operator whose vault nobody has used for its idle lifetime
    let daemon = a_daemon();
    let clock = AHandDrivenClock::new();
    let running = daemon.running_with_open_vaults_idling_out(clock.as_clock(), AN_HOUR);
    running.sign_in_and_create_the_vault().await;
    clock.advance(AN_HOUR);

    // When their access token asks for PR status
    let lookup = running.pr_lookup_for(THE_LOGIN);

    // Then it is unavailable, naming the reopen remedy — exactly as a restarted daemon reports it
    assert_eq!(
        (is_unavailable_until_the_vault_reopens(&lookup), lookup),
        (true, daemon.running().pr_lookup_for(THE_LOGIN))
    );
}

#[tokio::test]
async fn a_refresh_after_the_vault_closed_for_idleness_reopens_it_and_pr_status_reads_again() {
    // Given an operator whose vault was closed because nobody used it for its idle lifetime
    let daemon = a_daemon();
    let clock = AHandDrivenClock::new();
    let running = daemon.running_with_open_vaults_idling_out(clock.as_clock(), AN_HOUR);
    let (signed_in, created) = running.sign_in_and_create_the_vault().await;
    clock.advance(AN_HOUR);
    let closed = running.vaults.evict_idle();

    // When their browser refreshes its session, presenting the key it was handed
    let refreshed = running
        .refresh(&signed_in.refresh_token, &created.vault_unlock_key)
        .await;

    // Then the vault it found closed reopens as after a restart — no passphrase, no new login
    assert_eq!(
        (
            closed,
            state(refreshed.vault_state),
            running.pr_lookup_for(THE_LOGIN)
        ),
        (
            1,
            VaultState::Open,
            PrLookup::Perform(the_token_granted_at_exchange(1))
        )
    );
}

#[tokio::test]
async fn a_pr_status_read_keeps_the_vault_open_for_another_idle_lifetime() {
    // Given an operator whose PR status was read shortly before their vault's idle lifetime ran out
    let daemon = a_daemon();
    let clock = AHandDrivenClock::new();
    let running = daemon.running_with_open_vaults_idling_out(clock.as_clock(), AN_HOUR);
    running.sign_in_and_create_the_vault().await;
    clock.advance(AN_HOUR - Duration::from_secs(1));
    running.pr_lookup_for(THE_LOGIN);

    // When most of an hour passes again — over an hour since the vault opened
    clock.advance(AN_HOUR - Duration::from_secs(1));

    // Then PR status still reads with the stored token
    assert_eq!(
        running.pr_lookup_for(THE_LOGIN),
        PrLookup::Perform(the_token_granted_at_exchange(1))
    );
}

#[tokio::test]
async fn a_refresh_keeps_the_vault_open_for_another_idle_lifetime() {
    // Given an operator whose browser refreshed shortly before their vault's idle lifetime ran out
    let daemon = a_daemon();
    let clock = AHandDrivenClock::new();
    let running = daemon.running_with_open_vaults_idling_out(clock.as_clock(), AN_HOUR);
    let (signed_in, created) = running.sign_in_and_create_the_vault().await;
    clock.advance(AN_HOUR - Duration::from_secs(1));
    running
        .refresh(&signed_in.refresh_token, &created.vault_unlock_key)
        .await;

    // When most of an hour passes again
    clock.advance(AN_HOUR - Duration::from_secs(1));

    // Then the vault is still open
    assert_eq!(
        state(running.status(&signed_in.session_token).await.vault_state),
        VaultState::Open
    );
}

#[tokio::test]
async fn with_an_idle_lifetime_of_zero_an_open_vault_is_never_closed() {
    // Given a daemon built from a `github:` block whose open vaults never idle out, and an operator
    // who created their vault a year ago
    let storage = tempfile::tempdir().expect("a temporary directory");
    let clock = AHandDrivenClock::new();
    let vaults = credential_vaults_in(storage.path(), &the_vault_lifetimes_when_vaults_idle_for(0))
        .with_clock(clock.as_clock());
    vaults
        .retain(THE_LOGIN, a_github_record("gho_a_year_old"))
        .expect("the token is held");
    vaults
        .create(THE_LOGIN, &SecretString::new(THE_PASSPHRASE))
        .expect("the vault is created");
    clock.advance(Duration::from_secs(365 * 24 * 60 * 60));

    // When the sweep runs
    let closed = vaults.evict_idle();

    // Then nothing was closed
    assert_eq!((closed, vaults.state(THE_LOGIN)), (0, Registry::Open));
}

#[tokio::test(start_paused = true)]
async fn the_sweep_closes_an_idle_vault_nobody_looked_at() {
    // Given a daemon's vaults whose only expiring kind is the open vault, with the sweep running,
    // and a vault nobody has used or looked at for its idle lifetime
    let clock = AHandDrivenClock::new();
    let storage = tempfile::tempdir().expect("a temporary directory");
    let vaults = Arc::new(
        SessionVaults::new(storage.path())
            .with_clock(clock.as_clock())
            .with_pending_lifetime(None)
            .with_idle_lifetime(Some(AN_HOUR)),
    );
    let _sweep = spawn_credential_sweep(&vaults).expect("an idle lifetime runs a sweep");
    vaults
        .retain(THE_LOGIN, a_github_record("gho_left_alone"))
        .expect("the token is held");
    vaults
        .create(THE_LOGIN, &SecretString::new(A_NEW_PASSPHRASE))
        .expect("the vault is created");
    clock.advance(AN_HOUR);

    // When the sweep's next tick comes round
    tokio::time::advance(sweep_period(None, Some(AN_HOUR)).expect("a lifetime has a period")).await;
    tokio::task::yield_now().await;

    // Then it has already closed the vault: there is nothing left to close, and it reads locked
    assert_eq!(
        (vaults.evict_idle(), vaults.state(THE_LOGIN)),
        (0, Registry::Locked)
    );
}

#[test]
fn the_sweep_looks_once_per_shortest_lifetime_and_at_least_once_a_minute() {
    // Given / When
    let periods = (
        sweep_period(
            Some(Duration::from_secs(600)),
            Some(Duration::from_secs(30)),
        ),
        sweep_period(Some(Duration::from_secs(20)), Some(AN_HOUR)),
        sweep_period(None, Some(AN_HOUR)),
        sweep_period(None, None),
    );

    // Then
    assert_eq!(
        periods,
        (
            Some(Duration::from_secs(30)),
            Some(Duration::from_secs(20)),
            Some(Duration::from_secs(60)),
            None
        )
    );
}

#[tokio::test]
async fn startup_logs_the_configured_idle_lifetime() {
    // Given a daemon whose every log line is captured
    let log = captured_log();
    let storage = tempfile::tempdir().expect("a temporary directory");

    // When its vaults are built with an idle lifetime of an hour
    credential_vaults_in(
        storage.path(),
        &the_vault_lifetimes_when_vaults_idle_for(3600),
    );

    // Then the idle lifetime is announced
    assert!(
        logged(
            log,
            log::Level::Info,
            &[
                "open credential vault",
                "3600 s",
                "open_vault_idle_ttl_seconds"
            ]
        ),
        "expected an info line naming the idle lifetime, got:\n{}",
        everything(log)
    );
}

#[tokio::test]
async fn startup_warns_when_open_vaults_never_close_for_idleness() {
    // Given a daemon whose every log line is captured
    let log = captured_log();
    let storage = tempfile::tempdir().expect("a temporary directory");

    // When its vaults are built with an idle lifetime of 0
    credential_vaults_in(storage.path(), &the_vault_lifetimes_when_vaults_idle_for(0));

    // Then it warns that the data key stays in memory until a logout or a restart
    assert!(
        logged(
            log,
            log::Level::Warn,
            &["open_vault_idle_ttl_seconds is 0", "until", "restarts"]
        ),
        "expected a warning that open vaults never close, got:\n{}",
        everything(log)
    );
}

#[tokio::test]
async fn closing_an_idle_vault_is_logged_with_the_login_and_its_idle_time_and_no_secret() {
    // Given a daemon whose every log line is captured, and a login's vault left unused for an hour
    let log = captured_log();
    let daemon = a_daemon();
    let clock = AHandDrivenClock::new();
    let running = daemon.running_with_open_vaults_idling_out(clock.as_clock(), AN_HOUR);
    let signed_in = running.sign_in_as("idle-operator").await;
    running
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, true)
        .await
        .expect("a first passphrase creates the vault");
    clock.advance(AN_HOUR);

    // When the sweep closes it
    running.vaults.evict_idle();

    // Then the closing names the login and how long it sat unused — and no line holds the token or
    // the passphrase
    let lines = everything(log);
    assert_eq!(
        (
            logged(
                log,
                log::Level::Info,
                &["closed", "'idle-operator'", "3600 s"]
            ),
            lines.contains("gho_granted_at_exchange") || lines.contains(THE_PASSPHRASE)
        ),
        (true, false),
        "log lines:\n{lines}"
    );
}

/// Whether `lookup` is PR status unavailable until the vault reopens — the reason a restarted
/// daemon gives, naming the next session refresh as the way back.
fn is_unavailable_until_the_vault_reopens(lookup: &PrLookup) -> bool {
    matches!(lookup, PrLookup::Unavailable(reason) if reason.contains("next session refresh"))
}

/// What a `github:` block whose open vaults close once unused for `seconds` resolves to, the rest
/// at defaults.
fn the_vault_lifetimes_when_vaults_idle_for(seconds: u64) -> VaultLifetimes {
    VaultLifetimes::of(&GitHubConfig {
        open_vault_idle_ttl_seconds: Some(seconds),
        ..GitHubConfig::default()
    })
    .expect("a lifetime within its ceiling")
}

/// A GitHub record for [`THE_LOGIN`] holding `token`.
fn a_github_record(token: &str) -> CredentialRecord {
    CredentialRecord {
        provider: ProviderId::new("github"),
        account: AccountId::new(THE_LOGIN),
        label: "The Operator".to_string(),
        secret: SecretString::new(token),
        metadata: Default::default(),
        updated_at: 1,
    }
}
