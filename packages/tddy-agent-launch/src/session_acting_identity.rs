//! Who a co-located session's commits are by — one resolution, taken at the session's edge.
//!
//! The pure half ([`git_environment_for`]) turns a project's assignments and what the session's
//! vault holds into the `GIT_*` pairs; the thin half ([`SessionAccountAccess`]) reads the vault.
//! Both the token and the identity come from one `acting_identity` call. The identity half is
//! delivered as the agent's `GIT_*` environment at start; the token is **never** put in an
//! environment variable or a file — [`SessionGithubCredential`] hands it to the agent's tools over
//! the session's own toolcall socket, per call. Nothing in this module reads `GITHUB_TOKEN` or
//! `GH_TOKEN`.
//!
//! Two resolutions, by use: commits and pushes take the paired one, and a record that carries no
//! provider identifiers has its missing half derived — GitHub is asked who the token belongs to —
//! and stamped back into the vault, so only a derivation that fails refuses. Reads and PR-API
//! calls take the token-only one ([`SessionAccountAccess::acting_token`]), which never derives
//! and never looks at the metadata at all.

use std::path::Path;
use std::sync::Arc;

use crate::provider_account_lookup::find_provider_account;
use crate::session_git::{session_git_environment, session_git_environment_of};
use tddy_accounts::local_gh::{github_hostname_of, local_gh_git_identity};
use tddy_accounts::{
    acting_identity, acting_token, git_identity_of, AccountStore, AccountsError, ActingIdentity,
    ActingToken, IdentityError, LinkedIdentity, SessionSubjectResolver, SessionVaultAccountStore,
    PROVIDER_GITHUB,
};
use tddy_core::toolcall::{local_gh_source_value, GITHUB_SOURCE_ENV};
use tddy_core::{read_session_github_source, SessionGithubSource};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SessionVaults};
use tddy_github::github_rest_common::GITHUB_API_BASE;
use tddy_github::user_api::fetch_authenticated_user;
use tddy_host_service::GithubCli;
use tddy_projects::project_storage::{AccountAssignment, ProjectData};

/// A session's answer to its tools' `github-token`, shared between the listener's connections.
pub(crate) type SharedGithubCredential = Arc<dyn tddy_core::toolcall::GithubCredentialHandler>;

/// Why a session has no account identity to commit under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionIdentityRefusal {
    /// This daemon has no credential vaults wired, or the start carried no session token.
    NoVaultsConfigured,
    /// The session's vault could not be read.
    Vault(AccountsError),
    /// The vault was read and the project does not resolve to a usable account.
    Identity(IdentityError),
    /// More than one provider account carries the label a start named it by.
    AmbiguousLabel(String),
}

impl std::fmt::Display for SessionIdentityRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoVaultsConfigured => f.write_str(
                "no credential vault is available to this session start, so no account can be resolved",
            ),
            Self::Vault(AccountsError::NoSuchSession) => {
                f.write_str("the session token names no live session, so its vault cannot be opened")
            }
            Self::Vault(AccountsError::Locked) => {
                f.write_str("the signed-in person's credential vault is locked on this daemon")
            }
            Self::Vault(AccountsError::Uninitialized) => {
                f.write_str("the signed-in person has no credential vault yet")
            }
            Self::Vault(AccountsError::NotFound { .. }) => {
                f.write_str("the credential vault holds no such account")
            }
            Self::Vault(AccountsError::Unavailable(reason)) => {
                write!(f, "the credential vault could not be read: {reason}")
            }
            Self::Identity(error) => error.fmt(f),
            Self::AmbiguousLabel(label) => write!(
                f,
                "more than one provider account is labelled {label:?}: name it by its id"
            ),
        }
    }
}

/// The project row's assignments as the resolver takes them.
pub(crate) fn assignments_of(accounts: &[AccountAssignment]) -> Vec<(ProviderId, AccountId)> {
    accounts
        .iter()
        .map(|a| (ProviderId::new(&a.provider), AccountId::new(&a.account_id)))
        .collect()
}

/// The GitHub account `assignments` name, resolved once over `held` — token and identity together.
pub(crate) fn acting_identity_for(
    assignments: &[(ProviderId, AccountId)],
    held: &[CredentialRecord],
) -> Result<ActingIdentity, IdentityError> {
    acting_identity(assignments, &ProviderId::new(PROVIDER_GITHUB), held)
}

/// The GitHub account `assignments` name, resolved token-only over `held` — the account and its
/// token, never the metadata.
fn acting_token_for(
    assignments: &[(ProviderId, AccountId)],
    held: &[CredentialRecord],
) -> Result<ActingToken, IdentityError> {
    acting_token(assignments, &ProviderId::new(PROVIDER_GITHUB), held)
}

/// Ask GitHub who `token` belongs to — the question a missing commit identity is derived from.
///
/// A value, defaulting to the real `GET /user` and overridable per access
/// ([`SessionAccountAccess::with_user_probe`]) so tests stand a fake in for GitHub. The shape
/// mirrors [`SessionSubjectResolver`]: what varies is where the answer comes from, not what is
/// asked.
pub type AccountIdentityProbe = Arc<dyn Fn(&str) -> Result<LinkedIdentity, String> + Send + Sync>;

/// The production probe: `GET /user` at GitHub's own API root, asked with the account's token.
fn default_account_identity_probe(token: &str) -> Result<LinkedIdentity, String> {
    fetch_authenticated_user(GITHUB_API_BASE, token).map(|user| LinkedIdentity {
        subject_id: user.id.to_string(),
        login: user.login,
    })
}

/// The `GIT_*` pairs of the GitHub account `assignments` name, from one `acting_identity`
/// resolution over `held` — the pure half the unit tests drive without a vault.
#[cfg(test)]
pub(crate) fn git_environment_for(
    assignments: &[(ProviderId, AccountId)],
    held: &[CredentialRecord],
) -> Result<Vec<(String, String)>, IdentityError> {
    Ok(session_git_environment(&acting_identity_for(
        assignments,
        held,
    )?))
}

/// The token of the GitHub account `accounts` name, for an operation the daemon performs itself on
/// behalf of the signed-in person `session_token` belongs to — one token-only resolution over
/// that person's vault.
///
/// Reads and PR-API calls authenticate as the account and author nothing, so this never looks at
/// the record's provider metadata and never derives: a record linked before `subject_id` existed
/// still yields its token.
///
/// `Err` carries the reason there is none, in the resolver's own words. The process environment is
/// not consulted: with no resolvable account there is no token.
pub fn project_github_token(
    vaults: Option<Arc<SessionVaults>>,
    subject_of: SessionSubjectResolver,
    session_token: &str,
    accounts: &[AccountAssignment],
) -> Result<String, String> {
    SessionAccountAccess::new(vaults, subject_of, session_token)
        .acting_token(accounts)
        .map(|acting| acting.token)
        .map_err(|refusal| refusal.to_string())
}

/// What a session start may read to learn who it acts as: the daemon's vaults, how a session token
/// names its owner, and the token itself.
#[derive(Clone)]
pub struct SessionAccountAccess {
    vaults: Option<Arc<SessionVaults>>,
    subject_of: SessionSubjectResolver,
    session_token: String,
    /// What the vault is looked up by, through `subject_of`: the session token, or — for access
    /// built [`Self::for_subject`] — the subject itself. Empty means there is nothing to look up by.
    vault_key: String,
    /// The `gh` a Local gh project's identity is asked of; the one on a spawned child's `PATH`
    /// unless [`Self::with_github_cli`] names another.
    github_cli: GithubCli,
    /// Who a missing commit identity is asked of; the real `GET /user` unless
    /// [`Self::with_user_probe`] names another. Asked only when a record the vault holds carries
    /// no provider identifiers — never for a read, never when they are present.
    user_probe: AccountIdentityProbe,
}

impl SessionAccountAccess {
    pub fn new(
        vaults: Option<Arc<SessionVaults>>,
        subject_of: SessionSubjectResolver,
        session_token: &str,
    ) -> Self {
        Self {
            vaults,
            subject_of,
            session_token: session_token.to_string(),
            vault_key: session_token.to_string(),
            github_cli: GithubCli::on_spawn_child_path(),
            user_probe: Arc::new(default_account_identity_probe),
        }
    }

    /// Access to the vault of the signed-in person `subject` names, for a start the daemon makes on
    /// their behalf with no session token in hand: the vault is found by the subject directly, and
    /// [`Self::session_token`] stays empty, as it is for [`Self::none`].
    pub fn for_subject(vaults: Option<Arc<SessionVaults>>, subject: &str) -> Self {
        let owner = subject.to_string();
        Self {
            vault_key: subject.to_string(),
            ..Self::new(vaults, Arc::new(move |_| Some(owner.clone())), "")
        }
    }

    /// For a start with no caller token to read a vault with.
    pub fn none() -> Self {
        Self::new(None, Arc::new(|_| None), "")
    }

    /// The token the session this access was built for started with; empty for [`Self::none`].
    /// What a start the session asks for authenticates as.
    pub fn session_token(&self) -> &str {
        &self.session_token
    }

    /// The account `accounts` name — one `acting_identity` resolution over what the session's vault
    /// holds right now, or the reason there is none.
    ///
    /// The paired resolution, for commits and pushes. When the record carries no provider
    /// identifiers, the missing half is derived — the probe asks GitHub who the token belongs
    /// to — and stamped back into the vault record, so the next resolution finds it. Only a
    /// derivation that fails refuses, as [`IdentityError::DerivationFailed`]; a stamp that fails
    /// is warned about and the derived identity still answers, because the commit is correctly
    /// attributed either way.
    pub fn acting_identity(
        &self,
        accounts: &[AccountAssignment],
    ) -> Result<ActingIdentity, SessionIdentityRefusal> {
        let assignments = assignments_of(accounts);
        let held = self.held_github_accounts()?;
        match acting_identity_for(&assignments, &held) {
            Err(IdentityError::Unusable {
                provider,
                account,
                missing,
            }) => self.derived_identity(&provider, &account, missing),
            resolved => resolved.map_err(SessionIdentityRefusal::Identity),
        }
    }

    /// The account `accounts` name and its token — one token-only resolution over what the
    /// session's vault holds right now, or the reason there is none.
    ///
    /// For reads and PR-API calls, which authenticate as the account and author nothing: this
    /// never looks at the record's provider metadata and never derives — a record linked before
    /// `subject_id` existed still yields its token.
    pub fn acting_token(
        &self,
        accounts: &[AccountAssignment],
    ) -> Result<ActingToken, SessionIdentityRefusal> {
        acting_token_for(&assignments_of(accounts), &self.held_github_accounts()?)
            .map_err(SessionIdentityRefusal::Identity)
    }

    /// The paired identity for a record the vault holds that carries no provider identifiers:
    /// derive them from the token, stamp them back, and answer paired.
    ///
    /// The token half is resolved token-only over the vault as it stands — the same account the
    /// pure resolution just chose — so the derived identity and the token cannot name different
    /// accounts.
    fn derived_identity(
        &self,
        provider: &ProviderId,
        account: &AccountId,
        missing: &'static str,
    ) -> Result<ActingIdentity, SessionIdentityRefusal> {
        let assignments = vec![(provider.clone(), account.clone())];
        let held = self.held_github_accounts()?;
        let token =
            acting_token_for(&assignments, &held).map_err(SessionIdentityRefusal::Identity)?;
        let probed = (self.user_probe)(&token.token).map_err(|reason| {
            SessionIdentityRefusal::Identity(IdentityError::DerivationFailed {
                provider: provider.clone(),
                account: account.clone(),
                missing,
                reason,
            })
        })?;
        let acting = ActingIdentity {
            account: token.account,
            token: token.token,
            git: git_identity_of(&probed.subject_id, &probed.login),
        };
        if let Err(error) = self.vault_store()?.stamp_account_identity(
            &self.vault_key,
            provider,
            &acting.account,
            &probed,
        ) {
            log::warn!(
                target: "tddy_daemon::connection_service",
                "the GitHub identity derived for account {} was not persisted: {error:?}; the \
                 commit proceeds under it anyway",
                acting.account.as_str()
            );
        }
        Ok(acting)
    }

    /// The model-provider account `id_or_label` names in the session owner's vault — read the same
    /// way, with the same refusals, as the GitHub identity. See [`find_provider_account`].
    pub fn provider_account(
        &self,
        id_or_label: &str,
    ) -> Result<tddy_accounts::ProviderAccount, SessionIdentityRefusal> {
        find_provider_account(&self.held_accounts()?, id_or_label)
    }

    /// The provider account the session owner made their default for `harness`, if any — the
    /// `user_default` level of a session's route.
    pub fn default_provider_account(
        &self,
        harness: &str,
    ) -> Result<Option<String>, SessionIdentityRefusal> {
        let held = match self.held_accounts() {
            Ok(held) => held,
            // Nothing to read a default from: no vault is wired, or the person has none yet.
            Err(SessionIdentityRefusal::NoVaultsConfigured)
            | Err(SessionIdentityRefusal::Vault(AccountsError::Uninitialized)) => return Ok(None),
            Err(refusal) => return Err(refusal),
        };
        Ok(tddy_accounts::default_account_for(&held, harness)
            .map(|account| account.as_str().to_string()))
    }

    /// The GitHub records the session's vault holds right now.
    fn held_github_accounts(&self) -> Result<Vec<CredentialRecord>, SessionIdentityRefusal> {
        Ok(self
            .held_accounts()?
            .into_iter()
            .filter(|record| record.provider.as_str() == PROVIDER_GITHUB)
            .collect())
    }

    /// Every record the session owner's vault holds right now.
    fn held_accounts(&self) -> Result<Vec<CredentialRecord>, SessionIdentityRefusal> {
        self.vault_store()?
            .list(&self.vault_key)
            .map_err(SessionIdentityRefusal::Vault)
    }

    /// The session owner's vault as the account store — the same gate the reads go through, for
    /// the writes derivation persists.
    fn vault_store(&self) -> Result<SessionVaultAccountStore, SessionIdentityRefusal> {
        let vaults = self
            .vaults
            .as_ref()
            .filter(|_| !self.vault_key.is_empty())
            .ok_or(SessionIdentityRefusal::NoVaultsConfigured)?;
        Ok(SessionVaultAccountStore::new(
            Arc::clone(vaults),
            Arc::clone(&self.subject_of),
        ))
    }
}

/// What a session's start or resume binds from one lookup of its project: the commit pairs its
/// agent's host-side commands run under, and the handler its tools' `github-token` request is
/// answered by.
pub struct SessionIdentity {
    /// Empty when the project could not be read or does not resolve (logged), so the checkout's
    /// own identity stays in force.
    pub git_environment: Vec<(String, String)>,
    /// `None` only when the project could not be read; a project that does not resolve still binds
    /// a handler, which refuses each request with the resolver's own message.
    pub github_credential: Option<SharedGithubCredential>,
    /// The GitHub host a **Local gh** session asks `gh` about; `None` for a vault-backed session.
    /// Set even when `gh` could not answer at start, so the steps that later need a token fail in
    /// `gh`'s own words.
    pub local_gh_hostname: Option<String>,
    /// Whether the session's GitHub identity is the host's `gh` login. Independent of the
    /// hostname: a project whose git URL names no host is Local gh with no hostname, and its
    /// relay must still carry a logged-out `gh`.
    pub is_local_gh: bool,
}

/// Where a project's sessions take their GitHub identity from, as the session start resolved it.
#[derive(Debug, Clone, Copy)]
pub enum ProjectGithub<'a> {
    /// The project's account assignments, resolved over the signed-in person's vault.
    Vault(&'a [AccountAssignment]),
    /// The host's own `gh` login, asked as `os_user` about the GitHub host `git_url` names.
    LocalGh { os_user: &'a str, git_url: &'a str },
}

/// Who runs the host-side commands a session's identity is applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayUser {
    /// The session's own OS user — a co-located agent, or a daemon that runs as that user.
    SessionUser,
    /// Another OS user: the daemon's, relaying a sandboxed session's commands on a multi-user host.
    OtherUser,
}

impl SessionIdentity {
    /// The environment the host-side relay of a sandboxed session runs its commands under, when
    /// that relay runs as `relay`.
    ///
    /// A Local gh identity is the **session user's** `gh` login. A relay running as another user
    /// would reach that user's `gh` instead, so it carries no commit identity and only
    /// `GH_CONFIG_DIR=<logged_out_gh_config>`: `gh` there is logged out rather than the wrong
    /// account. A vault identity carries no credential and is relayed unchanged.
    pub fn relayed_by(
        self,
        relay: RelayUser,
        logged_out_gh_config: &Path,
    ) -> Vec<(String, String)> {
        match (self.is_local_gh, relay) {
            (true, RelayUser::OtherUser) => vec![(
                "GH_CONFIG_DIR".to_string(),
                logged_out_gh_config.display().to_string(),
            )],
            _ => self.git_environment,
        }
    }
}

/// The token of the host's `gh` login, asked as `os_user` about the GitHub host `git_url` names —
/// for an operation the daemon performs itself on behalf of a Local gh session (re-pointing a
/// planned PR). `Err` carries the reason there is none, in `gh`'s or the resolver's own words.
pub fn local_gh_github_token(
    github_cli: &GithubCli,
    os_user: &str,
    git_url: &str,
) -> Result<String, String> {
    let hostname = github_hostname_of(git_url).map_err(|e| e.to_string())?;
    github_cli
        .auth_token(os_user, &hostname)
        .map_err(|refusal| refusal.to_string())
}

impl SessionAccountAccess {
    /// The identity a session is launched with, over `accounts` — the project's assignments, or
    /// `None` when there is no project row to read them from (nothing is resolved then: no pairs,
    /// no handler). Both halves come from the same assignments, read once by the caller.
    pub fn session_identity(
        &self,
        session_id: &str,
        accounts: Option<&[AccountAssignment]>,
    ) -> SessionIdentity {
        let Some(accounts) = accounts else {
            return SessionIdentity {
                git_environment: Vec::new(),
                github_credential: None,
                local_gh_hostname: None,
                is_local_gh: false,
            };
        };
        // The one resolution of this start: the commit pairs and the handler both derive from its
        // outcome, so neither can name an account the other does not.
        let (git_environment, pinned) = match self.acting_identity(accounts) {
            Ok(acting) => (
                session_git_environment(&acting),
                PinnedAccount::Acting(acting.account),
            ),
            Err(refusal) => {
                // A refused project starts with the checkout's inherited commit identity.
                // Developer-consented (changeset, M2 "a refused resolution does not stop the
                // session") so unassigned projects keep working; the handler stays refused with the
                // same reason, so no token is handed out for commits that are not the account's.
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "session {session_id} starts without an account identity: {refusal}"
                );
                (Vec::new(), PinnedAccount::Refused(refusal.to_string()))
            }
        };
        SessionIdentity {
            git_environment,
            github_credential: Some(Arc::new(SessionGithubCredential {
                access: self.clone(),
                pinned,
            })),
            local_gh_hostname: None,
            is_local_gh: false,
        }
    }

    /// Ask `github_cli` for a Local gh project's identity (builder).
    pub fn with_github_cli(mut self, github_cli: GithubCli) -> Self {
        self.github_cli = github_cli;
        self
    }

    /// Ask `probe` who a token belongs to when a commit identity must be derived (builder;
    /// tests). Production asks the real `GET /user`.
    pub fn with_user_probe(mut self, probe: AccountIdentityProbe) -> Self {
        self.user_probe = probe;
        self
    }

    /// The identity a session is launched with, from wherever its project takes its GitHub identity
    /// from — `None` when there is no project row to read (nothing is resolved then: no pairs, no
    /// handler).
    ///
    /// For [`ProjectGithub::LocalGh`]: the `gh` login's four commit pairs plus `TDDY_GITHUB_SOURCE`
    /// naming the GitHub host, and a handler that never hands out a token — a session's tools read
    /// theirs from `gh`. When `gh` cannot answer, the source is still named, no pairs are added and
    /// the reason is logged; when the git URL names no host, neither is added.
    pub fn project_session_identity(
        &self,
        session_id: &str,
        project: Option<ProjectGithub<'_>>,
    ) -> SessionIdentity {
        match project {
            None => self.session_identity(session_id, None),
            Some(ProjectGithub::Vault(accounts)) => {
                self.session_identity(session_id, Some(accounts))
            }
            Some(ProjectGithub::LocalGh { os_user, git_url }) => {
                self.local_gh_identity(session_id, os_user, git_url)
            }
        }
    }

    /// The identity a session is launched with, over `project` — this host's row of the session's
    /// project, `None` when it could not be read (nothing is resolved then: no pairs, no handler).
    ///
    /// Which source the session uses is the one recorded in `session_dir` (a start that named one,
    /// kept for its resumes), otherwise the row's: Local gh when the row says so, the vault
    /// assignment otherwise. A record that cannot be read is logged and resolves nothing: the
    /// session never silently takes the row's source in place of the one it was started with.
    pub fn session_identity_in(
        &self,
        session_id: &str,
        os_user: &str,
        session_dir: &Path,
        project: Option<&ProjectData>,
    ) -> SessionIdentity {
        let Some(project) = project else {
            return self.project_session_identity(session_id, None);
        };
        let source = match read_session_github_source(session_dir) {
            Ok(recorded) => recorded.unwrap_or(if project.local_gh {
                SessionGithubSource::LocalGh
            } else {
                SessionGithubSource::Vault
            }),
            Err(error) => {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "session {session_id} has no account identity: its recorded GitHub source \
                     could not be read: {error}"
                );
                return self.project_session_identity(session_id, None);
            }
        };
        self.project_session_identity(
            session_id,
            Some(match source {
                SessionGithubSource::Vault => ProjectGithub::Vault(&project.accounts),
                SessionGithubSource::LocalGh => ProjectGithub::LocalGh {
                    os_user,
                    git_url: &project.git_url,
                },
            }),
        )
    }

    fn local_gh_identity(&self, session_id: &str, os_user: &str, git_url: &str) -> SessionIdentity {
        let github_credential: Option<SharedGithubCredential> = Some(Arc::new(LocalGhCredential));
        let hostname = match github_hostname_of(git_url) {
            Ok(hostname) => hostname,
            Err(error) => {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "session {session_id} starts without a local gh identity: {error}"
                );
                return SessionIdentity {
                    git_environment: Vec::new(),
                    github_credential,
                    local_gh_hostname: None,
                    is_local_gh: true,
                };
            }
        };
        let mut git_environment = match self.github_cli.logged_in_user(os_user, &hostname) {
            Ok(user) => session_git_environment_of(&local_gh_git_identity(&user.login, user.id)),
            Err(refusal) => {
                // The session still starts, under the checkout's own commit identity, and is still
                // told its source: a later step that needs a token then fails in `gh`'s words.
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "session {session_id} starts without a local gh identity: {refusal}"
                );
                Vec::new()
            }
        };
        git_environment.push((
            GITHUB_SOURCE_ENV.to_string(),
            local_gh_source_value(&hostname),
        ));
        SessionIdentity {
            git_environment,
            github_credential,
            local_gh_hostname: Some(hostname),
            is_local_gh: true,
        }
    }
}

/// The `github-token` answer of a Local gh session's host: a refusal. The token is `gh`'s, read by
/// the process that needs it, as the session's OS user; it never crosses the session's socket.
struct LocalGhCredential;

#[async_trait::async_trait]
impl tddy_core::toolcall::GithubCredentialHandler for LocalGhCredential {
    async fn github_token(&self) -> Result<String, String> {
        Err(
            "this session's project uses local gh: its GitHub token is read from the host's gh \
             login by the process that needs it, and is never handed out by the session host"
                .to_string(),
        )
    }
}

/// What a session's start decided about the account it acts as.
enum PinnedAccount {
    /// The start resolved this account; every later request fetches **this** record.
    Acting(AccountId),
    /// The start was refused, with this message; so is every later request, until a resume resolves
    /// again.
    Refused(String),
}

/// The token half of a session's resolution, answered to the agent's tools over the session's own
/// toolcall socket.
///
/// **Pinned to the outcome of the start's resolution** — the one the commit pairs came from. If the
/// start resolved an account, each call fetches that account's record, by id, from the owner's
/// vault; it can never answer with another account's token, whatever the project's assignments or
/// the vault's contents become. If the start was refused, each call repeats that refusal: a session
/// whose commits are not attributed to an account is never given that account's token. The vault is
/// read **per call** (a vault locked since, or a signed-out owner, refuses rather than being served
/// from a copy); the token is returned to the caller and kept nowhere.
pub(crate) struct SessionGithubCredential {
    access: SessionAccountAccess,
    pinned: PinnedAccount,
}

#[async_trait::async_trait]
impl tddy_core::toolcall::GithubCredentialHandler for SessionGithubCredential {
    async fn github_token(&self) -> Result<String, String> {
        let refuse = |reason: String| {
            // The refusal is the agent's to read; the token never reaches a log line.
            log::warn!(
                target: "tddy_daemon::connection_service",
                "a session's GitHub token was refused: {reason}"
            );
            reason
        };
        let account = match &self.pinned {
            PinnedAccount::Acting(account) => account,
            PinnedAccount::Refused(reason) => return Err(refuse(reason.clone())),
        };
        let held = self
            .access
            .held_github_accounts()
            .map_err(|refusal| refuse(refusal.to_string()))?;
        // The same resolution function the pairs came from, over the one assignment the start chose.
        acting_identity_for(
            &[(ProviderId::new(PROVIDER_GITHUB), account.clone())],
            &held,
        )
        .map(|acting| acting.token)
        .map_err(|error| refuse(error.to_string()))
    }
}
