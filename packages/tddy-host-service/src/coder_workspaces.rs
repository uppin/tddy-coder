//! The workspaces a Coder account can reach, behind `ListCoderWorkspaces`.
//!
//! Two ports, because neither side is this crate's: the account is in a vault `tddy-accounts`
//! reads ([`CoderCredentialSource`]), and the deployment is asked over an HTTP client the daemon
//! owns ([`CoderWorkspaceSource`]). The listing is one request to the deployment — never the
//! `coder` CLI — so a host without the CLI still lists.
//!
//! # Honesty
//!
//! As [`crate::ssh_config`]: an account with no workspaces is `OK` and empty. A listing that could
//! not be made is `FAILED`, and its reason is the [`CoderCredentialError`] or
//! [`CoderWorkspaceListError`] that stopped it, in that error's own words — so a locked vault, a
//! rejected token and an unreachable deployment read differently. No reason carries the token.

use std::fmt;

use async_trait::async_trait;

pub use tddy_remote_shell::CoderCredentials;

/// Why an account's deployment URL and token were not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoderCredentialError {
    /// The subject's vault is not open on this daemon: nothing in it can be read until they
    /// unlock it.
    VaultLocked,
    /// The vault is open and holds no Coder account by this id. Carries the id as it was asked.
    NoSuchAccount(String),
    /// The vault could not be asked at all. Carries the reason.
    Unavailable(String),
}

impl fmt::Display for CoderCredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VaultLocked => f.write_str(
                "the vault holding this Coder account is locked: unlock it and list again",
            ),
            Self::NoSuchAccount(account_id) => {
                write!(f, "the vault holds no Coder account {account_id}")
            }
            Self::Unavailable(reason) => {
                write!(f, "this daemon could not read the Coder account: {reason}")
            }
        }
    }
}

impl std::error::Error for CoderCredentialError {}

/// Reads a Coder account's deployment URL and token.
///
/// A port, because the vault is `tddy-accounts`' and this crate does not depend on it: the daemon
/// fills it from `CoderAccounts::credentials_for`.
pub trait CoderCredentialSource: Send + Sync {
    /// The credentials of `subject`'s Coder account `account_id`. `subject` is who the calling
    /// session's token resolved to.
    fn credentials_for(
        &self,
        subject: &str,
        account_id: &str,
    ) -> Result<CoderCredentials, CoderCredentialError>;
}

/// One workspace, as its deployment reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderWorkspace {
    /// The login name of the user the workspace belongs to.
    pub owner: String,
    /// The workspace's name, unique among its owner's.
    pub name: String,
    /// The deployment's word for the state of the workspace's latest build.
    pub status: String,
}

impl CoderWorkspace {
    /// The status of a workspace a command can be run on.
    pub const RUNNING: &'static str = "running";

    /// Whether a command can be run on this workspace: its status is [`Self::RUNNING`]. Every
    /// other status — stopped, starting, stopping, failed, one this code has never seen — is not.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.status == Self::RUNNING
    }
}

/// Why a deployment did not list an account's workspaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoderWorkspaceListError {
    /// The deployment answered and refused the token. Carries the deployment's own words.
    Rejected(String),
    /// Nothing answered at the URL: DNS, connect, TLS or a timeout. Carries the reason.
    Unreachable(String),
    /// Something answered, but not as a Coder deployment does — any other status, or a body that
    /// lists no workspaces. Carries what was seen.
    Unusable(String),
}

impl fmt::Display for CoderWorkspaceListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(words) => write!(
                f,
                "the Coder deployment rejected this account's token: {words}"
            ),
            Self::Unreachable(reason) => {
                write!(f, "the Coder deployment could not be reached: {reason}")
            }
            Self::Unusable(seen) => write!(
                f,
                "the Coder deployment did not answer with a list of workspaces: {seen}"
            ),
        }
    }
}

impl std::error::Error for CoderWorkspaceListError {}

/// Asks a Coder deployment for the workspaces an account owns.
///
/// A port, because the HTTP client is the daemon's: `GET /api/v2/workspaces?q=owner:me` with the
/// token in the `Coder-Session-Token` header.
#[async_trait]
pub trait CoderWorkspaceSource: Send + Sync {
    /// The workspaces owned by the user `credentials` authenticates as, in the deployment's order.
    async fn workspaces(
        &self,
        credentials: &CoderCredentials,
    ) -> Result<Vec<CoderWorkspace>, CoderWorkspaceListError>;
}

/// What a service that was never given a vault or a deployment client answers with: every listing
/// fails, saying this daemon was not wired for Coder.
pub(crate) struct CoderNotWired;

const NOT_WIRED: &str = "this daemon was started without Coder accounts";

impl CoderCredentialSource for CoderNotWired {
    fn credentials_for(
        &self,
        _subject: &str,
        _account_id: &str,
    ) -> Result<CoderCredentials, CoderCredentialError> {
        Err(CoderCredentialError::Unavailable(NOT_WIRED.to_string()))
    }
}

#[async_trait]
impl CoderWorkspaceSource for CoderNotWired {
    async fn workspaces(
        &self,
        _credentials: &CoderCredentials,
    ) -> Result<Vec<CoderWorkspace>, CoderWorkspaceListError> {
        Err(CoderWorkspaceListError::Unusable(NOT_WIRED.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn a_workspace_that_is(status: &str) -> CoderWorkspace {
        CoderWorkspace {
            owner: "ada".to_string(),
            name: "devbox".to_string(),
            status: status.to_string(),
        }
    }

    #[test]
    fn only_a_workspace_whose_status_is_running_is_running() {
        // Given one workspace in each state a deployment reports, and one it might report later
        let statuses = [
            "running", "stopped", "starting", "stopping", "failed", "pending", "",
        ];

        // When each is asked whether a command can be run on it
        let running: Vec<bool> = statuses
            .iter()
            .map(|status| a_workspace_that_is(status).is_running())
            .collect();

        // Then only the running one can
        assert_eq!(running, [true, false, false, false, false, false, false]);
    }

    #[test]
    fn a_locked_vault_says_to_unlock_it() {
        // Given / When
        let reason = CoderCredentialError::VaultLocked.to_string();

        // Then
        assert_eq!(
            reason,
            "the vault holding this Coder account is locked: unlock it and list again"
        );
    }

    #[test]
    fn an_unknown_account_names_the_account_that_was_asked_for() {
        // Given / When
        let reason = CoderCredentialError::NoSuchAccount("coder-u1@coder.example.com".to_string())
            .to_string();

        // Then
        assert_eq!(
            reason,
            "the vault holds no Coder account coder-u1@coder.example.com"
        );
    }

    #[test]
    fn a_rejected_token_and_an_unreachable_deployment_read_differently() {
        // Given
        let rejected = CoderWorkspaceListError::Rejected("API key is invalid.".to_string());
        let unreachable = CoderWorkspaceListError::Unreachable("connection refused".to_string());

        // When
        let reasons = (rejected.to_string(), unreachable.to_string());

        // Then
        assert_eq!(
            reasons,
            (
                "the Coder deployment rejected this account's token: API key is invalid."
                    .to_string(),
                "the Coder deployment could not be reached: connection refused".to_string(),
            )
        );
    }

    #[test]
    fn a_service_that_was_never_wired_for_coder_says_so() {
        // Given / When
        let refused = CoderNotWired.credentials_for("ada", "coder-u1@coder.example.com");

        // Then
        assert_eq!(
            refused,
            Err(CoderCredentialError::Unavailable(
                "this daemon was started without Coder accounts".to_string()
            ))
        );
    }
}
