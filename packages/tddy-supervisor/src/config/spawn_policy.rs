use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What the daemon is allowed to ask the supervisor to spawn, and on whose behalf.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnPolicy {
    /// Absolute paths of tools a session may exec.
    #[serde(default)]
    pub allowed_tool_paths: Vec<PathBuf>,
    /// Absolute paths of coding-agent harness binaries that may be run with their stdio relayed over
    /// a caller-owned socket (`SpawnRelayed`). **Empty by default: relayed spawning is off.** It is a
    /// list of its own, not `allowed_tool_paths`, so granting a session tool never grants a relay.
    #[serde(default)]
    pub allowed_relay_tool_paths: Vec<PathBuf>,
    /// Where a binary the daemon asks to run may live, for every spawn kind (session, sandbox,
    /// streamed, relayed). Each entry is absolute and names a **directory** (any file beneath it) or
    /// **one trusted file** (exactly that file). A requested path is judged by the file it resolves
    /// to, and that file, every directory above it and the entry itself must be owned by root (or
    /// the supervisor's euid) and writable by nobody else — a sticky world-writable directory
    /// included. See [`crate::policy::resolve_exec`].
    ///
    /// Replaces `allowed_tool_paths` and `allowed_relay_tool_paths`.
    // TODO(supervisor-agent-agnostic): implement — the two old lists and `allowed_env_keys` go, and
    // naming any of them fails loading with the replacement.
    ///
    /// An entry written as a map may set `trusted_prefix: true` (`{ path: /nix/store,
    /// trusted_prefix: true }`): the operator vouches for that directory itself, so its own mode is
    /// not checked — `/nix/store` is group-writable and sticky by design — while everything below it
    /// still is.
    #[serde(default)]
    pub exec_roots: Vec<ExecRoot>,
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
/// One `spawn_policy.exec_roots` entry: a bare absolute path, or a map that can also mark it a
/// trusted prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ExecRootEntry", into = "ExecRootEntry")]
pub struct ExecRoot {
    pub path: PathBuf,
    /// Set by the operator only, per entry: this directory's own owner and mode are not checked.
    /// Every directory and file below it still is.
    pub trusted_prefix: bool,
}

impl ExecRoot {
    /// A plain entry, checked all the way up.
    pub fn at(path: impl Into<PathBuf>) -> ExecRoot {
        ExecRoot {
            path: path.into(),
            trusted_prefix: false,
        }
    }

    /// An entry whose own directory the operator vouches for.
    pub fn trusted_prefix(path: impl Into<PathBuf>) -> ExecRoot {
        ExecRoot {
            path: path.into(),
            trusted_prefix: true,
        }
    }
}

/// How an [`ExecRoot`] is written in YAML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum ExecRootEntry {
    Path(PathBuf),
    Marked(MarkedExecRoot),
}

/// The map form, `{ path: …, trusted_prefix: true }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkedExecRoot {
    path: PathBuf,
    #[serde(default)]
    trusted_prefix: bool,
}

impl From<ExecRootEntry> for ExecRoot {
    fn from(entry: ExecRootEntry) -> ExecRoot {
        match entry {
            ExecRootEntry::Path(path) => ExecRoot::at(path),
            ExecRootEntry::Marked(MarkedExecRoot {
                path,
                trusted_prefix,
            }) => ExecRoot {
                path,
                trusted_prefix,
            },
        }
    }
}

impl From<ExecRoot> for ExecRootEntry {
    fn from(root: ExecRoot) -> ExecRootEntry {
        if root.trusted_prefix {
            ExecRootEntry::Marked(MarkedExecRoot {
                path: root.path,
                trusted_prefix: true,
            })
        } else {
            ExecRootEntry::Path(root.path)
        }
    }
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
