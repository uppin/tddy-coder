//! The `hooks` slot: a named Rust adapter writes the agent's lifecycle-hooks configuration. The
//! hook protocol (`tddy-tools session-hook`, the PermissionRequest decision JSON) is not declarable.

use std::path::{Path, PathBuf};

use tddy_agent_backend::claude_hooks::HookCommandParams;

use super::SlotError;

/// One way of wiring an agent's lifecycle hooks to `tddy-tools session-hook`.
pub trait HooksAdapter: Send + Sync {
    /// The name a spec's `hooks.adapter` uses.
    fn name(&self) -> &'static str;

    /// The hooks file for `params`, under the agent's working directory `workdir`, with its bytes.
    fn render(&self, workdir: &Path, params: &HookCommandParams<'_>) -> (PathBuf, Vec<u8>);
}

/// Claude Code's `<workdir>/.claude/settings.local.json`, built by
/// `tddy_agent_backend::build_claude_hooks_settings`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeSettingsLocal;

/// [`ClaudeSettingsLocal`]'s name.
pub const CLAUDE_SETTINGS_LOCAL: &str = "claude-settings-local";

impl HooksAdapter for ClaudeSettingsLocal {
    fn name(&self) -> &'static str {
        CLAUDE_SETTINGS_LOCAL
    }

    fn render(&self, workdir: &Path, params: &HookCommandParams<'_>) -> (PathBuf, Vec<u8>) {
        // TODO(claude-host): implement — the bytes `write_claude_hooks_settings` writes today.
        let _ = (workdir, params);
        todo!("claude-host: ClaudeSettingsLocal::render")
    }
}

/// The adapter named `name`; an unknown name is [`SlotError::UnknownHooksAdapter`].
pub fn adapter(name: &str) -> Result<&'static dyn HooksAdapter, SlotError> {
    // TODO(claude-host): implement.
    let _ = name;
    todo!("claude-host: hooks::adapter")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_claude_settings_local_name_resolves_to_its_adapter() {
        // When
        let resolved = adapter("claude-settings-local").map(|a| a.name());

        // Then
        assert_eq!(resolved, Ok(CLAUDE_SETTINGS_LOCAL));
    }

    #[test]
    fn an_unknown_hooks_adapter_is_refused_naming_it() {
        // When
        let refused = adapter("cursor-hooks-json").map(|a| a.name());

        // Then
        assert_eq!(
            refused,
            Err(SlotError::UnknownHooksAdapter {
                name: "cursor-hooks-json".to_string(),
            })
        );
    }
}
