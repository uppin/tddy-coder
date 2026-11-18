//! Reaching a session's **Coder** target: a workspace of one of the session owner's Coder
//! accounts, through the `coder` CLI on this daemon.
//!
//! [`crate::remote_agent_launch::target::shell_for_target`] resolves a target that needs nothing but its
//! own fields. A Coder target needs more: the caller's subject, and the vault that holds the
//! account's deployment URL and token ([`CoderCredentialSource`]). This is the entry for it. Once
//! the shell is resolved, everything else — [`crate::remote_agent_launch::launch_on_target`], the
//! hooks carrier, the follower — is the path an ssh target takes, unchanged.
//!
//! The token is read here, at the start and again at every resume, and lives only in the shell
//! this returns. A session's record names the account and never holds the token, so a vault that
//! is locked refuses the start, and refuses the resume.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-coder-hosts.md.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use tddy_core::SessionTarget;
use tddy_host_service::coder_workspaces::{
    CoderCredentialError, CoderCredentialSource, CoderCredentials,
};
use tddy_remote_shell::RemoteShell;

/// Why a Coder target cannot be reached through a remote shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoderTargetError {
    /// The target is of another kind: it is [`crate::remote_agent_launch::target::shell_for_target`]'s.
    NotACoderTarget,
    /// The target names no account, no owner or no workspace.
    NoWorkspace,
    /// The caller's vault is not open on this daemon, so the account's token cannot be read.
    VaultLocked,
    /// The caller's vault is open and holds no Coder account by this id. Carries the id as the
    /// target named it.
    NoSuchAccount(String),
    /// The vault could not be asked at all. Carries the reason.
    Unavailable(String),
}

impl fmt::Display for CoderTargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotACoderTarget => f.write_str("the target is not a Coder workspace"),
            Self::NoWorkspace => {
                f.write_str("the Coder target names no account, no owner or no workspace")
            }
            Self::VaultLocked => f.write_str(
                "the vault holding this session's Coder account is locked: unlock it and try again",
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

impl std::error::Error for CoderTargetError {}

/// The vault of a daemon that was started without Coder accounts: every read fails, saying so.
/// What a session host resolves Coder targets against until the daemon wires its own.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCoderAccounts;

impl CoderCredentialSource for NoCoderAccounts {
    fn credentials_for(
        &self,
        _subject: &str,
        _account_id: &str,
    ) -> Result<CoderCredentials, CoderCredentialError> {
        Err(CoderCredentialError::Unavailable(
            "this daemon was started without Coder accounts".to_string(),
        ))
    }
}

/// The remote shell that reaches the Coder workspace `target` names, as `subject`'s account: a
/// [`tddy_remote_shell::CoderShell`] on `<owner>/<workspace>[.<agent>]`, through the `coder` on the
/// daemon's `PATH` (and the OpenSSH `ssh` on it for a streamed command), holding the account's
/// deployment URL and token as `credentials` reads them.
///
/// `subject` is who the calling session's token resolved to — the owner of the vault asked. The
/// target is checked before the vault is read: one that is not a Coder target, or names no
/// workspace, is refused without a read.
pub fn shell_for_coder_target(
    target: &SessionTarget,
    subject: &str,
    credentials: &dyn CoderCredentialSource,
) -> Result<Arc<dyn RemoteShell>, CoderTargetError> {
    shell_for_coder_target_through("coder", "ssh", target, subject, credentials)
}

/// [`shell_for_coder_target`], reaching the workspace through the CLI `coder_binary`, and
/// streaming a command through the OpenSSH client `ssh_binary` with that CLI as its proxy.
pub fn shell_for_coder_target_through(
    coder_binary: impl Into<PathBuf>,
    ssh_binary: impl Into<PathBuf>,
    target: &SessionTarget,
    subject: &str,
    credentials: &dyn CoderCredentialSource,
) -> Result<Arc<dyn RemoteShell>, CoderTargetError> {
    let SessionTarget::Coder {
        account_id,
        owner,
        workspace,
        agent,
    } = target
    else {
        return Err(CoderTargetError::NotACoderTarget);
    };
    let (account_id, owner, workspace) = (account_id.trim(), owner.trim(), workspace.trim());
    if account_id.is_empty() || owner.is_empty() || workspace.is_empty() {
        return Err(CoderTargetError::NoWorkspace);
    }
    let held = credentials
        .credentials_for(subject, account_id)
        .map_err(|error| match error {
            CoderCredentialError::VaultLocked => CoderTargetError::VaultLocked,
            CoderCredentialError::NoSuchAccount(id) => CoderTargetError::NoSuchAccount(id),
            CoderCredentialError::Unavailable(reason) => CoderTargetError::Unavailable(reason),
        })?;
    let mut workspace_ref = tddy_remote_shell::CoderWorkspaceRef::new(owner, workspace);
    if let Some(agent) = agent.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
        workspace_ref = workspace_ref.on_agent(agent);
    }
    Ok(Arc::new(
        tddy_remote_shell::CoderShell::through(coder_binary, held, workspace_ref)
            .with_ssh_binary(ssh_binary),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use pretty_assertions::assert_eq;
    use tddy_credentials::SecretString;
    use tddy_remote_shell::ShellAddress;

    const ADA: &str = "ada";
    const ADAS_ACCOUNT: &str = "coder-3f1c9a52@coder.example.com";

    /// A vault that answers every read with one fixed outcome, and records who asked for what.
    struct AVault {
        answer: Result<CoderCredentials, CoderCredentialError>,
        asked: Mutex<Vec<(String, String)>>,
    }

    impl CoderCredentialSource for AVault {
        fn credentials_for(
            &self,
            subject: &str,
            account_id: &str,
        ) -> Result<CoderCredentials, CoderCredentialError> {
            self.asked
                .lock()
                .expect("the recorder is only held to push")
                .push((subject.to_string(), account_id.to_string()));
            self.answer.clone()
        }
    }

    impl AVault {
        fn asked(&self) -> Vec<(String, String)> {
            self.asked.lock().expect("the recorder is readable").clone()
        }
    }

    fn a_vault_answering(answer: Result<CoderCredentials, CoderCredentialError>) -> AVault {
        AVault {
            answer,
            asked: Mutex::new(Vec::new()),
        }
    }

    fn an_open_vault_holding_adas_account() -> AVault {
        a_vault_answering(Ok(CoderCredentials::new(
            "https://coder.example.com",
            SecretString::new("adas-coder-session-token"),
        )))
    }

    fn adas_devbox() -> SessionTarget {
        SessionTarget::coder(ADAS_ACCOUNT, ADA, "devbox")
    }

    /// Where the shell `target` resolves to reaches, asked as Ada.
    fn reached_for(
        target: &SessionTarget,
        vault: &AVault,
    ) -> Result<ShellAddress, CoderTargetError> {
        shell_for_coder_target(target, ADA, vault).map(|shell| shell.address())
    }

    #[test]
    fn a_coder_target_is_reached_through_a_coder_shell_on_its_workspace() {
        // Given
        let vault = an_open_vault_holding_adas_account();

        // When
        let address = reached_for(&adas_devbox(), &vault);

        // Then
        assert_eq!(address, Ok(ShellAddress::new("coder", "ada/devbox")));
    }

    #[test]
    fn a_coder_target_naming_an_agent_reaches_that_agent() {
        // Given
        let vault = an_open_vault_holding_adas_account();

        // When
        let address = reached_for(&adas_devbox().on_agent("main"), &vault);

        // Then
        assert_eq!(address, Ok(ShellAddress::new("coder", "ada/devbox.main")));
    }

    #[test]
    fn the_account_is_read_from_the_callers_vault_once() {
        // Given
        let vault = an_open_vault_holding_adas_account();

        // When
        let resolved = reached_for(&adas_devbox(), &vault).is_ok();

        // Then the vault was asked once, as the caller, for the account the target names
        assert_eq!(
            (resolved, vault.asked()),
            (true, vec![(ADA.to_string(), ADAS_ACCOUNT.to_string())])
        );
    }

    #[test]
    fn an_ssh_target_is_not_a_coder_target_and_reads_no_vault() {
        // Given
        let vault = an_open_vault_holding_adas_account();

        // When
        let address = reached_for(&SessionTarget::ssh("buildbox"), &vault);

        // Then
        assert_eq!(
            (address, vault.asked()),
            (Err(CoderTargetError::NotACoderTarget), Vec::new())
        );
    }

    #[test]
    fn a_coder_target_naming_no_workspace_is_refused_before_the_vault_is_read() {
        // Given
        let vault = an_open_vault_holding_adas_account();

        // When
        let address = reached_for(&SessionTarget::coder(ADAS_ACCOUNT, ADA, "  "), &vault);

        // Then
        assert_eq!(
            (address, vault.asked()),
            (Err(CoderTargetError::NoWorkspace), Vec::new())
        );
    }

    #[test]
    fn a_locked_vault_refuses_the_target() {
        // Given
        let vault = a_vault_answering(Err(CoderCredentialError::VaultLocked));

        // When
        let address = reached_for(&adas_devbox(), &vault);

        // Then
        assert_eq!(address, Err(CoderTargetError::VaultLocked));
    }

    #[test]
    fn an_account_the_vault_does_not_hold_refuses_the_target_naming_it() {
        // Given
        let vault = a_vault_answering(Err(CoderCredentialError::NoSuchAccount(
            ADAS_ACCOUNT.to_string(),
        )));

        // When
        let address = reached_for(&adas_devbox(), &vault);

        // Then
        assert_eq!(
            address,
            Err(CoderTargetError::NoSuchAccount(ADAS_ACCOUNT.to_string()))
        );
    }

    #[test]
    fn a_vault_that_cannot_be_asked_refuses_the_target_with_its_reason() {
        // Given
        let vault = a_vault_answering(Err(CoderCredentialError::Unavailable(
            "the vault file is unreadable".to_string(),
        )));

        // When
        let address = reached_for(&adas_devbox(), &vault);

        // Then
        assert_eq!(
            address,
            Err(CoderTargetError::Unavailable(
                "the vault file is unreadable".to_string()
            ))
        );
    }

    #[test]
    fn a_locked_vault_says_to_unlock_it() {
        // Given / When
        let reason = CoderTargetError::VaultLocked.to_string();

        // Then
        assert_eq!(
            reason,
            "the vault holding this session's Coder account is locked: unlock it and try again"
        );
    }

    #[test]
    fn a_daemon_started_without_coder_accounts_says_so() {
        // Given / When
        let refused = NoCoderAccounts.credentials_for(ADA, ADAS_ACCOUNT);

        // Then
        assert_eq!(
            refused,
            Err(CoderCredentialError::Unavailable(
                "this daemon was started without Coder accounts".to_string()
            ))
        );
    }
}
