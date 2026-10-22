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

use std::path::{Path, PathBuf};

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
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("yaml"))
        .collect();
    paths.sort();

    let mut profiles = Vec::new();
    for path in paths {
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(e) => {
                log::warn!("harness_profile: failed to read {}: {e}", path.display());
                continue;
            }
        };
        match names_a_harness(&contents) {
            Ok(true) => {}
            // An OpenAI-loop definition: `load_agent_defs` reads it.
            Ok(false) => continue,
            Err(e) => {
                log::warn!("harness_profile: failed to parse {}: {e}", path.display());
                continue;
            }
        }
        match parse_harness_profile(&contents) {
            Ok(profile) => profiles.push(profile),
            Err(e) => log::warn!("harness_profile: failed to parse {}: {e}", path.display()),
        }
    }
    profiles
}

/// Parse one profile, refusing a document that is not one, or one that also names an endpoint.
pub fn parse_harness_profile(yaml: &str) -> Result<HarnessProfile, String> {
    if !names_a_harness(yaml)? {
        return Err("not a harness profile: it has no `harness:` key".to_string());
    }
    serde_yaml::from_str(yaml).map_err(|e| e.to_string())
}

/// Whether `yaml` carries a `harness:` key. A document that is not YAML at all is an error, not a
/// `false`: a profile with a syntax error must not be taken for an OpenAI-loop definition.
fn names_a_harness(yaml: &str) -> Result<bool, String> {
    let document: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|e| e.to_string())?;
    Ok(document.get("harness").is_some())
}

/// The agent-list rows (`id`, `label`) profiles add: the profile's name, and its label or name.
#[must_use]
pub fn harness_profile_rows(profiles: &[HarnessProfile]) -> Vec<(String, String)> {
    profiles
        .iter()
        .map(|profile| {
            let label = profile.label.as_deref().unwrap_or(&profile.name);
            (profile.name.clone(), label.to_string())
        })
        .collect()
}
