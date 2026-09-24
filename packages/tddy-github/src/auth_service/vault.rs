//! The credential-vault half of the auth service: retaining a login's GitHub token, reopening
//! the vault at a refresh, removing a lineage's slot at logout, and serving `UnlockVault` /
//! `ResetVault`. Split out of `auth_service.rs` so the session-token half and the vault half can
//! each be read on their own; the public surface is unchanged and re-exported from there.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Semaphore, SemaphorePermit};

use tddy_credentials::{
    AccountId, CredentialRecord, ProviderId, SecretString, SessionVaults, UnlockKey, VaultError,
    VaultState, MAX_PASSPHRASE_CHARS, MIN_PASSPHRASE_CHARS,
};
use tddy_rpc::{Code, Status};
use tddy_service::proto::auth::{
    LogoutRequest, ResetVaultRequest, ResetVaultResponse, UnlockVaultRequest, UnlockVaultResponse,
    VaultState as ProtoVaultState,
};

mod backoff;

use backoff::PassphraseBackoff;
pub use backoff::FREE_WRONG_PASSPHRASES;

/// How many passphrase key derivations this service runs at once, whoever asked for them.
///
/// Each is Argon2id over 19 MiB, tens of milliseconds of one core. They run on tokio's blocking
/// pool rather than on an RPC worker, and no more than this many at a time, so a flood of unlock
/// attempts — from one user or many — queues behind them instead of starving every other RPC.
const CONCURRENT_KEY_DERIVATIONS: usize = 2;

/// What vault passphrases may cost this daemon: a bound on concurrent key derivations, and each
/// user's backoff after wrong passphrases. One per auth service, which a daemon builds once.
pub(super) struct UnlockThrottle {
    derivations: Semaphore,
    backoff: PassphraseBackoff,
}

impl Default for UnlockThrottle {
    fn default() -> Self {
        Self {
            derivations: Semaphore::new(CONCURRENT_KEY_DERIVATIONS),
            backoff: PassphraseBackoff::default(),
        }
    }
}

impl UnlockThrottle {
    /// A turn to derive a key; waits while [`CONCURRENT_KEY_DERIVATIONS`] others are running.
    async fn turn(&self) -> Result<SemaphorePermit<'_>, Status> {
        self.derivations
            .acquire()
            .await
            .map_err(|_| Status::internal("the credential vault's key derivations have stopped"))
    }

    /// Refused, naming the wait, while `login` is still owed one for wrong passphrases. Asked once
    /// a turn is held, so attempts that queued behind a derivation are judged by its outcome.
    fn allow(&self, login: &str) -> Result<(), Status> {
        match self.backoff.retry_after(login, Instant::now()) {
            None => Ok(()),
            Some(wait) => Err(Status {
                code: Code::ResourceExhausted,
                message: format!(
                    "too many wrong passphrases for the credential vault of '{login}'; retry \
                     after {} s",
                    whole_seconds(wait)
                ),
            }),
        }
    }

    /// Count what an unlock attempt by `login` came to: a wrong passphrase extends their wait, a
    /// right one ends it.
    fn record<T>(&self, login: &str, outcome: &Result<T, VaultError>) {
        match outcome {
            Ok(_) => self.backoff.right(login),
            Err(VaultError::Locked) => self.backoff.wrong(login, Instant::now()),
            Err(_) => {}
        }
    }
}

/// `wait` rounded up to whole seconds, so a retry after the stated time is never still refused.
fn whole_seconds(wait: Duration) -> u64 {
    wait.as_secs() + u64::from(wait.subsec_nanos() > 0)
}

/// Run a vault operation that derives a key from a passphrase on tokio's blocking pool.
async fn derive_off_the_executor<T: Send + 'static>(
    derive: impl FnOnce() -> Result<T, VaultError> + Send + 'static,
) -> Result<Result<T, VaultError>, Status> {
    tokio::task::spawn_blocking(derive).await.map_err(|e| {
        log::error!(
            target: "tddy_github::auth_service",
            "a credential vault key derivation did not finish: {e}"
        );
        Status::internal("the credential vault operation did not finish")
    })
}

use super::{AuthServiceImpl, GITHUB_PROVIDER};
use crate::provider::{GitHubOAuthProvider, GitHubUser};
use crate::session_token_v2::TokenKind;

impl<P: GitHubOAuthProvider> AuthServiceImpl<P> {
    /// The vaults this service retains real logins' credentials in — `None` when it keeps none:
    /// no `auth_storage`, or a stub provider, whose token is synthetic and different on every
    /// exchange, so a demo holds no credential by construction (D12).
    pub(super) fn retaining_vaults(&self) -> Option<&SessionVaults> {
        self.credential_vaults
            .as_deref()
            .filter(|_| self.provider.issues_usable_access_token())
    }

    /// Where `login`'s vault stands, as a response reports it.
    pub(super) fn vault_state_of(&self, login: &str) -> ProtoVaultState {
        self.retaining_vaults()
            .map_or(ProtoVaultState::None, |vaults| {
                to_proto_state(vaults.state(login))
            })
    }

    /// Retain this login's GitHub token, and say where the operator's vault stands — with the
    /// signing-in lineage's unlock key in its wire form when the vault is open, `""` otherwise.
    ///
    /// Open: the token is sealed and the lineage is handed an unlock slot, with no prompt. Closed
    /// (`Locked` after a restart, `Uninitialized` before the first passphrase): the token is held
    /// in memory for the vault, and the state tells the client to prompt. Either way the login is
    /// **reported**, never silently half-done: a session minted while its token cannot be read
    /// says so. A write that fails is still a failed login — the operator would otherwise appear
    /// signed in while every GitHub-backed read reported itself unavailable.
    pub(super) fn retain_the_login_credential(
        &self,
        user: &GitHubUser,
        access_token: &str,
    ) -> Result<(ProtoVaultState, String), Status> {
        let Some(vaults) = self.retaining_vaults() else {
            return Ok((ProtoVaultState::None, String::new()));
        };
        let login = &user.login;
        let retained = vaults
            .retain(login, github_record(user, access_token)?)
            .map_err(|e| refused_by_the_vault(login, "retain the GitHub access token", e))?;
        if retained.state != VaultState::Open {
            log::info!(
                target: "tddy_github::auth_service",
                "the credential vault of '{login}' is {:?} on this daemon; its GitHub token is held \
                 in memory until the vault is unlocked",
                retained.state
            );
        }
        Ok((
            to_proto_state(retained.state),
            retained
                .unlock_key
                .map(|unlock| unlock.to_wire())
                .unwrap_or_default(),
        ))
    }

    /// Reopen the refreshing user's vault through the unlock key their lineage presented, and
    /// return the rotated key with the vault's state.
    ///
    /// `""` is returned only when no key was presented, or the presented one can never open this
    /// user's vault again: another user's, malformed, or [`VaultError::Locked`] — its slot was
    /// rotated, removed or evicted. Any other failure (the file could not be read, say) says
    /// nothing about the key, so the presented key is handed back **unrotated**: an empty one
    /// would make the browser throw away its only way back into the vault over a passing I/O error.
    ///
    /// Never fails the refresh. The session token and the vault are separate things: refusing
    /// the refresh would sign the operator out of everything for a credential-store problem.
    /// Every key that does not reopen the vault is logged, and the state says what opens it now.
    pub(super) fn reopen_the_vault(
        &self,
        login: &str,
        presented: &str,
    ) -> (String, ProtoVaultState) {
        let Some(vaults) = self.retaining_vaults() else {
            return (String::new(), ProtoVaultState::None);
        };
        if presented.is_empty() {
            return (String::new(), to_proto_state(vaults.state(login)));
        }
        let reopened = UnlockKey::from_wire(presented)
            .filter(|unlock| unlock.subject() == login)
            .ok_or(VaultError::Locked)
            .and_then(|unlock| vaults.reopen(&unlock));
        match reopened {
            Ok(rotated) => (rotated.to_wire(), ProtoVaultState::Open),
            Err(VaultError::Locked) => {
                log::warn!(
                    target: "tddy_github::auth_service",
                    "the vault unlock key presented at session refresh for login '{login}' opens \
                     no slot of its credential vault; it stays as it is until its passphrase is \
                     given"
                );
                (String::new(), to_proto_state(vaults.state(login)))
            }
            Err(e) => {
                log::error!(
                    target: "tddy_github::auth_service",
                    "could not reopen the credential vault of '{login}' at session refresh ({e}); \
                     handing the presented unlock key back unrotated"
                );
                (presented.to_string(), to_proto_state(vaults.state(login)))
            }
        }
    }

    /// The GitHub login an access token was minted for — `unauthenticated` for anything else.
    pub(super) async fn caller_login(&self, session_token: &str) -> Result<String, Status> {
        let Some(ref signing) = self.signing else {
            return Err(Status::failed_precondition(
                "session token signing is not configured",
            ));
        };
        let claims = signing
            .authority
            .verify(session_token)
            .await
            .map_err(|e| Status::unauthenticated(e.to_string()))?;
        if claims.kind != TokenKind::Access {
            return Err(Status::unauthenticated(
                "session token: not an access token",
            ));
        }
        Ok(claims.login)
    }

    /// The vaults an unlock or a reset acts on, or why there is nothing to act on.
    pub(super) fn vaults_to_unlock(&self) -> Result<Arc<SessionVaults>, Status> {
        self.credential_vaults
            .as_ref()
            .filter(|_| self.provider.issues_usable_access_token())
            .map(Arc::clone)
            .ok_or_else(|| {
                Status::failed_precondition("this daemon keeps no credential vault for this login")
            })
    }

    /// `UnlockVault`: open the caller's vault with its passphrase, or create it under a first one.
    pub(super) async fn unlock_the_vault(
        &self,
        req: UnlockVaultRequest,
    ) -> Result<UnlockVaultResponse, Status> {
        let passphrase = SecretString::new(req.passphrase);
        let login = self.caller_login(&req.session_token).await?;
        let vaults = self.vaults_to_unlock()?;
        check_passphrase_length(&passphrase)?;
        if req.create {
            check_new_passphrase(&passphrase)?;
        }
        let _turn = self.unlock_throttle.turn().await?;
        self.unlock_throttle.allow(&login)?;
        let (create, subject) = (req.create, login.clone());
        let opened = derive_off_the_executor(move || {
            if create {
                vaults.create(&subject, &passphrase)
            } else {
                vaults.unlock(&subject, &passphrase)
            }
        })
        .await?;
        self.unlock_throttle.record(&login, &opened);
        let unlock =
            opened.map_err(|e| refused_by_the_vault(&login, "open the credential vault", e))?;
        Ok(UnlockVaultResponse {
            vault_state: ProtoVaultState::Open as i32,
            vault_unlock_key: unlock.to_wire(),
        })
    }

    /// `ResetVault`: set the caller's vault aside and create a fresh one under a new passphrase.
    pub(super) async fn reset_the_vault(
        &self,
        req: ResetVaultRequest,
    ) -> Result<ResetVaultResponse, Status> {
        let new_passphrase = SecretString::new(req.new_passphrase);
        let login = self.caller_login(&req.session_token).await?;
        let vaults = self.vaults_to_unlock()?;
        check_passphrase_length(&new_passphrase)?;
        check_new_passphrase(&new_passphrase)?;
        let _turn = self.unlock_throttle.turn().await?;
        let subject = login.clone();
        let reset = derive_off_the_executor(move || vaults.reset(&subject, &new_passphrase))
            .await?
            .map_err(|e| refused_by_the_vault(&login, "reset the credential vault", e))?;
        if let Some(ref aside) = reset.set_aside {
            log::warn!(
                target: "tddy_github::auth_service",
                "the credential vault of '{login}' was reset; the old one is kept at {}",
                aside.display()
            );
        }
        Ok(ResetVaultResponse {
            vault_state: ProtoVaultState::Open as i32,
            vault_unlock_key: reset.unlock_key.to_wire(),
        })
    }

    /// What a logout does to the vault: remove the signing-out lineage's unlock slot — or, for a
    /// lineage that never opened the vault, drop the GitHub token its login left waiting for it.
    pub(super) async fn forget_the_lineage(&self, req: &LogoutRequest) {
        // Signed session tokens are stateless — logout is client-side (the client discards its
        // token). What the daemon does hold is this lineage's unlock slot in the vault, and that
        // is removed. The key itself proves the lineage, so an expired access token does not keep
        // the slot alive.
        let Some(vaults) = self.credential_vaults.as_ref() else {
            return;
        };
        match UnlockKey::from_wire(&req.vault_unlock_key) {
            Some(unlock) => {
                if let Err(e) = vaults.forget(&unlock) {
                    log::warn!(
                        target: "tddy_github::auth_service",
                        "logout of '{}' left its vault unlock slot in place: {e}",
                        unlock.subject()
                    );
                }
            }
            // No key: the vault was closed when this lineage signed in, so its login's live token
            // may still be waiting in memory. The access token says whose; without a valid one
            // there is nobody to drop it for, and it waits for the next unlock or restart.
            None => match self.caller_login(&req.session_token).await {
                Ok(login) => vaults.discard_pending(&login),
                Err(e) => log::debug!(
                    target: "tddy_github::auth_service",
                    "logout presented no unlock key and no valid access token ({}); nothing \
                     waiting for a vault is dropped",
                    e.message()
                ),
            },
        }
    }
}

/// A passphrase short enough to be worth deriving a key from: no person types more than
/// [`MAX_PASSPHRASE_CHARS`], and nobody gets to make the daemon hash a megabyte per guess.
fn check_passphrase_length(passphrase: &SecretString) -> Result<(), Status> {
    if passphrase.expose().chars().count() > MAX_PASSPHRASE_CHARS {
        return Err(Status::invalid_argument(format!(
            "a credential vault passphrase is at most {MAX_PASSPHRASE_CHARS} characters"
        )));
    }
    Ok(())
}

/// A passphrase a vault may be created under — long enough that Argon2id's cost means something.
fn check_new_passphrase(passphrase: &SecretString) -> Result<(), Status> {
    if passphrase.expose().chars().count() < MIN_PASSPHRASE_CHARS {
        return Err(Status::invalid_argument(format!(
            "a credential vault passphrase is at least {MIN_PASSPHRASE_CHARS} characters"
        )));
    }
    Ok(())
}

fn to_proto_state(state: VaultState) -> ProtoVaultState {
    match state {
        VaultState::Open => ProtoVaultState::Open,
        VaultState::Locked => ProtoVaultState::Locked,
        VaultState::Uninitialized => ProtoVaultState::Uninitialized,
    }
}

/// What a login's GitHub token is sealed as. The GitHub login is both the vault's subject and
/// the account, until a second GitHub account per user exists (`#keyring` 8/9).
///
/// A clock before the Unix epoch is refused rather than recorded as `0`: `updated_at` is what the
/// Accounts screen (`#keyring` 4/9) orders and ages records by, and a silent zero would present a
/// fresh credential as the oldest one held.
fn github_record(user: &GitHubUser, access_token: &str) -> Result<CredentialRecord, Status> {
    let updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| {
            log::error!(
                target: "tddy_github::auth_service",
                "the daemon's clock reads before the Unix epoch ({e}); not retaining a GitHub token \
                 with no valid timestamp"
            );
            Status::internal("the daemon's clock is wrong; cannot retain the GitHub access token")
        })?
        .as_secs();
    Ok(CredentialRecord {
        provider: ProviderId::new(GITHUB_PROVIDER),
        account: AccountId::new(&user.login),
        label: if user.name.is_empty() {
            user.login.clone()
        } else {
            user.name.clone()
        },
        secret: SecretString::new(access_token),
        metadata: [
            (GITHUB_ID_METADATA.to_string(), user.id.to_string()),
            (AVATAR_URL_METADATA.to_string(), user.avatar_url.clone()),
        ]
        .into(),
        updated_at,
    })
}

/// The metadata key a GitHub record carries its numeric user id under.
pub const GITHUB_ID_METADATA: &str = "github_id";
/// The metadata key a GitHub record carries its avatar URL under.
pub const AVATAR_URL_METADATA: &str = "avatar_url";

/// The status a vault operation's refusal is reported with. `doing` names the operation, as the
/// client's message does: "could not {doing} for login '…'".
///
/// Only `Io` names server-side detail — the path it could not write, the OS error behind it — so
/// that one goes to the daemon log and the client is told only *what* failed and for which login.
/// Every other refusal is told as it is, because the operator's remedy depends on which it was:
/// `Locked` is a passphrase to retry, `FormatMismatch` is an upgrade.
fn refused_by_the_vault(login: &str, doing: &str, error: VaultError) -> Status {
    match error {
        VaultError::Io(detail) => {
            log::error!(
                target: "tddy_github::auth_service",
                "could not {doing} for login '{login}': {detail}"
            );
            Status::internal(format!("could not {doing} for login '{login}'"))
        }
        refusal => {
            log::warn!(
                target: "tddy_github::auth_service",
                "the credential vault refused to {doing} for '{login}': {refusal}"
            );
            Status::failed_precondition(refusal.to_string())
        }
    }
}
