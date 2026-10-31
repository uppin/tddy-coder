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

    /// [`Self::render`], or why `params` cannot be rendered by this adapter. What a launcher calls
    /// (`#agent-tools 7/12`): an adapter that reports over the daemon's agent-tool socket refuses
    /// params without one ([`SlotError::NoDaemonSocket`]) instead of falling back to `daemon_url`.
    /// Every adapter that renders any params needs no override.
    fn try_render(
        &self,
        workdir: &Path,
        params: &HookCommandParams<'_>,
    ) -> Result<(PathBuf, Vec<u8>), SlotError> {
        Ok(self.render(workdir, params))
    }
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

/// The adapter of an agent that takes no lifecycle hooks (`#agent-tools 8/12`: OpenCode, Muse and
/// Warp on today's launch), named `unsupported`: a launch that passes it hook params is refused
/// ([`SlotError::HooksUnsupported`]) rather than given no hooks silently.
///
/// TODO(harness-clis): a stand-in. The word belongs on the slot — `hooks: unsupported`, as
/// `ToolSlot` takes it — which is a type change to `#agent-tools 2/12`'s `HooksSlot`; until the
/// developer decides it, an adapter carries the declaration.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnsupportedHooks;

/// [`UnsupportedHooks`]'s name.
pub const UNSUPPORTED_HOOKS: &str = "unsupported";

impl HooksAdapter for UnsupportedHooks {
    fn name(&self) -> &'static str {
        UNSUPPORTED_HOOKS
    }

    fn render(&self, workdir: &Path, params: &HookCommandParams<'_>) -> (PathBuf, Vec<u8>) {
        // TODO(harness-clis): implement — never reached through `try_render`, which refuses.
        let _ = (workdir, params);
        todo!("harness-clis: UnsupportedHooks::render")
    }

    fn try_render(
        &self,
        workdir: &Path,
        params: &HookCommandParams<'_>,
    ) -> Result<(PathBuf, Vec<u8>), SlotError> {
        // TODO(harness-clis): implement — `Err(SlotError::HooksUnsupported)`.
        let _ = (workdir, params);
        todo!("harness-clis: UnsupportedHooks::try_render")
    }
}

/// The adapter named `name`; an unknown name is [`SlotError::UnknownHooksAdapter`].
pub fn adapter(name: &str) -> Result<&'static dyn HooksAdapter, SlotError> {
    // TODO(claude-host): implement.
    // TODO(cursor-cli): and resolve `cursor-hooks-json` to `super::cursor::CursorHooksJson`.
    // TODO(harness-clis): and `unsupported` to `UnsupportedHooks`.
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
    fn the_unsupported_hooks_adapter_refuses_hook_params() {
        // Given
        let params = HookCommandParams {
            tddy_tools_path: "/opt/tddy/tddy-tools",
            daemon_url: "http://127.0.0.1:8899",
            session_id: "01900000-0000-7000-8000-0000000000a1",
            os_user: "dev",
            hook_token: "a-hook-token",
            daemon_socket: None,
            answer_wait_secs: 120,
        };

        // When
        let refused = UnsupportedHooks.try_render(Path::new("/work/session"), &params);

        // Then
        assert_eq!(refused, Err(SlotError::HooksUnsupported));
    }

    #[test]
    fn an_unknown_hooks_adapter_is_refused_naming_it() {
        // When
        // (`cursor-hooks-json` was the example until `#agent-tools 7/12` registered it.)
        let refused = adapter("an-unregistered-hooks-json").map(|a| a.name());

        // Then
        assert_eq!(
            refused,
            Err(SlotError::UnknownHooksAdapter {
                name: "an-unregistered-hooks-json".to_string(),
            })
        );
    }
}
