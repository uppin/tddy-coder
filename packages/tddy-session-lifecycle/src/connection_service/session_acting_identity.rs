//! Who a co-located session's commits are by — one resolution, taken at the session's edge.
//!
//! The pure half ([`git_environment_for`]) turns a project's assignments and what the session's
//! vault holds into the `GIT_*` pairs; the thin half ([`SessionAccountAccess`]) reads the vault.
//! Both the token and the identity come from one `acting_identity` call. The identity half is
//! delivered as the agent's `GIT_*` environment at start; the token is **never** put in an
//! environment variable or a file — [`SessionGithubCredential`] hands it to the agent's tools over
//! the session's own toolcall socket, per call. Nothing in this module reads `GITHUB_TOKEN` or
//! `GH_TOKEN`.

use std::sync::Arc;

use tddy_accounts::{
    acting_identity, AccountStore, AccountsError, ActingIdentity, IdentityError,
    SessionSubjectResolver, SessionVaultAccountStore, PROVIDER_GITHUB,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SessionVaults};
use tddy_daemon_livekit::session_git::session_git_environment;
use tddy_projects::project_storage::AccountAssignment;

/// A session's answer to its tools' `github-token`, shared between the listener's connections.
pub(crate) type SharedGithubCredential = Arc<dyn tddy_core::toolcall::GithubCredentialHandler>;

/// Why a session has no account identity to commit under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionIdentityRefusal {
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
pub(crate) struct SessionAccountAccess {
    vaults: Option<Arc<SessionVaults>>,
    subject_of: SessionSubjectResolver,
    session_token: String,
}

impl SessionAccountAccess {
    pub(crate) fn new(
        vaults: Option<Arc<SessionVaults>>,
        subject_of: SessionSubjectResolver,
        session_token: &str,
    ) -> Self {
        Self {
            vaults,
            subject_of,
            session_token: session_token.to_string(),
        }
    }

    /// For a start with no caller token to read a vault with.
    pub(crate) fn none() -> Self {
        Self::new(None, Arc::new(|_| None), "")
    }

    /// The account `accounts` name — one `acting_identity` resolution over what the session's vault
    /// holds right now, or the reason there is none.
    pub(crate) fn acting_identity(
        &self,
        accounts: &[AccountAssignment],
    ) -> Result<ActingIdentity, SessionIdentityRefusal> {
        acting_identity_for(&assignments_of(accounts), &self.held_github_accounts()?)
            .map_err(SessionIdentityRefusal::Identity)
    }

    /// The GitHub records the session's vault holds right now.
    fn held_github_accounts(&self) -> Result<Vec<CredentialRecord>, SessionIdentityRefusal> {
        let vaults = self
            .vaults
            .as_ref()
            .filter(|_| !self.session_token.is_empty())
            .ok_or(SessionIdentityRefusal::NoVaultsConfigured)?;
        let held = SessionVaultAccountStore::new(Arc::clone(vaults), Arc::clone(&self.subject_of))
            .list(&self.session_token)
            .map_err(SessionIdentityRefusal::Vault)?;
        Ok(held
            .into_iter()
            .filter(|record| record.provider.as_str() == PROVIDER_GITHUB)
            .collect())
    }
}

/// What a session's start or resume binds from one lookup of its project: the commit pairs its
/// agent's host-side commands run under, and the handler its tools' `github-token` request is
/// answered by.
pub(crate) struct SessionIdentity {
    /// Empty when the project could not be read or does not resolve (logged), so the checkout's
    /// own identity stays in force.
    pub(crate) git_environment: Vec<(String, String)>,
    /// `None` only when the project could not be read; a project that does not resolve still binds
    /// a handler, which refuses each request with the resolver's own message.
    pub(crate) github_credential: Option<SharedGithubCredential>,
}

impl SessionAccountAccess {
    /// The identity a session is launched with, over `accounts` — the project's assignments, or
    /// `None` when there is no project row to read them from (nothing is resolved then: no pairs,
    /// no handler). Both halves come from the same assignments, read once by the caller.
    pub(crate) fn session_identity(
        &self,
        session_id: &str,
        accounts: Option<&[AccountAssignment]>,
    ) -> SessionIdentity {
        let Some(accounts) = accounts else {
            return SessionIdentity {
                git_environment: Vec::new(),
                github_credential: None,
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
        }
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

impl super::DaemonSessionHost {
    /// The accounts `project_id` assigns, or `None` — logged — when its row cannot be read.
    pub(crate) fn project_account_assignments(
        &self,
        os_user: &str,
        session_id: &str,
        project_id: &str,
    ) -> Option<Vec<AccountAssignment>> {
        // A client-supplied checkout belongs to no project; there is nothing to read and nothing
        // wrong, so it is not a refusal worth a warning.
        if project_id.trim().is_empty() {
            return None;
        }
        match super::service_util::find_registered_project(&self.tddy_data_dir, os_user, project_id)
        {
            Ok((_, project)) => Some(project.accounts),
            Err(status) => {
                log::warn!(
                    target: "tddy_daemon::connection_service",
                    "session {session_id} has no account identity: its project could not be \
                     read: {}",
                    status.message()
                );
                None
            }
        }
    }

    /// What the session's vault reads go through: this daemon's vaults, how a session token names
    /// its owner, and the token the session was started with.
    pub(crate) fn session_account_access(&self, session_token: &str) -> SessionAccountAccess {
        SessionAccountAccess::new(
            self.credential_vaults(),
            self.user_resolver(),
            session_token,
        )
    }

    /// The identity a session of `project_id` is launched with: its commit pairs and the handler
    /// that answers its tools' `github-token`, over the project's assignments as they stand now.
    /// A project that cannot be read — logged — yields neither.
    pub(crate) fn session_identity(
        &self,
        os_user: &str,
        session_id: &str,
        project_id: &str,
        session_token: &str,
    ) -> SessionIdentity {
        let accounts = self.project_account_assignments(os_user, session_id, project_id);
        self.session_account_access(session_token)
            .session_identity(session_id, accounts.as_deref())
    }
}
