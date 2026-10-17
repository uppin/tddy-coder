//! What a daemon-run clone is given to run in: which ssh-agent, which helper, which grant.
//!
//! Pure over its inputs, so what reaches `git` is a value a test reads. Deciding *whether* a clone
//! gets any of it, and under which host conditions, is [`crate::clone_planner`]'s.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use tddy_host_service::askpass::CloneGrant;
use tddy_host_service::ssh_agent::AgentSocketResolver;
use tddy_spawn::clone_env::CloneSshEnv;

/// Why a clone cannot be given its user's ssh-agent.
///
/// The resolver's two answers stay two errors: "looked, found none" sends an operator to start an
/// agent, "could not look" sends them to their directory service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneAgentError {
    NoAgentReachable { os_user: String },
    LookupFailed { os_user: String, reason: String },
}

impl fmt::Display for CloneAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAgentReachable { os_user } => {
                write!(f, "no ssh-agent is reachable for {os_user}")
            }
            Self::LookupFailed { os_user, reason } => write!(
                f,
                "the daemon could not look for {os_user}'s ssh-agent: {reason}"
            ),
        }
    }
}

impl std::error::Error for CloneAgentError {}

/// The socket of `os_user`'s ssh-agent. Never an empty path for ssh to fail on later.
pub fn agent_socket_for(
    resolver: &dyn AgentSocketResolver,
    os_user: &str,
) -> Result<PathBuf, CloneAgentError> {
    match resolver.socket_for(os_user) {
        Ok(Some(socket)) => Ok(socket),
        Ok(None) => Err(CloneAgentError::NoAgentReachable {
            os_user: os_user.to_string(),
        }),
        Err(reason) => Err(CloneAgentError::LookupFailed {
            os_user: os_user.to_string(),
            reason,
        }),
    }
}

/// The ssh settings of a clone run as `os_user` whose helper is `askpass_program` and whose
/// operator is the one `grant` admits.
pub fn clone_ssh_env_for(
    resolver: &dyn AgentSocketResolver,
    os_user: &str,
    askpass_program: &Path,
    grant: &CloneGrant,
) -> Result<CloneSshEnv, CloneAgentError> {
    let socket = agent_socket_for(resolver, os_user)?;
    Ok(CloneSshEnv::new(askpass_program, &grant.url(), &grant.token()).agent_socket(socket))
}

/// How a clone reaches its remote, decided once per clone.
#[derive(Debug, Clone)]
pub enum ClonePlan {
    /// Fetched from another daemon through `tddy-remote-git-repo`; no key of this host's is used.
    Facilitator { ssh_command: String },
    /// Over SSH with the user's own key, asking the operator for its passphrase.
    Ssh(CloneSshEnv),
}

impl ClonePlan {
    /// The environment variables to set on the `git clone` child.
    #[must_use]
    pub fn environment(self) -> BTreeMap<String, String> {
        match self {
            Self::Facilitator { ssh_command } => {
                BTreeMap::from([("GIT_SSH_COMMAND".to_string(), ssh_command)])
            }
            Self::Ssh(ssh) => ssh.into_env().into_iter().collect(),
        }
    }
}

/// Whether `git_url` is fetched over SSH: `ssh://…` or scp-like `[user@]host:path`.
#[must_use]
pub fn is_ssh_url(git_url: &str) -> bool {
    let url = git_url.trim();
    if let Some((scheme, _)) = url.split_once("://") {
        return matches!(scheme, "ssh" | "git+ssh" | "ssh+git");
    }
    // scp-like: a colon before any slash, with a host in front of it
    matches!(url.split_once(':'), Some((host, _)) if !host.is_empty() && !host.contains('/'))
}

/// The helper binary: a sibling of the running executable (or of `deps/`'s parent under cargo),
/// else the bare name on `PATH`. Follows `resolve_remote_git_repo_path`, which `./install` pairs it
/// with.
#[must_use]
pub fn resolve_askpass_path() -> PathBuf {
    const NAME: &str = "tddy-askpass";
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mut bin_dir) = exe.parent().map(Path::to_path_buf) {
            if bin_dir.file_name().and_then(|n| n.to_str()) == Some("deps") {
                bin_dir.pop();
            }
            let candidate = bin_dir.join(NAME);
            if candidate.is_file() {
                return candidate;
            }
        }
        if let Some(candidate) = exe.parent().map(|p| p.join(NAME)) {
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(NAME)
}
