//! A tool's `policy:` section — the non-file allows its process needs in a jail.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).
//!
//! ```yaml
//! policy: { jit: true, fork: true, sysctl_read: true, exec_paths: [/usr/bin, /bin] }
//! ```
//!
//! A missing section allows none of them. Lowering a tool set's sections into one jail policy is
//! `tddy-sandbox-recipes`' `lower_policy`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicySection {
    /// Dynamic code generation (a JIT: V8, the JVM).
    #[serde(default)]
    pub jit: bool,
    /// Forking child processes.
    #[serde(default)]
    pub fork: bool,
    /// Host directories whose binaries the process may exec, beyond its declared tools.
    #[serde(default)]
    pub exec_paths: Vec<PathBuf>,
    /// Reading `sysctl`s (Node reads the CPU count).
    #[serde(default)]
    pub sysctl_read: bool,
}
