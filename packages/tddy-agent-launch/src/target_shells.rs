//! The worktree service's [`TargetShells`] port, over the same target shells a session start and a
//! resume use (`#target-parity` 2/7): an ssh target's transport, or a Coder workspace reached with
//! the owner's vault credentials.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::SessionTarget;
use tddy_host_service::coder_workspaces::CoderCredentialSource;
use tddy_remote_shell::RemoteShell;
use tddy_rpc::Status;
use tddy_worktree_service::target_worktrees::TargetShells;

/// Target shells resolved as a session start resolves them, Coder credentials read from
/// `credentials`.
pub struct SessionTargetShells {
    credentials: Arc<dyn CoderCredentialSource>,
}

impl SessionTargetShells {
    /// Target shells whose Coder workspaces are reached with credentials from `credentials`.
    #[must_use]
    pub fn new(credentials: Arc<dyn CoderCredentialSource>) -> Self {
        Self { credentials }
    }
}

#[async_trait]
impl TargetShells for SessionTargetShells {
    async fn shell_for(
        &self,
        target: &SessionTarget,
        os_user: &str,
    ) -> Result<Arc<dyn RemoteShell>, Status> {
        // TODO(target-worktree-actions): implement over `shell_for_session_target`.
        let _ = (target, os_user, &self.credentials);
        todo!("SessionTargetShells::shell_for")
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_host_service::coder_workspaces::{CoderCredentialError, CoderCredentials};
    use tddy_rpc::Code;

    use super::*;

    /// A vault that is not open.
    struct ALockedVault;

    impl CoderCredentialSource for ALockedVault {
        fn credentials_for(
            &self,
            _subject: &str,
            _account_id: &str,
        ) -> Result<CoderCredentials, CoderCredentialError> {
            Err(CoderCredentialError::VaultLocked)
        }
    }

    #[tokio::test]
    async fn an_ssh_target_is_reached_through_its_host_alias() {
        // Given
        let shells = SessionTargetShells::new(Arc::new(ALockedVault));

        // When
        let shell = shells
            .shell_for(&SessionTarget::ssh("buildbox"), "ada")
            .await;

        // Then
        assert!(shell.is_ok(), "an ssh target needs no vault");
    }

    #[tokio::test]
    async fn a_coder_target_behind_a_locked_vault_is_refused_as_a_failed_precondition() {
        // Given
        let shells = SessionTargetShells::new(Arc::new(ALockedVault));
        let target = SessionTarget::coder("coder-1@coder.example.com", "ada", "remote-dev");

        // When
        let refused = shells
            .shell_for(&target, "ada")
            .await
            .err()
            .map(|s| s.code());

        // Then
        assert_eq!(refused, Some(Code::FailedPrecondition));
    }
}
