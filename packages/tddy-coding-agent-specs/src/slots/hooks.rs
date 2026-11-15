//! The `hooks` slot: a named Rust adapter writes the agent's lifecycle-hooks configuration. The
//! hook protocol (`tddy-tools session-hook`, the PermissionRequest decision JSON) is not declarable.

use std::path::{Path, PathBuf};

use tddy_agent_backend::claude_hooks::{
    build_claude_hooks_settings, HookCommandParams, CLAUDE_INSTALLED_EVENTS,
};
use tddy_agent_backend::muse_hooks::{build_muse_hooks_config_value, MUSE_INSTALLED_EVENTS};

use super::cursor::{CursorHooksJson, CURSOR_HOOKS_JSON};
use super::SlotError;

/// What rendering an adapter's hooks yields: a file to write, or argv entries to splice into the
/// agent's launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderedHooks {
    /// A hooks file: its path and its bytes.
    File(PathBuf, Vec<u8>),
    /// Hooks delivered on the command line (Muse's `-c hooks=…`).
    Argv(Vec<String>),
}

/// How an adapter's hooks reach the agent: a file under its working directory, or argv entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HooksDelivery {
    File,
    Argv,
}

/// One way of wiring an agent's lifecycle hooks to `tddy-tools session-hook`.
pub trait HooksAdapter: Send + Sync {
    /// The name a spec's `hooks.adapter` uses.
    fn name(&self) -> &'static str;

    /// How this adapter's hooks reach the agent. A resume re-renders argv hooks and keeps a start's
    /// file as it is: only argv delivery passes hooks to a resumed launch.
    fn delivery(&self) -> HooksDelivery {
        HooksDelivery::File
    }

    /// The hooks file for `params`, under the agent's working directory `workdir`, with its bytes.
    /// Only file adapters render one: argv adapters override [`Self::try_render`] and never reach
    /// this.
    fn render(&self, workdir: &Path, params: &HookCommandParams<'_>) -> (PathBuf, Vec<u8>);

    /// The hook events the file this adapter renders installs — what a spec's
    /// `conversation.rotation.hook` may name (`#orchestrators` 6/11). None for an adapter that
    /// installs no hooks.
    fn installed_events(&self) -> &'static [&'static str] {
        &[]
    }

    /// [`Self::render`] as [`RenderedHooks::File`], or why `params` cannot be rendered by this
    /// adapter. What a launcher calls (`#agent-tools 7/12`): an adapter that reports over the
    /// daemon's agent-tool socket refuses params without one ([`SlotError::NoDaemonSocket`])
    /// instead of falling back to `daemon_url`. Every file adapter that renders any params needs
    /// no override; argv adapters override with their entries.
    fn try_render(
        &self,
        workdir: &Path,
        params: &HookCommandParams<'_>,
    ) -> Result<RenderedHooks, SlotError> {
        let (path, bytes) = self.render(workdir, params);
        Ok(RenderedHooks::File(path, bytes))
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

    fn installed_events(&self) -> &'static [&'static str] {
        CLAUDE_INSTALLED_EVENTS
    }

    fn render(&self, workdir: &Path, params: &HookCommandParams<'_>) -> (PathBuf, Vec<u8>) {
        let settings = build_claude_hooks_settings(params);
        // `{:#}` is `serde_json`'s pretty form, and unlike `to_string_pretty` cannot fail.
        (
            workdir.join(".claude").join("settings.local.json"),
            format!("{settings:#}").into_bytes(),
        )
    }
}

/// Muse Code's `-c hooks=…` argv entries, built by
/// `tddy_agent_backend::build_muse_hooks_config_value`: the same hook commands as
/// [`ClaudeSettingsLocal`], delivered on the command line. A file is not written — project
/// `.muse/hooks.json` loads only in a trusted workspace, while `-c` installs the same hooks
/// without one and merges with the user's `managed_hooks_path` hooks.
///
/// Like [`CursorHooksJson`](super::cursor::CursorHooksJson), reports over the daemon's agent-tool
/// socket and nothing else: params without one are [`SlotError::NoDaemonSocket`].
#[derive(Debug, Clone, Copy, Default)]
pub struct MuseConfigArg;

/// [`MuseConfigArg`]'s name.
pub const MUSE_CONFIG_ARG: &str = "muse-config-arg";

impl HooksAdapter for MuseConfigArg {
    fn name(&self) -> &'static str {
        MUSE_CONFIG_ARG
    }

    fn delivery(&self) -> HooksDelivery {
        HooksDelivery::Argv
    }

    fn installed_events(&self) -> &'static [&'static str] {
        MUSE_INSTALLED_EVENTS
    }

    fn render(&self, _workdir: &Path, _params: &HookCommandParams<'_>) -> (PathBuf, Vec<u8>) {
        // This adapter delivers argv, not a file: every launch goes through `try_render`.
        unreachable!("the muse hooks adapter writes no file: its argv comes from try_render")
    }

    fn try_render(
        &self,
        _workdir: &Path,
        params: &HookCommandParams<'_>,
    ) -> Result<RenderedHooks, SlotError> {
        if params.daemon_socket.is_none() {
            return Err(SlotError::NoDaemonSocket {
                adapter: MUSE_CONFIG_ARG.to_string(),
            });
        }
        let value = build_muse_hooks_config_value(params);
        // Compact: the value is one argv entry, and the launch splices it without quoting.
        Ok(RenderedHooks::Argv(vec![
            "-c".to_string(),
            format!("hooks={value}"),
        ]))
    }
}

/// The adapter of an agent that takes no lifecycle hooks (`#agent-tools 8/12`: OpenCode and Warp
/// on today's launch), named `unsupported`: a launch that passes it hook params is refused
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

    fn render(&self, _workdir: &Path, _params: &HookCommandParams<'_>) -> (PathBuf, Vec<u8>) {
        // This adapter has no hooks file to write, and `render` has no refusal to return: every
        // launch goes through `try_render`, which refuses the params.
        unreachable!(
            "the unsupported hooks adapter writes no file: its try_render refuses the params"
        )
    }

    fn try_render(
        &self,
        _workdir: &Path,
        _params: &HookCommandParams<'_>,
    ) -> Result<RenderedHooks, SlotError> {
        Err(SlotError::HooksUnsupported)
    }
}

/// The adapter named `name`; an unknown name is [`SlotError::UnknownHooksAdapter`].
pub fn adapter(name: &str) -> Result<&'static dyn HooksAdapter, SlotError> {
    match name {
        CLAUDE_SETTINGS_LOCAL => Ok(&ClaudeSettingsLocal),
        CURSOR_HOOKS_JSON => Ok(&CursorHooksJson),
        MUSE_CONFIG_ARG => Ok(&MuseConfigArg),
        UNSUPPORTED_HOOKS => Ok(&UnsupportedHooks),
        _ => Err(SlotError::UnknownHooksAdapter {
            name: name.to_string(),
        }),
    }
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
    fn the_muse_config_arg_name_resolves_to_its_adapter() {
        // When
        let resolved = adapter("muse-config-arg").map(|a| a.name());

        // Then
        assert_eq!(resolved, Ok(MUSE_CONFIG_ARG));
    }

    /// Muse hooks travel on the command line (`-c hooks=…`), not in a file: the render is argv,
    /// two entries whose second half is the hooks map as compact JSON.
    #[test]
    fn the_muse_adapter_renders_dash_c_hooks_argv() {
        // Given
        let params = HookCommandParams {
            tddy_tools_path: "/opt/tddy/tddy-tools",
            daemon_url: "http://127.0.0.1:8899",
            session_id: "0190-muse",
            os_user: "dev",
            hook_token: "a-hook-token",
            daemon_socket: Some("/var/folders/xy/T/tddy-agent-tools-1a2b.sock"),
            answer_wait_secs: 120,
        };

        // When
        let rendered = MuseConfigArg.try_render(Path::new("/work/session"), &params);

        // Then
        let Ok(RenderedHooks::Argv(argv)) = rendered else {
            panic!("the muse adapter must render argv, got: {rendered:?}");
        };
        assert_eq!(argv.len(), 2);
        assert_eq!(argv[0], "-c");
        let value = argv[1]
            .strip_prefix("hooks=")
            .expect("the second entry is hooks=<json>");
        let hooks: serde_json::Value = serde_json::from_str(value).expect("the -c value is JSON");
        assert_eq!(
            value,
            serde_json::to_string(&hooks).expect("compact JSON"),
            "the -c value is compact JSON (the hook commands inside still space their arguments)"
        );
        let hooks = hooks.as_object().expect("the -c value is the hooks map");
        let mut events: Vec<&str> = hooks.keys().map(String::as_str).collect();
        events.sort_unstable();
        assert_eq!(
            events,
            vec![
                "Notification",
                "PermissionRequest",
                "PostToolUse",
                "PreToolUse",
                "SessionEnd",
                "SessionStart",
                "Stop",
                "UserPromptSubmit",
            ]
        );
        assert_eq!(
            hooks["PreToolUse"][0]["matcher"].as_str(),
            Some("request_user_input")
        );
        let stop = hooks["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .expect("a command string");
        assert!(
            stop.contains("session-hook")
                && stop.contains("--session 0190-muse")
                && stop.contains("--daemon-socket"),
            "the hook reports over the agent-tool socket: {stop}"
        );
    }

    /// Params without a daemon socket are refused rather than rendered with the `daemon_url`
    /// fallback, as Cursor's are: on an embedded daemon every report to it is a swallowed 404.
    #[test]
    fn the_muse_adapter_refuses_params_without_a_daemon_socket() {
        // Given
        let params = HookCommandParams {
            tddy_tools_path: "/opt/tddy/tddy-tools",
            daemon_url: "http://127.0.0.1:8899",
            session_id: "0190-muse",
            os_user: "dev",
            hook_token: "a-hook-token",
            daemon_socket: None,
            answer_wait_secs: 120,
        };

        // When
        let refused = MuseConfigArg.try_render(Path::new("/work/session"), &params);

        // Then
        assert_eq!(
            refused,
            Err(SlotError::NoDaemonSocket {
                adapter: MUSE_CONFIG_ARG.to_string(),
            })
        );
    }

    /// The delivery each adapter declares: files for Claude and Cursor, argv for Muse —
    /// what a resume renders hooks for again.
    #[test]
    fn each_adapter_declares_how_it_delivers_its_hooks() {
        // When
        let deliveries = [
            adapter("claude-settings-local").unwrap().delivery(),
            adapter("cursor-hooks-json").unwrap().delivery(),
            adapter("muse-config-arg").unwrap().delivery(),
        ];

        // Then
        assert_eq!(
            deliveries,
            [
                HooksDelivery::File,
                HooksDelivery::File,
                HooksDelivery::Argv,
            ]
        );
    }

    /// The Muse adapter installs Claude's eight events, by name — what a spec's
    /// `conversation.rotation.hook` may name.
    #[test]
    fn the_muse_adapter_installs_the_eight_events() {
        // When
        let installed = MuseConfigArg.installed_events();

        // Then
        assert_eq!(
            installed,
            [
                "SessionStart",
                "UserPromptSubmit",
                "PostToolUse",
                "Notification",
                "PermissionRequest",
                "PreToolUse",
                "Stop",
                "SessionEnd",
            ]
        );
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
