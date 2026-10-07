//! The daemon's half of `#keyring` 8/9: adding a GitHub account without becoming it.
//!
//! `tddy-accounts` owns the flow and its two ports — [`AccountLinker`] and [`LinkedAccountStore`] —
//! and is kept free of the auth stack. This module is where those ports meet it:
//!
//! - [`GitHubAccountLinker`] drives GitHub's OAuth **device flow** through a
//!   [`GitHubOAuthProvider`], the token-exchange half `#keyring` 2/9 already exposes. That trait
//!   has no notion of a session, so nothing here can mint one; login's session-minting half
//!   (`AuthServiceImpl::complete_login`) is not reachable from this file.
//! - [`VaultLinkedAccountStore`] writes the resulting credential into the caller's **open** vault.
//!
//! See `packages/tddy-accounts/docs/account-linking.md`.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use tddy_accounts::{
    deadline_after, forget_expired, AccountLinker, AccountsServiceImpl, LinkChallenge, LinkError,
    LinkProgress, LinkedAccountStore, LinkedIdentity, SessionSubjectResolver,
    SessionVaultAccountStore, META_SUBJECT_ID,
};
use tddy_credentials::{
    AccountId, CredentialRecord, ProviderId, SessionVault, SessionVaults, VaultError, VaultState,
};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_github::provider::{DeviceLoginPoll, DeviceLoginStart};
use tddy_github::{GitHubOAuthProvider, GITHUB_ID_METADATA, GITHUB_PROVIDER};

/// What a person is told when the vault could not be read or written. Deliberately path-free.
const STORE_UNREADABLE: &str = "the credential store could not be read or written on this daemon";

/// Run `future` to completion from inside a synchronous port method.
///
/// The ports are synchronous (`tddy-accounts` is runtime-free) and the provider is async, and the
/// ports are called from inside an RPC handler on the daemon's multi-thread runtime.
/// `block_in_place` hands the worker's other tasks to another thread for the duration, which is
/// what makes blocking here safe; it panics on a current-thread runtime, which no daemon host uses.
fn block_in_runtime<F: Future>(future: F) -> F::Output {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

/// One device flow in progress, held under the daemon's own `link_id`.
struct DeviceAttempt {
    /// GitHub's bearer half of the flow. Never leaves this struct: the operator sees `user_code`,
    /// and the client sees `link_id`.
    device_code: String,
    /// GitHub's current minimum between polls, widened by `slow_down`.
    interval_seconds: u64,
    deadline: Instant,
}

/// [`AccountLinker`] over GitHub's device flow.
pub struct GitHubAccountLinker {
    provider: Arc<dyn GitHubOAuthProvider>,
    attempts: Mutex<HashMap<String, DeviceAttempt>>,
}

impl GitHubAccountLinker {
    #[must_use]
    pub fn new(provider: Arc<dyn GitHubOAuthProvider>) -> Self {
        Self {
            provider,
            attempts: Mutex::new(HashMap::new()),
        }
    }

    fn attempts(&self) -> Result<MutexGuard<'_, HashMap<String, DeviceAttempt>>, LinkError> {
        self.attempts
            .lock()
            .map_err(|_| LinkError::Unavailable("link attempts are unavailable".to_string()))
    }
}

impl AccountLinker for GitHubAccountLinker {
    fn begin(&self, provider: &ProviderId) -> Result<LinkChallenge, LinkError> {
        if provider.as_str() != GITHUB_PROVIDER {
            return Err(LinkError::UnsupportedProvider(
                provider.as_str().to_string(),
            ));
        }
        let DeviceLoginStart {
            device_code,
            user_code,
            verification_uri,
            expires_in_seconds,
            interval_seconds,
        } = block_in_runtime(self.provider.start_device_login()).map_err(LinkError::Unavailable)?;

        let now = Instant::now();
        let deadline = deadline_after(now, expires_in_seconds)?;
        let link_id = uuid::Uuid::new_v4().to_string();
        let mut attempts = self.attempts()?;
        // The service forgets an attempt at its deadline; this keeps the same promise for the
        // device codes it holds, so an abandoned attempt costs neither side memory for long.
        forget_expired(&mut attempts, now, |attempt| attempt.deadline);
        attempts.insert(
            link_id.clone(),
            DeviceAttempt {
                device_code,
                interval_seconds,
                deadline,
            },
        );
        Ok(LinkChallenge {
            link_id,
            user_code,
            verification_uri,
            expires_in_seconds,
            interval_seconds,
        })
    }

    fn poll(&self, link_id: &str) -> Result<LinkProgress, LinkError> {
        let (device_code, interval_seconds) = {
            let attempts = self.attempts()?;
            let attempt = attempts.get(link_id).ok_or(LinkError::NoSuchLink)?;
            (attempt.device_code.clone(), attempt.interval_seconds)
        };
        let polled = block_in_runtime(self.provider.poll_device_login(&device_code))
            .map_err(LinkError::Unavailable)?;
        let mut attempts = self.attempts()?;
        match polled {
            DeviceLoginPoll::Pending => Ok(LinkProgress::Pending { interval_seconds }),
            DeviceLoginPoll::SlowDown { interval_seconds } => {
                if let Some(attempt) = attempts.get_mut(link_id) {
                    attempt.interval_seconds = interval_seconds;
                }
                Ok(LinkProgress::Pending { interval_seconds })
            }
            DeviceLoginPoll::Denied => {
                attempts.remove(link_id);
                Ok(LinkProgress::Denied)
            }
            DeviceLoginPoll::Expired => {
                attempts.remove(link_id);
                Ok(LinkProgress::Expired)
            }
            DeviceLoginPoll::Complete { access_token, user } => {
                attempts.remove(link_id);
                Ok(LinkProgress::Approved {
                    identity: LinkedIdentity {
                        subject_id: user.id.to_string(),
                        login: user.login,
                    },
                    access_token,
                })
            }
        }
    }
}

/// [`LinkedAccountStore`] over the vaults a daemon keeps open, one per signed-in login.
pub struct VaultLinkedAccountStore {
    vaults: Arc<SessionVaults>,
    subject_of: SessionSubjectResolver,
}

impl VaultLinkedAccountStore {
    #[must_use]
    pub fn new(vaults: Arc<SessionVaults>, subject_of: SessionSubjectResolver) -> Self {
        Self { vaults, subject_of }
    }

    fn subject(&self, session_token: &str) -> Result<String, LinkError> {
        (self.subject_of)(session_token).ok_or(LinkError::NoSuchSession)
    }

    /// The caller's open vault. Anything else is [`LinkError::Locked`]: a vault that is sealed, and
    /// one that does not exist yet, are both closed to a link, and both are opened by the same
    /// passphrase prompt — which is what `LINK_VAULT_LOCKED` sends the operator to.
    fn open_vault(&self, subject: &str) -> Result<Arc<SessionVault>, LinkError> {
        match self.vaults.state(subject) {
            VaultState::Open => self.vaults.use_open(subject).ok_or(LinkError::Locked),
            VaultState::Locked | VaultState::Uninitialized => Err(LinkError::Locked),
        }
    }
}

fn refusal_of(subject: &str, error: VaultError) -> LinkError {
    match error {
        VaultError::Locked => LinkError::Locked,
        VaultError::Io(_) => {
            log::error!(
                target: "tddy_daemon::account_linking",
                "credential store for {subject} could not be read or written: {error}"
            );
            LinkError::Unavailable(STORE_UNREADABLE.to_string())
        }
        told => LinkError::Unavailable(told.to_string()),
    }
}

impl LinkedAccountStore for VaultLinkedAccountStore {
    /// What the vault holds at `provider`, with the GitHub user id of a **login-created** record
    /// exposed as [`META_SUBJECT_ID`].
    ///
    /// Login (`#keyring` 2/9) records the id under `github_id`; linking dedups on `subject_id`. Left
    /// unreconciled, linking the very account the session was established with would add a second
    /// record for it. The record is only *presented* with the key — it reaches the vault with it
    /// only when the link re-saves it, which is the re-link doing its job.
    fn held(
        &self,
        session_token: &str,
        provider: &ProviderId,
    ) -> Result<Vec<CredentialRecord>, LinkError> {
        let subject = self.subject(session_token)?;
        let mut records = self
            .open_vault(&subject)?
            .list(Some(provider))
            .map_err(|error| refusal_of(&subject, error))?;
        for record in &mut records {
            if let Some(id) = record.metadata.get(GITHUB_ID_METADATA).cloned() {
                record
                    .metadata
                    .entry(META_SUBJECT_ID.to_string())
                    .or_insert(id);
            }
        }
        Ok(records)
    }

    fn put(&self, session_token: &str, record: CredentialRecord) -> Result<(), LinkError> {
        let subject = self.subject(session_token)?;
        self.open_vault(&subject)?
            .put(record)
            .map_err(|error| refusal_of(&subject, error))
    }

    /// Login stores its credential as `github` / the login name, and a session's subject *is* that
    /// login, so the account a session was established with is derived, not looked up.
    fn session_account(
        &self,
        session_token: &str,
    ) -> Result<Option<(ProviderId, AccountId)>, LinkError> {
        let subject = self.subject(session_token)?;
        let provider = ProviderId::new(GITHUB_PROVIDER);
        let account = AccountId::new(subject.clone());
        // Derived from the session, but only *true* while the vault holds the record: a session
        // whose own account was never stored (or has been removed) is not one this guard protects.
        let holds_it = self
            .open_vault(&subject)?
            .list(Some(&provider))
            .map_err(|error| refusal_of(&subject, error))?
            .iter()
            .any(|record| record.account == account);
        Ok(holds_it.then_some((provider, account)))
    }
}

/// The accounts service over the vaults this daemon keeps, able to link a GitHub account where
/// GitHub can serve a real device flow.
///
/// Linking is wired only then; otherwise the two link RPCs stay unwired and answer
/// `FAILED_PRECONDITION` rather than linking a credential that could never work (a stub's token is
/// synthetic). Nothing is substituted for the missing provider.
///
/// The session token resolves to the GitHub login, which is the vault's subject.
pub fn accounts_service_over(
    vaults: &Arc<SessionVaults>,
    subject_of: SessionSubjectResolver,
    config: &DaemonConfig,
) -> AccountsServiceImpl<SessionVaultAccountStore> {
    let linking_provider = tddy_daemon_auth::github_account_linking_provider(config);
    let service = AccountsServiceImpl::new(Arc::new(SessionVaultAccountStore::new(
        Arc::clone(vaults),
        subject_of.clone(),
    )));
    match linking_provider {
        Some(provider) => service.with_linking(
            Arc::new(GitHubAccountLinker::new(provider)),
            Arc::new(VaultLinkedAccountStore::new(Arc::clone(vaults), subject_of)),
        ),
        None => service,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use async_trait::async_trait;
    use pretty_assertions::assert_eq;
    use tddy_credentials::{CredentialStore, SecretString, FIRST_VERSION};
    use tddy_github::GitHubUser;

    use tddy_accounts::record_for_link;

    use super::*;

    const ADA: &str = "ada";
    const ADAS_GITHUB_ID: &str = "1024";
    const ADAS_SESSION: &str = "session-token-for-ada";
    const ADAS_PASSPHRASE: &str = "correct horse battery staple";

    // ---- the linker -------------------------------------------------------------------------

    /// GitHub's device flow, scripted: polls answer `answers` in order, then `Pending`.
    struct AGitHubThatAnswers {
        answers: Mutex<Vec<DeviceLoginPoll>>,
        window_seconds: u64,
    }

    const GITHUBS_DEVICE_CODE: &str = "github-device-code";
    const GITHUBS_USER_CODE: &str = "WXYZ-1234";
    const A_QUARTER_OF_AN_HOUR: u64 = 900;
    const GITHUBS_POLL_INTERVAL: u64 = 5;

    fn a_github_that_answers(answers: Vec<DeviceLoginPoll>) -> Arc<AGitHubThatAnswers> {
        a_github_whose_codes_last(A_QUARTER_OF_AN_HOUR, answers)
    }

    fn a_github_whose_codes_last(
        window_seconds: u64,
        answers: Vec<DeviceLoginPoll>,
    ) -> Arc<AGitHubThatAnswers> {
        Arc::new(AGitHubThatAnswers {
            answers: Mutex::new(answers.into_iter().rev().collect()),
            window_seconds,
        })
    }

    #[async_trait]
    impl GitHubOAuthProvider for AGitHubThatAnswers {
        fn authorize_url(&self) -> Result<(String, String), String> {
            Err("not used".to_string())
        }

        async fn exchange_code(
            &self,
            _code: &str,
            _state: &str,
        ) -> Result<(String, GitHubUser), String> {
            Err("not used".to_string())
        }

        async fn start_device_login(&self) -> Result<DeviceLoginStart, String> {
            Ok(DeviceLoginStart {
                device_code: GITHUBS_DEVICE_CODE.to_string(),
                user_code: GITHUBS_USER_CODE.to_string(),
                verification_uri: "https://github.com/login/device".to_string(),
                expires_in_seconds: self.window_seconds,
                interval_seconds: GITHUBS_POLL_INTERVAL,
            })
        }

        async fn poll_device_login(&self, _device_code: &str) -> Result<DeviceLoginPoll, String> {
            Ok(self
                .answers
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(DeviceLoginPoll::Pending))
        }

        fn issues_usable_access_token(&self) -> bool {
            true
        }
    }

    fn a_linker_over(answers: Vec<DeviceLoginPoll>) -> GitHubAccountLinker {
        GitHubAccountLinker::new(a_github_that_answers(answers))
    }

    fn github() -> ProviderId {
        ProviderId::new(GITHUB_PROVIDER)
    }

    fn grace_approved_it() -> DeviceLoginPoll {
        DeviceLoginPoll::Complete {
            access_token: "gho_grace".to_string(),
            user: GitHubUser {
                id: 2048,
                login: "grace".to_string(),
                avatar_url: String::new(),
                name: "Grace".to_string(),
            },
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn beginning_hands_out_a_link_id_that_is_not_githubs_device_code() {
        // Given
        let linker = a_linker_over(vec![]);

        // When
        let challenge = linker.begin(&github()).expect("the link begins");

        // Then
        assert_ne!(challenge.link_id, GITHUBS_DEVICE_CODE);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn beginning_shows_the_operator_githubs_user_code() {
        // Given
        let linker = a_linker_over(vec![]);

        // When
        let challenge = linker.begin(&github()).expect("the link begins");

        // Then
        assert_eq!(challenge.user_code, GITHUBS_USER_CODE);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn beginning_reports_githubs_minimum_poll_interval() {
        // Given
        let linker = a_linker_over(vec![]);

        // When
        let challenge = linker.begin(&github()).expect("the link begins");

        // Then
        assert_eq!(challenge.interval_seconds, GITHUBS_POLL_INTERVAL);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_provider_other_than_github_is_unsupported() {
        // Given
        let linker = a_linker_over(vec![]);

        // When
        let begun = linker.begin(&ProviderId::new("cloudflare"));

        // Then
        assert_eq!(
            begun,
            Err(LinkError::UnsupportedProvider("cloudflare".to_string()))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_approval_is_the_github_user_id_as_text_with_the_login_beside_it() {
        // Given
        let linker = a_linker_over(vec![grace_approved_it()]);
        let link_id = linker.begin(&github()).expect("begun").link_id;

        // When
        let polled = linker.poll(&link_id);

        // Then
        assert_eq!(
            polled,
            Ok(LinkProgress::Approved {
                identity: LinkedIdentity {
                    subject_id: "2048".to_string(),
                    login: "grace".to_string(),
                },
                access_token: "gho_grace".to_string(),
            })
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_slow_down_widens_the_interval_the_next_poll_reports() {
        // Given
        let linker = a_linker_over(vec![DeviceLoginPoll::SlowDown {
            interval_seconds: 10,
        }]);
        let link_id = linker.begin(&github()).expect("begun").link_id;
        linker.poll(&link_id).expect("the slow down");

        // When
        let polled = linker.poll(&link_id);

        // Then
        assert_eq!(
            polled,
            Ok(LinkProgress::Pending {
                interval_seconds: 10
            })
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_denied_attempt_is_forgotten() {
        // Given
        let linker = a_linker_over(vec![DeviceLoginPoll::Denied]);
        let link_id = linker.begin(&github()).expect("begun").link_id;
        linker.poll(&link_id).expect("the denial");

        // When
        let polled = linker.poll(&link_id);

        // Then
        assert_eq!(polled, Err(LinkError::NoSuchLink));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_expired_attempt_is_forgotten() {
        // Given
        let linker = a_linker_over(vec![DeviceLoginPoll::Expired]);
        let link_id = linker.begin(&github()).expect("begun").link_id;
        linker.poll(&link_id).expect("the expiry");

        // When
        let polled = linker.poll(&link_id);

        // Then
        assert_eq!(polled, Err(LinkError::NoSuchLink));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_completed_attempt_is_forgotten() {
        // Given
        let linker = a_linker_over(vec![grace_approved_it()]);
        let link_id = linker.begin(&github()).expect("begun").link_id;
        linker.poll(&link_id).expect("the approval");

        // When
        let polled = linker.poll(&link_id);

        // Then
        assert_eq!(polled, Err(LinkError::NoSuchLink));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn beginning_another_link_prunes_an_attempt_whose_code_has_run_out() {
        // Given an attempt whose code lasted no time at all
        let linker = GitHubAccountLinker::new(a_github_whose_codes_last(0, vec![]));
        let abandoned = linker.begin(&github()).expect("begun").link_id;

        // When another begins
        linker.begin(&github()).expect("the next begins");

        // Then the abandoned one is no longer known
        assert_eq!(linker.poll(&abandoned), Err(LinkError::NoSuchLink));
    }

    // ---- the store --------------------------------------------------------------------------

    fn a_login_created_record() -> CredentialRecord {
        CredentialRecord {
            provider: github(),
            account: AccountId::new(ADA),
            label: "Ada".to_string(),
            secret: SecretString::new("gho_ada"),
            metadata: BTreeMap::from([(
                GITHUB_ID_METADATA.to_string(),
                ADAS_GITHUB_ID.to_string(),
            )]),
            updated_at: 1_726_700_000,
            version: FIRST_VERSION,
        }
    }

    fn a_store_over(dir: &Path) -> (Arc<SessionVaults>, VaultLinkedAccountStore) {
        let vaults = Arc::new(SessionVaults::new(dir));
        let subject_of: SessionSubjectResolver =
            Arc::new(|token: &str| (token == ADAS_SESSION).then(|| ADA.to_string()));
        (
            Arc::clone(&vaults),
            VaultLinkedAccountStore::new(vaults, subject_of),
        )
    }

    fn ada_created_her_vault(dir: &Path) {
        let vault = CredentialStore::create(
            &CredentialStore::path_in(dir, ADA),
            &SecretString::new(ADAS_PASSPHRASE),
            ADA,
        )
        .expect("Ada's vault is created");
        vault.put(a_login_created_record()).expect("retained");
    }

    fn a_daemon_where_ada_unlocked_her_vault() -> (tempfile::TempDir, VaultLinkedAccountStore) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        ada_created_her_vault(dir.path());
        let (vaults, store) = a_store_over(dir.path());
        vaults
            .unlock(ADA, &SecretString::new(ADAS_PASSPHRASE))
            .expect("the passphrase opens the vault");
        (dir, store)
    }

    #[test]
    fn a_login_created_record_is_held_under_the_github_user_id_linking_dedups_on() {
        // Given
        let (_dir, store) = a_daemon_where_ada_unlocked_her_vault();

        // When
        let held = store
            .held(ADAS_SESSION, &github())
            .expect("the vault reads");

        // Then
        assert_eq!(
            held.iter()
                .map(|record| record.metadata.get(META_SUBJECT_ID).cloned())
                .collect::<Vec<_>>(),
            vec![Some(ADAS_GITHUB_ID.to_string())]
        );
    }

    #[test]
    fn a_linked_record_is_written_into_the_callers_open_vault() {
        // Given
        let (_dir, store) = a_daemon_where_ada_unlocked_her_vault();
        let mut grace = a_login_created_record();
        grace.account = AccountId::new("github-2048");

        // When
        store.put(ADAS_SESSION, grace).expect("the record is put");

        // Then
        assert_eq!(
            store
                .held(ADAS_SESSION, &github())
                .expect("reads")
                .iter()
                .map(|record| record.account.as_str().to_string())
                .collect::<Vec<_>>(),
            vec!["ada".to_string(), "github-2048".to_string()]
        );
    }

    #[test]
    fn a_vault_that_is_not_open_is_locked_to_a_link() {
        // Given a vault that exists and has not been unlocked since the daemon started
        let dir = tempfile::tempdir().expect("a temporary directory");
        ada_created_her_vault(dir.path());
        let (_vaults, store) = a_store_over(dir.path());

        // When
        let put = store.put(ADAS_SESSION, a_login_created_record());

        // Then
        assert_eq!(put, Err(LinkError::Locked));
    }

    #[test]
    fn a_token_no_session_owns_names_no_session() {
        // Given
        let (_dir, store) = a_daemon_where_ada_unlocked_her_vault();

        // When
        let held = store.held("a-token-nobody-minted", &github());

        // Then
        assert_eq!(held, Err(LinkError::NoSuchSession));
    }

    #[test]
    fn a_vault_that_does_not_exist_yet_is_locked_to_a_link() {
        // Given a daemon where Ada never created a vault
        let dir = tempfile::tempdir().expect("a temporary directory");
        let (_vaults, store) = a_store_over(dir.path());

        // When
        let put = store.put(ADAS_SESSION, a_login_created_record());

        // Then
        assert_eq!(put, Err(LinkError::Locked));
    }

    #[test]
    fn a_vault_that_cannot_be_read_or_written_is_unavailable_without_naming_a_path() {
        // When
        let refusal = refusal_of(
            ADA,
            VaultError::Io("/var/tddy/ada.vault: disk full".to_string()),
        );

        // Then
        assert_eq!(
            refusal,
            LinkError::Unavailable(STORE_UNREADABLE.to_string())
        );
    }

    #[test]
    fn linking_the_account_a_login_created_updates_that_record_in_place() {
        // Given Ada's login-created record, keyed by `github_id` rather than `subject_id`
        let (_dir, store) = a_daemon_where_ada_unlocked_her_vault();
        let ada_again = LinkedIdentity {
            subject_id: ADAS_GITHUB_ID.to_string(),
            login: ADA.to_string(),
        };
        let held = store.held(ADAS_SESSION, &github()).expect("reads");

        // When she links the same GitHub account
        let relinked =
            record_for_link(&held, &github(), &ada_again, "gho_ada_fresh", 1_726_800_000);
        store
            .put(ADAS_SESSION, relinked)
            .expect("the record is put");

        // Then the vault holds one record for her, not two
        assert_eq!(
            store
                .held(ADAS_SESSION, &github())
                .expect("reads")
                .iter()
                .map(|record| record.account.as_str().to_string())
                .collect::<Vec<_>>(),
            vec![ADA.to_string()]
        );
    }

    #[test]
    fn the_session_account_is_none_when_the_vault_does_not_hold_it() {
        // Given Ada's vault is open and holds nothing
        let dir = tempfile::tempdir().expect("a temporary directory");
        CredentialStore::create(
            &CredentialStore::path_in(dir.path(), ADA),
            &SecretString::new(ADAS_PASSPHRASE),
            ADA,
        )
        .expect("Ada's vault is created");
        let (vaults, store) = a_store_over(dir.path());
        vaults
            .unlock(ADA, &SecretString::new(ADAS_PASSPHRASE))
            .expect("the passphrase opens the vault");

        // When
        let account = store.session_account(ADAS_SESSION);

        // Then
        assert_eq!(account, Ok(None));
    }

    #[test]
    fn the_session_account_is_the_one_login_stored_under_the_login_name() {
        // Given
        let (_dir, store) = a_daemon_where_ada_unlocked_her_vault();

        // When
        let account = store.session_account(ADAS_SESSION);

        // Then
        assert_eq!(account, Ok(Some((github(), AccountId::new(ADA)))));
    }
}
