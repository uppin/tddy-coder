//! Harness profiles: agent definitions that run a coding harness on a provider account.
//!
//! A profile lives beside the OpenAI-loop definitions in `<tddyhome>/agents/*.yaml`, told apart by
//! its `harness:` key:
//!
//! ```yaml
//! name: claude-on-meta
//! label: Claude Code · Meta Wonder
//! harness: claude-cli          # claude, claude-acp, codex, codex-acp, opencode, claude-cli, opencode-cli, muse-cli
//! provider_account: meta-hackathon   # an account id or label in the starting person's vault
//! model: muse-wonder-1.0
//! ```
//!
//! It is its own type rather than a field on [`crate::agent_def::SpecializedAgentDef`]: that def is
//! an endpoint (`base_url` is required), and a profile is a harness pointed at an account. A profile
//! carrying `base_url` or `api_key` is refused at load.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// One harness profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessProfile {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    /// A tool-session agent id or a CLI session type.
    pub harness: String,
    /// The provider account, by id or label, in the starting person's vault.
    pub provider_account: String,
    pub model: String,
}

/// Every harness profile in `dir`, by file; a file that is not a profile (no `harness:` key) is
/// skipped silently, and a malformed profile is skipped with a warning naming it.
#[must_use]
pub fn load_harness_profiles(dir: &Path) -> Vec<HarnessProfile> {
    let _ = dir;
    // TODO(agent-def-route): implement
    todo!()
}

/// Parse one profile, refusing a document that is not one, or one that also names an endpoint.
pub fn parse_harness_profile(yaml: &str) -> Result<HarnessProfile, String> {
    let _ = yaml;
    // TODO(agent-def-route): implement
    todo!()
}

/// The agent-list rows (`id`, `label`) profiles add: the profile's name, and its label or name.
#[must_use]
pub fn harness_profile_rows(profiles: &[HarnessProfile]) -> Vec<(String, String)> {
    let _ = profiles;
    // TODO(agent-def-route): implement
    todo!()
}
