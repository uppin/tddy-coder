//! Who a co-located session's commits are by — one resolution, taken at the session's edge.
//!
//! The pure half ([`git_environment_for`]) turns a project's assignments and what the session's
//! vault holds into the `GIT_*` pairs; the thin half ([`SessionAccountAccess`]) reads the vault.
//! Both the token and the identity come from one `acting_identity` call, and only the identity half
//! is delivered here: the token is never put in the agent's environment, and nothing in this module
//! reads `GITHUB_TOKEN` or `GH_TOKEN`.

use std::sync::Arc;

use tddy_accounts::{
    acting_identity, AccountStore, AccountsError, IdentityError, SessionSubjectResolver,
    SessionVaultAccountStore, PROVIDER_GITHUB,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SessionVaults};
use tddy_daemon_livekit::session_git::session_git_environment;
use tddy_projects::project_storage::AccountAssignment;

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

/// The `GIT_*` pairs of the GitHub account `assignments` name, from one `acting_identity`
/// resolution over `held`.
pub(crate) fn git_environment_for(
    assignments: &[(ProviderId, AccountId)],
    held: &[CredentialRecord],
) -> Result<Vec<(String, String)>, IdentityError> {
    let acting = acting_identity(assignments, &ProviderId::new(PROVIDER_GITHUB), held)?;
    Ok(session_git_environment(&acting))
}

/// What a session start may read to learn who it acts as: the daemon's vaults, how a session token
/// names its owner, and the token itself.
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

    /// The identity pairs for `accounts`, or the reason there are none.
    pub(crate) fn git_environment(
        &self,
        accounts: &[AccountAssignment],
    ) -> Result<Vec<(String, String)>, SessionIdentityRefusal> {
        let vaults = self
            .vaults
            .as_ref()
            .filter(|_| !self.session_token.is_empty())
            .ok_or(SessionIdentityRefusal::NoVaultsConfigured)?;
        let held = SessionVaultAccountStore::new(Arc::clone(vaults), Arc::clone(&self.subject_of))
            .list(&self.session_token)
            .map_err(SessionIdentityRefusal::Vault)?;
        let held: Vec<CredentialRecord> = held
            .into_iter()
            .filter(|record| record.provider.as_str() == PROVIDER_GITHUB)
            .collect();
        git_environment_for(&assignments_of(accounts), &held)
            .map_err(SessionIdentityRefusal::Identity)
    }

    /// [`Self::git_environment`], where a refusal leaves the session starting with no `GIT_*`
    /// variables and is logged — the checkout's own identity stays in force.
    pub(crate) fn git_environment_or_inherited(
        &self,
        session_id: &str,
        accounts: &[AccountAssignment],
    ) -> Vec<(String, String)> {
        // TODO(keyring 9/9): a refused project starts with the checkout's inherited commit identity.
        // Consented in the changeset (M2) so unassigned projects keep working; it is the one place
        // the identity half is not strictly enforced.
        self.git_environment(accounts).unwrap_or_else(|refusal| {
            log::warn!(
                target: "tddy_daemon::connection_service",
                "session {session_id} starts without an account identity: {refusal}"
            );
            Vec::new()
        })
    }
}
