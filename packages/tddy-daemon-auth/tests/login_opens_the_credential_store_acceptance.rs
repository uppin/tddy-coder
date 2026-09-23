//! Acceptance: signing in retains the GitHub token, and says where the credential vault stands.
//!
//! The vault's key comes from the operator's **passphrase**, not from the GitHub token: GitHub
//! mints a new token at every code or device exchange, so a key derived from one would lock the
//! vault at the next login after a restart. Signing in therefore always completes, and the
//! response says what, if anything, the operator must do for their token to be kept:
//!
//! - **OPEN** — the vault is open on this daemon: the token is sealed and the lineage is handed an
//!   unlock slot, with no prompt;
//! - **LOCKED** — a vault exists and is closed here: the token waits in memory until the
//!   passphrase is given;
//! - **UNINITIALIZED** — no vault yet: the token waits until a first passphrase creates one;
//! - **NONE** — a stub login, which holds no credential by construction and creates nothing.
//!
//! A login whose token cannot be retained is *reported* by that state, never silent; a write that
//! fails still fails the login (`tddy-github`'s `github_token_retention_acceptance.rs`).

mod support;

use std::sync::Mutex;

use support::{
    a_daemon, a_demo_daemon_retaining_credentials_in, call, state, the_auth_service,
    the_token_granted_at_exchange, A_NEW_PASSPHRASE, A_WRONG_PASSPHRASE, THE_LOGIN, THE_PASSPHRASE,
};
use tddy_credentials::VaultError;
use tddy_daemon_auth::github_pr_credentials::PrLookup;
use tddy_rpc::Code;
use tddy_service::proto::auth::{
    ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
    UnlockVaultRequest, UnlockVaultResponse, VaultState,
};

#[tokio::test]
async fn the_first_real_login_is_signed_in_and_asked_to_create_the_vault() {
    // Given a daemon that has never held this operator's vault
    let daemon = a_daemon();

    // When they sign in for the first time
    let signed_in = daemon.running().sign_in().await;

    // Then they are signed in, told to choose a passphrase, and nothing is on disk yet
    assert_eq!(
        (
            signed_in.session_token.is_empty(),
            state(signed_in.vault_state),
            signed_in.vault_unlock_key,
            daemon.files_in_storage()
        ),
        (
            false,
            VaultState::Uninitialized,
            String::new(),
            Vec::<String>::new()
        )
    );
}

#[tokio::test]
async fn choosing_a_first_passphrase_creates_the_vault_holding_the_logins_token() {
    // Given an operator signed in for the first time
    let daemon = a_daemon();
    let running = daemon.running();
    let signed_in = running.sign_in().await;

    // When they choose a passphrase
    let created = running
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, true)
        .await
        .expect("a first passphrase creates the vault");

    // Then the vault is open, the lineage holds a key to it, and PR status reads with the token
    assert_eq!(
        (
            state(created.vault_state),
            daemon.unlock_key_opens_the_vault(&created.vault_unlock_key),
            running.pr_lookup_for(THE_LOGIN)
        ),
        (
            VaultState::Open,
            true,
            PrLookup::Perform(the_token_granted_at_exchange(1))
        )
    );
}

#[tokio::test]
async fn after_a_restart_a_login_with_a_new_token_opens_the_vault_once_the_passphrase_is_given() {
    // Given an operator who created their vault, and a daemon that has since restarted
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();

    // When they sign in afresh — GitHub mints them a different token — and give their passphrase
    let signed_in = after_restart.sign_in().await;
    let unlocked = after_restart
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, false)
        .await
        .map(|response| state(response.vault_state))
        .map_err(|status| status.code);

    // Then the login was told the vault is locked rather than refused, the passphrase opened it,
    // and PR status reads with the token this login received
    assert_eq!(
        (
            state(signed_in.vault_state),
            unlocked,
            after_restart.pr_lookup_for(THE_LOGIN)
        ),
        (
            VaultState::Locked,
            Ok(VaultState::Open),
            PrLookup::Perform(the_token_granted_at_exchange(2))
        )
    );
}

#[tokio::test]
async fn the_passphrase_opens_the_same_vault_the_first_login_created() {
    // Given a lineage holding a key to the vault its login created, and a restart
    let daemon = a_daemon();
    let (_, created) = daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();

    // When a later login with a new token unlocks it by passphrase
    let signed_in = after_restart.sign_in().await;
    after_restart
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, false)
        .await
        .expect("the passphrase opens the vault");

    // Then it is the same vault, not a second one: the first lineage's key still opens it
    assert!(daemon.unlock_key_opens_the_vault(&created.vault_unlock_key));
}

#[tokio::test]
async fn a_real_login_over_a_vault_this_daemon_has_not_opened_signs_in_and_leaves_the_file_alone() {
    // Given a vault on disk, and a daemon that has restarted since it was last open
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let before = daemon.vault_on_disk(THE_LOGIN);
    let after_restart = daemon.running();

    // When a real login arrives with a token the vault has never seen
    let signed_in = after_restart.sign_in().await;

    // Then the operator is signed in, the vault is reported locked, and not a byte of it changed
    assert_eq!(
        (
            signed_in.session_token.is_empty(),
            state(signed_in.vault_state),
            daemon.vault_on_disk(THE_LOGIN)
        ),
        (false, VaultState::Locked, before)
    );
}

#[tokio::test]
async fn a_wrong_passphrase_is_refused_naming_the_lock_and_changes_nothing_on_disk() {
    // Given a locked vault after a restart, and the bytes it holds
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let before = daemon.vault_on_disk(THE_LOGIN);
    let after_restart = daemon.running();
    let signed_in = after_restart.sign_in().await;

    // When the wrong passphrase is given
    let refused = after_restart
        .unlock_vault(&signed_in.session_token, A_WRONG_PASSPHRASE, false)
        .await
        .map(|_| ())
        .map_err(|status| (status.code, status.message));

    // Then
    assert_eq!(
        (refused, daemon.vault_on_disk(THE_LOGIN)),
        (
            Err((Code::FailedPrecondition, VaultError::Locked.to_string())),
            before
        )
    );
}

#[tokio::test]
async fn after_a_wrong_passphrase_the_operator_is_still_signed_in_and_the_vault_still_locked() {
    // Given a locked vault after a restart, and a wrong passphrase given for it
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();
    let signed_in = after_restart.sign_in().await;
    let _refused = after_restart
        .unlock_vault(&signed_in.session_token, A_WRONG_PASSPHRASE, false)
        .await;

    // When the page checks the session
    let status = after_restart.status(&signed_in.session_token).await;

    // Then a wrong passphrase costs nothing but the retry
    assert_eq!(
        (status.authenticated, state(status.vault_state)),
        (true, VaultState::Locked)
    );
}

#[tokio::test]
async fn a_login_while_the_vault_is_open_needs_no_passphrase_and_is_handed_an_unlock_slot() {
    // Given an operator whose vault is open on this daemon
    let daemon = a_daemon();
    let running = daemon.running();
    running.sign_in_and_create_the_vault().await;

    // When they sign in from a second browser, with the new token GitHub mints for it
    let second_browser = running.sign_in().await;

    // Then there is no prompt: the new lineage has its own key, and the new token is the one kept
    assert_eq!(
        (
            state(second_browser.vault_state),
            daemon.unlock_key_opens_the_vault(&second_browser.vault_unlock_key),
            running.pr_lookup_for(THE_LOGIN)
        ),
        (
            VaultState::Open,
            true,
            PrLookup::Perform(the_token_granted_at_exchange(2))
        )
    );
}

#[tokio::test]
async fn a_device_login_after_a_restart_reports_the_vault_locked_like_a_redirect_login() {
    // Given a vault on disk, and a daemon that has restarted since it was last open
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();

    // When the operator signs in by device code
    let completed = after_restart.sign_in_by_device().await;

    // Then both flows report the vault alike
    assert_eq!(state(completed.vault_state), VaultState::Locked);
}

#[tokio::test]
async fn a_reset_sets_the_old_vault_aside_and_seals_the_logins_token_into_a_fresh_one() {
    // Given an operator who forgot their passphrase, signed in after a restart
    let daemon = a_daemon();
    daemon.running().sign_in_and_create_the_vault().await;
    let old_vault = daemon.vault_on_disk(THE_LOGIN);
    let after_restart = daemon.running();
    let signed_in = after_restart.sign_in().await;

    // When they reset the vault under a new passphrase
    let reset = after_restart
        .reset_vault(&signed_in.session_token, A_NEW_PASSPHRASE)
        .await
        .expect("a reset succeeds");

    // Then the old file is kept aside, byte for byte, and the fresh vault holds this login's token
    let set_aside = daemon
        .files_in_storage()
        .into_iter()
        .find(|name| name.contains(".locked-"))
        .map(|name| std::fs::read(daemon.storage().join(name)).ok());
    assert_eq!(
        (
            state(reset.vault_state),
            set_aside,
            after_restart.pr_lookup_for(THE_LOGIN)
        ),
        (
            VaultState::Open,
            Some(old_vault),
            PrLookup::Perform(the_token_granted_at_exchange(2))
        )
    );
}

#[tokio::test]
async fn after_a_reset_only_the_new_passphrase_unlocks_the_vault() {
    // Given a vault reset under a new passphrase, and a restart
    let daemon = a_daemon();
    let running = daemon.running();
    let (signed_in, _) = running.sign_in_and_create_the_vault().await;
    running
        .reset_vault(&signed_in.session_token, A_NEW_PASSPHRASE)
        .await
        .expect("a reset succeeds");
    let after_restart = daemon.running();
    let signed_in = after_restart.sign_in().await;

    // When each passphrase is tried
    let old = after_restart
        .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, false)
        .await
        .map(|_| ())
        .map_err(|status| status.code);
    let new = after_restart
        .unlock_vault(&signed_in.session_token, A_NEW_PASSPHRASE, false)
        .await
        .map(|_| ())
        .map_err(|status| status.code);

    // Then
    assert_eq!((old, new), (Err(Code::FailedPrecondition), Ok(())));
}

#[tokio::test]
async fn a_first_passphrase_shorter_than_the_minimum_is_refused_and_creates_nothing() {
    // Given an operator signed in for the first time
    let daemon = a_daemon();
    let running = daemon.running();
    let signed_in = running.sign_in().await;

    // When they choose a passphrase too short to be worth deriving a key from
    let refused = running
        .unlock_vault(&signed_in.session_token, "short", true)
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
async fn unlocking_without_a_valid_access_token_is_unauthenticated() {
    // Given a daemon
    let daemon = a_daemon();

    // When somebody presents a token the daemon never minted
    let refused = daemon
        .running()
        .unlock_vault("not-a-session-token", THE_PASSPHRASE, true)
        .await
        .map(|_| ())
        .map_err(|status| status.code);

    // Then
    assert_eq!(refused, Err(Code::Unauthenticated));
}

#[tokio::test]
async fn no_passphrase_reaches_the_log_the_vault_file_or_any_response() {
    // Given a daemon whose every log line is captured
    let log = captured_log();
    let daemon = a_daemon();
    let running = daemon.running();

    // When an operator creates, fails to unlock, unlocks and resets their vault
    let (signed_in, created) = running.sign_in_and_create_the_vault().await;
    let after_restart = daemon.running();
    let again = after_restart.sign_in().await;
    let wrong = after_restart
        .unlock_vault(&again.session_token, A_WRONG_PASSPHRASE, false)
        .await;
    let unlocked = after_restart
        .unlock_vault(&again.session_token, THE_PASSPHRASE, false)
        .await;
    let reset = after_restart
        .reset_vault(&again.session_token, A_NEW_PASSPHRASE)
        .await;

    // Then none of the three passphrases is in any line logged, byte stored or field answered
    let answered = format!("{signed_in:?} {created:?} {again:?} {wrong:?} {unlocked:?} {reset:?}");
    let on_disk: String = daemon
        .files_in_storage()
        .into_iter()
        .map(|name| {
            String::from_utf8_lossy(&std::fs::read(daemon.storage().join(name)).unwrap())
                .into_owned()
        })
        .collect();
    let logged = log.lock().unwrap().join("\n");
    let leaked: Vec<&str> = [THE_PASSPHRASE, A_WRONG_PASSPHRASE, A_NEW_PASSPHRASE]
        .into_iter()
        .filter(|passphrase| {
            answered.contains(passphrase)
                || on_disk.contains(passphrase)
                || logged.contains(passphrase)
        })
        .collect();
    assert_eq!(leaked, Vec::<&str>::new());
}

#[tokio::test]
async fn a_stub_login_leaves_no_credential_store_behind_and_reports_no_vault() {
    // Given a demo daemon — a stub provider retains nothing, because its access token is synthetic
    let dir = tempfile::tempdir().expect("a temporary directory");
    let storage = dir.path().join("auth");
    let (config, _config_dir) = a_demo_daemon_retaining_credentials_in(&storage);

    // When the demo operator signs in
    let signed_in = sign_in_to(&config).await;

    // Then the login succeeds, reports that there is no vault to unlock, and none exists
    assert_eq!(
        (
            signed_in.session_token.is_empty(),
            state(signed_in.vault_state),
            signed_in.vault_unlock_key,
            std::fs::read_dir(&storage)
                .map(|entries| entries.count())
                .ok()
        ),
        (false, VaultState::None, String::new(), Some(0))
    );
}

#[tokio::test]
async fn a_stub_daemon_has_no_vault_to_unlock() {
    // Given a demo operator signed in
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (config, _config_dir) = a_demo_daemon_retaining_credentials_in(&dir.path().join("auth"));
    let signed_in = sign_in_to(&config).await;

    // When the page asks to create a vault anyway
    let refused: Result<UnlockVaultResponse, _> = call(
        &the_auth_service(&config),
        "UnlockVault",
        UnlockVaultRequest {
            session_token: signed_in.session_token,
            passphrase: THE_PASSPHRASE.to_string(),
            create: true,
        },
    )
    .await;

    // Then there is nothing to create: a demo holds no credential
    assert_eq!(
        refused.map(|_| ()).map_err(|status| status.code),
        Err(Code::FailedPrecondition)
    );
}

/// Complete a whole sign-in through the served `auth.AuthService`.
async fn sign_in_to(config: &tddy_daemon_kernel::config::DaemonConfig) -> ExchangeCodeResponse {
    let auth = the_auth_service(config);
    let started: GetAuthUrlResponse = call(&auth, "GetAuthUrl", GetAuthUrlRequest {})
        .await
        .expect("a configured daemon hands out an authorize url");
    call(
        &auth,
        "ExchangeCode",
        ExchangeCodeRequest {
            code: "the-code".to_string(),
            state: started.state,
        },
    )
    .await
    .expect("the demo login succeeds")
}

/// Every log line this test binary emits from here on, at every level.
fn captured_log() -> &'static Mutex<Vec<String>> {
    // Once per binary; a later call (another test) finds the logger already installed.
    if log::set_logger(&CAPTURING_LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Trace);
    }
    &LOGGED_LINES
}

static LOGGED_LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static CAPTURING_LOGGER: CapturingLogger = CapturingLogger;

struct CapturingLogger;

impl log::Log for CapturingLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        LOGGED_LINES
            .lock()
            .unwrap()
            .push(format!("{} {}", record.target(), record.args()));
    }

    fn flush(&self) {}
}
