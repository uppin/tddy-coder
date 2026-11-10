use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// `users:` in `supervisor.yaml`: the mapping file and the directory its sockets are derived under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsersConfig {
    /// Absolute path of `users.yaml` (`/etc/tddy/users.yaml`). Watched; a change restarts the
    /// services that serve users.
    pub file: PathBuf,
    /// Absolute directory holding one `<os user>/daemon.sock` per mapped OS user. Every directory
    /// above it must be writable by root alone.
    #[serde(default = "default_user_sockets_dir")]
    pub sockets_dir: PathBuf,
}

fn default_user_sockets_dir() -> PathBuf {
    PathBuf::from("/run/tddy-users")
}

/// Refuse the hand-kept user lists `users.yaml` replaces, naming that file (`#agent-tools` 31/32).
///
/// Checked on the untyped document, before typed deserialization, so a leftover key is told it is
/// obsolete rather than getting serde's bare `unknown field`.
pub(super) fn refuse_old_user_list_keys(document: &serde_yaml::Value) -> Result<(), ConfigError> {
    let Some(top) = document.as_mapping() else {
        return Ok(());
    };
    let old_key = |key: &str| ConfigError {
        message: format!(
            "`{key}:` in supervisor.yaml is obsolete — the GitHub → OS-user mapping lives in \
             users.yaml now (see the top-level `users:` block); delete the `{key}:` key"
        ),
    };
    if let Some(policy) = top.get("spawn_policy").and_then(|v| v.as_mapping()) {
        if policy.contains_key("allowed_session_users") {
            return Err(old_key("allowed_session_users"));
        }
    }
    if let Some(services) = top.get("services").and_then(|v| v.as_sequence()) {
        for service in services {
            let Some(service) = service.as_mapping() else {
                continue;
            };
            for key in ["host_sockets", "agent_tool_sockets"] {
                if service.contains_key(key) {
                    return Err(old_key(key));
                }
            }
        }
    }
    Ok(())
}
