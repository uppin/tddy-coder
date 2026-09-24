//! Acceptance: a sign-in's GitHub token waits in memory for its vault only so long.
//!
//! A sign-in over a closed vault holds the token GitHub granted in memory, unsealed, until the
//! passphrase opens the vault — and that waiting token is what permits choosing a passphrase at
//! all (a first one, or a reset). Both expire after `github.pending_login_ttl_seconds` (ten minutes
//! unless configured; `0` for never):
//!
//! - past it, a first passphrase or a reset is refused, telling the operator to sign in to GitHub
//!   again, and the vault is reported exactly as closed as it was;
//! - a sweep drops an expired token even when nobody asks for it, and there is no sweep at all
//!   when tokens never expire;
//! - each of those moments is logged — the lifetime at startup, the hold, the expiry, the refusal —
//!   and neither the token nor a passphrase ever is.
//!
//! Time moves by hand: the vaults read a clock the test drives, and the sweep's ticks are tokio's
//! paused clock. Nothing sleeps.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use support::{
    a_daemon, state, the_token_granted_at_exchange, AHandDrivenClock, A_NEW_PASSPHRASE, THE_LOGIN,
    THE_PASSPHRASE,
};
use tddy_credentials::{
    AccountId, CredentialRecord, ProviderId, SecretString, SessionVaults, VaultError,
};
use tddy_daemon_auth::pending_logins::{
    credential_vaults_in, spawn_pending_login_sweep, sweep_period,
};
use tddy_daemon_kernel::pending_login_ttl::PendingLoginTtl;
use tddy_rpc::Code;
use tddy_service::proto::auth::VaultState;

const TEN_MINUTES: Duration = Duration::from_secs(600);
const A_MOMENT_PAST_TEN_MINUTES: Duration = Duration::from_secs(601);

#[tokio::test]
async fn a_first_passphrase_after_the_sign_in_expired_is_refused_telling_the_operator_to_sign_in_again(
) {
    // Given a first sign-in whose token has waited past its lifetime
    let daemon = a_daemon();
    let clock = AHandDrivenClock::new();
    let running = daemon.running_on(clock.as_clock(), Some(TEN_MINUTES));
    let signed_in = running.sign_in().await;
    clock.advance(A_MOMENT_PAST_TEN_MINUTES);

    // When the operator chooses a first passphrase
    let refused = running
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, true)
        .await
        .map(|_| ())
        .map_err(|status| {
            (
                status.code,
                status.message.contains("sign in to GitHub again"),
            )
        });

    // Then it is refused with the remedy, nothing is created, and the vault is still uncreated
    assert_eq!(
        (
            refused,
            daemon.files_in_storage(),
            state(running.status(&signed_in.session_token).await.vault_state)
        ),
        (
            Err((Code::FailedPrecondition, true)),
            Vec::<String>::new(),
            VaultState::Uninitialized
        )
    );
}

#[tokio::test]
async fn a_reset_after_the_sign_in_expired_is_refused_and_the_vault_stays_locked() {
    // Given a locked vault after a restart, and a sign-in whose token then waited past its lifetime
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let before = daemon.vault_on_disk(THE_LOGIN);
    let clock = AHandDrivenClock::new();
    let after_restart = daemon.running_on(clock.as_clock(), Some(TEN_MINUTES));
    let signed_in = after_restart.sign_in().await;
    clock.advance(A_MOMENT_PAST_TEN_MINUTES);

    // When a reset is asked for
    let refused = after_restart
        .reset_vault(&signed_in.session_token, A_NEW_PASSPHRASE)
        .await
        .map(|_| ())
        .map_err(|status| (status.code, status.message));

    // Then
    assert_eq!(
        (
            refused,
            daemon.vault_on_disk(THE_LOGIN),
            state(
                after_restart
                    .status(&signed_in.session_token)
                    .await
                    .vault_state
            )
        ),
        (
            Err((
                Code::FailedPrecondition,
                VaultError::NoFreshLogin.to_string()
            )),
            before,
            VaultState::Locked
        )
    );
}

#[tokio::test]
async fn a_sign_in_on_a_daemon_whose_pending_logins_never_expire_still_creates_the_vault_a_year_on()
{
    // Given a daemon configured with a lifetime of 0, and a first sign-in a year ago
    let daemon = a_daemon();
    let clock = AHandDrivenClock::new();
    let running = daemon.running_on(
        clock.as_clock(),
        PendingLoginTtl::from_seconds(0).unwrap().lifetime(),
    );
    let signed_in = running.sign_in().await;
    clock.advance(Duration::from_secs(365 * 24 * 60 * 60));

    // When the operator chooses a first passphrase
    let created = running
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, true)
        .await
        .map(|response| state(response.vault_state))
        .map_err(|status| status.code);

    // Then
    assert_eq!(created, Ok(VaultState::Open));
}

#[tokio::test(start_paused = true)]
async fn the_sweep_drops_an_expired_token_nobody_touched() {
    // Given a daemon's vaults with the sweep running, and a sign-in's token left waiting until
    // past its lifetime with nobody asking for it
    let clock = AHandDrivenClock::new();
    let storage = tempfile::tempdir().expect("a temporary directory");
    let vaults = Arc::new(
        SessionVaults::new(storage.path())
            .with_clock(clock.as_clock())
            .with_pending_lifetime(Some(TEN_MINUTES)),
    );
    let _sweep = spawn_pending_login_sweep(&vaults).expect("a lifetime runs a sweep");
    vaults
        .retain(
            THE_LOGIN,
            a_github_record(&the_token_granted_at_exchange(1)),
        )
        .expect("the token is held");
    clock.advance(A_MOMENT_PAST_TEN_MINUTES);

    // When the sweep's next tick comes round
    tokio::time::advance(sweep_period(TEN_MINUTES)).await;
    tokio::task::yield_now().await;

    // Then it has already dropped the token: there is nothing left for anybody to expire
    assert_eq!(vaults.expire_pending(), 0);
}

#[tokio::test]
async fn a_daemon_whose_pending_logins_never_expire_runs_no_sweep() {
    // Given a daemon's vaults whose pending sign-ins never expire
    let storage = tempfile::tempdir().expect("a temporary directory");
    let vaults = Arc::new(SessionVaults::new(storage.path()).with_pending_lifetime(None));

    // When the sweep is asked for
    let sweep = spawn_pending_login_sweep(&vaults);

    // Then there is none
    assert!(sweep.is_none());
}

#[test]
fn the_sweep_runs_at_least_once_a_minute_and_at_least_once_a_lifetime() {
    // Given / When
    let periods = (
        sweep_period(TEN_MINUTES),
        sweep_period(Duration::from_secs(20)),
    );

    // Then
    assert_eq!(periods, (Duration::from_secs(60), Duration::from_secs(20)));
}

#[tokio::test]
async fn startup_logs_the_configured_lifetime() {
    // Given a daemon whose every log line is captured
    let log = captured_log();
    let storage = tempfile::tempdir().expect("a temporary directory");

    // When its vaults are built with a two-minute lifetime
    credential_vaults_in(storage.path(), PendingLoginTtl::from_seconds(120).unwrap());

    // Then the lifetime is announced
    assert!(
        logged(log, log::Level::Info, &["pending", "120 s"]),
        "expected an info line naming the lifetime, got:\n{}",
        everything(log)
    );
}

#[tokio::test]
async fn startup_warns_when_pending_tokens_never_expire() {
    // Given a daemon whose every log line is captured
    let log = captured_log();
    let storage = tempfile::tempdir().expect("a temporary directory");

    // When its vaults are built with a lifetime of 0
    credential_vaults_in(storage.path(), PendingLoginTtl::from_seconds(0).unwrap());

    // Then it warns where the tokens stay, and for how long
    assert!(
        logged(log, log::Level::Warn, &["held in memory until", "restart"]),
        "expected a warning that pending tokens never expire, got:\n{}",
        everything(log)
    );
}

#[tokio::test]
async fn holding_expiring_and_refusing_are_each_logged_and_the_token_never_is() {
    // Given a daemon whose every log line is captured, and a first sign-in by a login of its own
    let log = captured_log();
    let daemon = a_daemon();
    let clock = AHandDrivenClock::new();
    let running = daemon.running_on(clock.as_clock(), Some(TEN_MINUTES));
    let signed_in = running.sign_in_as("logged-operator").await;

    // When its token outlives its lifetime, and a first passphrase is then chosen anyway
    clock.advance(A_MOMENT_PAST_TEN_MINUTES);
    let _refused = running
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, true)
        .await;

    // Then the hold, the expiry and the refusal were logged, naming the login — and no line holds
    // the token or the passphrase
    let lines = everything(log);
    assert_eq!(
        (
            logged(
                log,
                log::Level::Info,
                &["holding", "'logged-operator'", "expires in 600 s"]
            ),
            logged(
                log,
                log::Level::Info,
                &["dropped", "'logged-operator'", "waited 601 s"]
            ),
            logged(
                log,
                log::Level::Warn,
                &["refused", "'logged-operator'", "expired"]
            ),
            lines.contains("gho_granted_at_exchange") || lines.contains(THE_PASSPHRASE)
        ),
        (true, true, true, false),
        "log lines:\n{lines}"
    );
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

// ---------------------------------------------------------------------------
// Log capture — the same once-per-binary logger `login_opens_the_credential_store_acceptance.rs`
// installs, keeping each line's level.
// ---------------------------------------------------------------------------

type Lines = Mutex<Vec<(log::Level, String)>>;

/// Every log line this test binary emits from here on, at every level.
fn captured_log() -> &'static Lines {
    if log::set_logger(&CAPTURING_LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Trace);
    }
    &LOGGED_LINES
}

/// Whether a line at `level` holds every one of `parts`.
fn logged(log: &Lines, level: log::Level, parts: &[&str]) -> bool {
    log.lock()
        .unwrap()
        .iter()
        .any(|(at, line)| *at == level && parts.iter().all(|part| line.contains(part)))
}

fn everything(log: &Lines) -> String {
    log.lock()
        .unwrap()
        .iter()
        .map(|(level, line)| format!("{level} {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

static LOGGED_LINES: Lines = Mutex::new(Vec::new());
static CAPTURING_LOGGER: CapturingLogger = CapturingLogger;

struct CapturingLogger;

impl log::Log for CapturingLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        LOGGED_LINES.lock().unwrap().push((
            record.level(),
            format!("{} {}", record.target(), record.args()),
        ));
    }

    fn flush(&self) {}
}
