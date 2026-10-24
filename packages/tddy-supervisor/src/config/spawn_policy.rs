use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What the daemon is allowed to ask the supervisor to spawn, and on whose behalf.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnPolicy {
    /// OS users a session may run as. `root` is rejected at load time.
    #[serde(default)]
    pub allowed_session_users: Vec<String>,
    /// Absolute paths of tools a session may exec.
    #[serde(default)]
    pub allowed_tool_paths: Vec<PathBuf>,
    /// Absolute paths of coding-agent harness binaries that may be run with their stdio relayed over
    /// a caller-owned socket (`SpawnRelayed`). **Empty by default: relayed spawning is off.** It is a
    /// list of its own, not `allowed_tool_paths`, so granting a session tool never grants a relay.
    #[serde(default)]
    pub allowed_relay_tool_paths: Vec<PathBuf>,
    /// Roots under which a sandbox bind-mount source must fall.
    #[serde(default)]
    pub allowed_mount_roots: Vec<PathBuf>,
    /// Environment variables a caller may set on a session it asks for.
    ///
    /// Empty by default, so a caller's `env` is refused rather than passed on. The caller is the
    /// daemon; a variable it can put into the environment of a process running as *another* OS user
    /// is a grant, and grants belong in this file. Loader variables ([`is_loader_env_key`]) are
    /// rejected here at load time — listing one would hand over the tool allowlist with it.
    #[serde(default)]
    pub allowed_env_keys: Vec<String>,
}
/// Environment variables that decide what code a process loads, whatever binary it was told to
/// exec. Prefixes, because the loader reads a family of each.
const LOADER_ENV_PREFIXES: [&str; 2] = ["LD_", "MALLOC_"];

/// The rest of the same family, which have no common prefix.
const LOADER_ENV_KEYS: [&str; 5] = [
    "GCONV_PATH",
    "NLSPATH",
    "LOCPATH",
    "HOSTALIASES",
    "RESOLV_HOST_CONF",
];

/// Whether `key` is one of the variables that chooses code rather than configuring behavior.
///
/// `LD_PRELOAD` on its own turns any allowlisted tool into a loader for a library the caller picked,
/// which is the `allowed_tool_paths` allowlist bypassed without being touched. It works because the
/// child execs *after* `setuid` to an ordinary account, so the binary is not setuid, `AT_SECURE` is
/// unset, and the dynamic loader honors all of these.
pub(crate) fn is_loader_env_key(key: &str) -> bool {
    LOADER_ENV_PREFIXES
        .iter()
        .any(|prefix| key.starts_with(prefix))
        || LOADER_ENV_KEYS.contains(&key)
}
