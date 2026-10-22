//! Who a co-located session's commits are by — one resolution, taken at the session's edge.
//!
//! The pure half ([`git_environment_for`]) turns a project's assignments and what the session's
//! vault holds into the `GIT_*` pairs; the thin half ([`SessionAccountAccess`]) reads the vault.
//! Both the token and the identity come from one `acting_identity` call. The identity half is
//! delivered as the agent's `GIT_*` environment at start; the token is **never** put in an
//! environment variable or a file — [`SessionGithubCredential`] hands it to the agent's tools over
//! the session's own toolcall socket, per call. Nothing in this module reads `GITHUB_TOKEN` or
//! `GH_TOKEN`.

use std::path::Path;
use std::sync::Arc;

use crate::session_git::{session_git_environment, session_git_environment_of};
use tddy_accounts::local_gh::{github_hostname_of, local_gh_git_identity};
use tddy_accounts::{
    acting_identity, AccountStore, AccountsError, ActingIdentity, IdentityError,
    SessionSubjectResolver, SessionVaultAccountStore, PROVIDER_GITHUB,
};
use tddy_core::toolcall::{local_gh_source_value, GITHUB_SOURCE_ENV};
use tddy_core::{read_session_github_source, SessionGithubSource};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SessionVaults};
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
/// behalf of the signed-in person `session_token` belongs to — one `acting_identity` resolution over
/// that person's vault, the same one a session's commit identity and its tools' token come from.
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
        .acting_identity(accounts)
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
    /// The `gh` a Local gh project's identity is asked of; the one on a spawned child's `PATH`
    /// unless [`Self::with_github_cli`] names another.
    github_cli: GithubCli,
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
            github_cli: GithubCli::on_spawn_child_path(),
        }
    }

    /// For a start with no caller token to read a vault with.
    pub fn none() -> Self {
        Self::new(None, Arc::new(|_| None), "")
    }

    /// The account `accounts` name — one `acting_identity` resolution over what the session's vault
    /// holds right now, or the reason there is none.
    pub fn acting_identity(
        &self,
        accounts: &[AccountAssignment],
    ) -> Result<ActingIdentity, SessionIdentityRefusal> {
        acting_identity_for(&assignments_of(accounts), &self.held_github_accounts()?)
            .map_err(SessionIdentityRefusal::Identity)
    }

    /// The model-provider account `account_id` names in the session owner's vault — read the same
    /// way, with the same refusals, as the GitHub identity.
    pub fn provider_account(
        &self,
        account_id: &str,
    ) -> Result<tddy_accounts::ProviderAccount, SessionIdentityRefusal> {
        let wanted = AccountId::new(account_id);
        self.held_accounts()?
            .iter()
            .filter(|record| record.account == wanted)
            .find_map(tddy_accounts::ProviderAccount::from_record)
            .ok_or_else(|| {
                SessionIdentityRefusal::Vault(AccountsError::NotFound {
                    provider: ProviderId::new("provider"),
                    account: wanted,
                })
            })
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
        let vaults = self
            .vaults
            .as_ref()
            .filter(|_| !self.session_token.is_empty())
            .ok_or(SessionIdentityRefusal::NoVaultsConfigured)?;
        SessionVaultAccountStore::new(Arc::clone(vaults), Arc::clone(&self.subject_of))
            .list(&self.session_token)
            .map_err(SessionIdentityRefusal::Vault)
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
