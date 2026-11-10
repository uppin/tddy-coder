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
