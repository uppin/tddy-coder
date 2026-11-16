use std::fmt;
use std::sync::Arc;
use tddy_core::SessionTarget;
use tddy_remote_shell::{RemoteShell, SshShell};
use tddy_service::proto::session::{
    session_target::Kind as ProtoTargetKind, SessionTarget as ProtoSessionTarget, SshTarget,
};

/// Why a session's target cannot be reached through a remote shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    /// An ssh target that names no `Host` alias.
    NoSshHost,
    /// A target this daemon has no transport for, with the reason.
    Unsupported(String),
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSshHost => write!(f, "the ssh target names no host alias"),
            Self::Unsupported(reason) => write!(f, "the target cannot be reached: {reason}"),
        }
    }
}

impl std::error::Error for TargetError {}

/// The remote shell that reaches `target`: for an ssh target, the OpenSSH transport on its alias,
/// through the `ssh` on the daemon's `PATH`.
pub fn shell_for_target(target: &SessionTarget) -> Result<Arc<dyn RemoteShell>, TargetError> {
    match target {
        SessionTarget::Ssh { config_host } => {
            let config_host = config_host.trim();
            if config_host.is_empty() {
                return Err(TargetError::NoSshHost);
            }
            Ok(Arc::new(SshShell::new(config_host)))
        }
        // A Coder target needs the caller's account, which this function is not given: the
        // launch resolves it through `coder_target::shell_for_coder_target`.
        SessionTarget::Coder { .. } => Err(TargetError::Unsupported(
            "a Coder target is reached with its account's credentials".to_string(),
        )),
    }
}

/// The `Host` alias an ssh `target` names; `None` for no target.
pub(crate) fn target_config_host(target: Option<&SessionTarget>) -> Option<&str> {
    match target {
        Some(SessionTarget::Ssh { config_host }) => {
            Some(config_host.trim()).filter(|host| !host.is_empty())
        }
        Some(SessionTarget::Coder { .. }) | None => None,
    }
}

/// The session target a start's proto `target` names; `None` when it names none. A proto target
/// naming no kind, or an ssh kind naming no alias, is refused as [`TargetError`].
pub fn session_target_of_proto(
    target: Option<&ProtoSessionTarget>,
) -> Result<Option<SessionTarget>, TargetError> {
    let Some(target) = target else {
        return Ok(None);
    };
    match &target.kind {
        Some(ProtoTargetKind::Ssh(ssh)) => {
            if ssh.config_host.trim().is_empty() {
                return Err(TargetError::NoSshHost);
            }
            Ok(Some(SessionTarget::ssh(ssh.config_host.trim())))
        }
        Some(ProtoTargetKind::Coder(coder)) => {
            let target = SessionTarget::coder(
                coder.account_id.trim(),
                coder.owner.trim(),
                coder.workspace.trim(),
            );
            let agent = coder.agent.trim();
            Ok(Some(if agent.is_empty() {
                target
            } else {
                target.on_agent(agent)
            }))
        }
        None => Err(TargetError::Unsupported(
            "the target names no kind of machine".to_string(),
        )),
    }
}

/// The proto target a `.session.yaml` [`SessionTarget`] lists as.
pub fn proto_of_session_target(target: &SessionTarget) -> ProtoSessionTarget {
    match target {
        SessionTarget::Ssh { config_host } => ProtoSessionTarget {
            kind: Some(ProtoTargetKind::Ssh(SshTarget {
                config_host: config_host.clone(),
            })),
        },
        SessionTarget::Coder {
            account_id,
            owner,
            workspace,
            agent,
        } => ProtoSessionTarget {
            kind: Some(ProtoTargetKind::Coder(
                tddy_service::proto::session::CoderTarget {
                    account_id: account_id.clone(),
                    owner: owner.clone(),
                    workspace: workspace.clone(),
                    agent: agent.clone().unwrap_or_default(),
                },
            )),
        },
    }
}
