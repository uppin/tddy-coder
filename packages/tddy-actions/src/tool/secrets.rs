//! A tool's `secrets:` section — values delivered to its process out of band, never in the jail's
//! env list or argv.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).
//!
//! ```yaml
//! secrets:
//!   - env: CLAUDE_CODE_OAUTH_TOKEN
//!     source: { kind: host-env, var: CLAUDE_CODE_OAUTH_TOKEN }
//!     when: unrouted
//! ```
//!
//! Lowering is `tddy-sandbox-recipes`' `lower_secrets`: each item becomes a `TDDY_SECRET_<env>`
//! file the runner resolves to `<env>` on the tool's process alone.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretItem {
    /// The variable the tool reads the secret from.
    pub env: String,
    pub source: SecretItemSource,
    /// Deliver the secret only on a launch the guard admits; absent delivers it always.
    #[serde(default)]
    pub when: Option<RouteGuard>,
}

/// Where a secret's value is read on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SecretItemSource {
    /// The daemon's environment variable `var`; unset delivers nothing.
    HostEnv { var: String },
    /// A host file whose contents are the secret.
    HostFile { path: PathBuf },
}

/// Which launches a spec item applies to, by whether the session runs on a provider route.
///
/// The routing split a recipe once made in code — Claude's own OAuth token only when no provider
/// route is set — declared on the item it governs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteGuard {
    /// Only when the session runs on the tool's own vendor.
    Unrouted,
    /// Only when the session runs on a provider route.
    Routed,
}
