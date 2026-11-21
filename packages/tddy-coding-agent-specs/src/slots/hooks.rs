//! The `hooks` slot: a named Rust adapter writes the agent's lifecycle-hooks configuration. The
//! hook protocol (`tddy-tools session-hook`, the PermissionRequest decision JSON) is not declarable.

use std::path::{Path, PathBuf};

pub use tddy_agent_backend::claude_hooks::session_file::LegacyRelease;
use tddy_agent_backend::claude_hooks::session_file::{
    claude_session_hooks_file, release_legacy_claude_settings, LEGACY_CLAUDE_SETTINGS_FILE,
};
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

    /// How this adapter's hooks reach the agent. Every launch — a start and each resume — renders
    /// them again, whichever way they are delivered: the socket they report over is the running
    /// daemon's, and a file left from an earlier launch may name a daemon that has since exited.
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

    /// The argv entries that load the hooks file this adapter rendered at `hooks_file`: the load
    /// flag and the path, for an agent that does not find the file by itself. Empty by default:
    /// the agent finds the file where the launch writes it.
    fn file_load_args(&self, _hooks_file: &Path) -> Vec<String> {
        Vec::new()
    }

    /// A file under `workdir` that an earlier build of this adapter wrote every session's hooks
    /// into, shared by all the sessions working there. A launch takes the daemon's entries out of
    /// it ([`Self::release_legacy_shared_file`]) before its agent starts, or the agent would load
    /// them beside its own file. `None` by default: the adapter never wrote one.
    fn legacy_shared_file(&self, _workdir: &Path) -> Option<PathBuf> {
        None
    }

    /// What becomes of [`Self::legacy_shared_file`], holding `bytes`, once the daemon's entries are
    /// out of it. [`LegacyRelease::Keep`] by default.
    fn release_legacy_shared_file(&self, _bytes: &[u8]) -> LegacyRelease {
        LegacyRelease::Keep
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

/// Claude Code's hooks, one file per session — `<workdir>/.tddy/hooks/<session_id>/claude-settings.json`
/// ([`claude_session_hooks_file`]) — built by `tddy_agent_backend::build_claude_hooks_settings` and
/// loaded with `--settings`.
///
/// The name is the adapter's historical one: its file used to be `<workdir>/.claude/settings.local.json`,
/// one per worktree, so a second session started in the same worktree took the first one's hooks
/// (reported 2026-11-21). The daemon's entries left in that file are released on every launch
/// ([`HooksAdapter::legacy_shared_file`]).
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

    /// Claude Code reads no file under `.tddy/`, so the session's file is loaded explicitly.
    fn file_load_args(&self, hooks_file: &Path) -> Vec<String> {
        vec![
            "--settings".to_string(),
            hooks_file.to_string_lossy().into_owned(),
        ]
    }

    fn legacy_shared_file(&self, workdir: &Path) -> Option<PathBuf> {
        Some(workdir.join(LEGACY_CLAUDE_SETTINGS_FILE))
    }

    fn release_legacy_shared_file(&self, bytes: &[u8]) -> LegacyRelease {
        release_legacy_claude_settings(bytes)
    }

    fn render(&self, workdir: &Path, params: &HookCommandParams<'_>) -> (PathBuf, Vec<u8>) {
        let settings = build_claude_hooks_settings(params);
        // `{:#}` is `serde_json`'s pretty form, and unlike `to_string_pretty` cannot fail.
        (
            claude_session_hooks_file(workdir, params.session_id),
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
        if params.remote_log.is_none() && params.daemon_socket.is_none() {
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
            remote_log: None,
            route_env: &[],
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
            remote_log: None,
            route_env: &[],
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
            remote_log: None,
            route_env: &[],
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
