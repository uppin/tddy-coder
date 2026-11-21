//! The worktree service's [`TargetShells`] port, over the same target shells a session start, a
//! resume and a target session's tabs use (`#target-parity` 2/7): an ssh target's transport, or a
//! Coder workspace reached with the owner's vault credentials. The daemon's resolver is the tabs'
//! [`SessionTargetShells`], so the Worktree tab's actions and a tab on the target reach it alike.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_core::SessionTarget;
use tddy_remote_shell::RemoteShell;
use tddy_rpc::Status;
use tddy_worktree_service::target_worktrees::TargetShells;

pub use crate::tab_target_shell::SessionTargetShells;

#[async_trait]
impl TargetShells for SessionTargetShells {
    async fn shell_for(
        &self,
        target: &SessionTarget,
        subject: &str,
    ) -> Result<Arc<dyn RemoteShell>, Status> {
        crate::opencode_cli_spawn::session_start::shell_for_session_target(
            target,
            subject,
            self.coder_credentials.as_ref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_host_service::coder_workspaces::{
        CoderCredentialError, CoderCredentialSource, CoderCredentials,
    };
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
