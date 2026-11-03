use std::path::Path;

use tddy_core::cli_harness::{cli_harness, CliLaunch};

use super::CliSessionManager;

/// The argv the CLI harness registered under `session_type` builds for `launch`. The registry is the
/// one source of every CLI harness's argument list, so the session manager asks it rather than
/// keeping a copy.
fn registered_argv(session_type: &str, launch: &CliLaunch<'_>) -> Vec<String> {
    let harness = cli_harness(session_type).expect("the session type is a registered CLI harness");
    (harness.argv)(launch)
}

impl CliSessionManager {
    /// Build the argv for the `claude` process.
    ///
    /// Exported so tests can assert the argument list without spawning a real PTY.
    ///
    /// Arg order: `[binary, "--model", model, <session flag>, id, "--permission-mode", mode, prompt?]`.
    /// `model` is omitted when empty. The session flag is `--resume <id>` when `resume` is true
    /// (continue an existing on-disk transcript) and `--session-id <id>` otherwise (assign the id to
    /// a fresh session). `initial_prompt` is appended as a positional arg only when non-empty
    /// (trimmed); an empty/whitespace prompt is treated as absent so the process is started
    /// interactively without an injected first turn. `permission_mode` defaults to `"auto"` when
    /// `None` or empty/whitespace.
    ///
    /// The argv is the `claude-cli` harness descriptor's
    /// ([`tddy_core::cli_harness::cli_harness`]), which builds the base argv through the shared
    /// [`tddy_core::claude_argv::build_claude_base_argv`] so this path and the sandboxed runner stay
    /// in lockstep. A managed workflow's `--append-system-prompt-file` is inserted by
    /// [`Self::start_with_options`] (before any positional prompt), not here, so this builder stays
    /// focused on the base argv plus the positional prompt.
    pub fn build_claude_argv(
        binary_path: &str,
        model: &str,
        session_id: &str,
        initial_prompt: Option<&str>,
        permission_mode: Option<&str>,
        dangerously_skip_permissions: bool,
        resume: bool,
    ) -> Vec<String> {
        registered_argv(
            "claude-cli",
            &CliLaunch {
                binary: binary_path,
                model,
                session_id,
                initial_prompt,
                permission_mode,
                dangerously_skip_permissions,
                resume,
                ..CliLaunch::default()
            },
        )
    }

    /// The argv of a claude process as [`Self::start_with_options`] launches it: [`Self::build_claude_argv`],
    /// with `--append-system-prompt-file` and `extra_args` inserted before any positional prompt, so
    /// every flag precedes the prompt.
    ///
    /// Shared by the PTY launch and the tmux launch (`tddy_agent_launch::claude_cli_tmux`), so the
    /// two cannot build different argument lists for the same session.
    #[allow(clippy::too_many_arguments)]
    pub fn build_claude_cli_argv(
        binary_path: &str,
        model: &str,
        session_id: &str,
        initial_prompt: Option<&str>,
        permission_mode: Option<&str>,
        dangerously_skip_permissions: bool,
        resume: bool,
        append_system_prompt_file: Option<&Path>,
        extra_args: Vec<String>,
    ) -> Vec<String> {
        let mut argv = Self::build_claude_argv(
            binary_path,
            model,
            session_id,
            initial_prompt,
            permission_mode,
            dangerously_skip_permissions,
            resume,
        );
        // Insert before a trailing positional prompt (if any) so every flag precedes the prompt.
        let has_positional_prompt = initial_prompt.is_some_and(|p| !p.trim().is_empty());
        let insert_at = |argv: &Vec<String>| {
            if has_positional_prompt {
                argv.len() - 1
            } else {
                argv.len()
            }
        };
        if let Some(path) = append_system_prompt_file {
            let at = insert_at(&argv);
            argv.splice(
                at..at,
                [
                    "--append-system-prompt-file".to_string(),
                    path.to_string_lossy().into_owned(),
                ],
            );
        }
        if !extra_args.is_empty() {
            let at = insert_at(&argv);
            argv.splice(at..at, extra_args);
        }
        argv
    }

    /// Build the argv for the Cursor Agent CLI process.
    ///
    /// Arg order: `[binary, "--resume", chat_id, "--model", model, prompt?]`. `model` is omitted
    /// when empty. `initial_prompt` is appended as a positional arg when non-empty (trimmed).
    ///
    /// `chat_id` is the Cursor chat the session owns (`cursor-agent create-chat`); passing it keeps
    /// every spawn — first start and each resume — in the same chat. The `--resume` pair is emitted
    /// only when a non-empty id is given: `--resume` takes an *optional* argument, so a bare flag
    /// drops the CLI into an interactive chat picker that would wedge the PTY.
    ///
    /// The argv is the `cursor-cli` harness descriptor's ([`tddy_core::cli_harness::cli_harness`]).
    pub fn build_cursor_argv(
        binary_path: &str,
        model: &str,
        chat_id: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Vec<String> {
        registered_argv(
            "cursor-cli",
            &CliLaunch {
                binary: binary_path,
                model,
                conversation_id: chat_id,
                initial_prompt,
                ..CliLaunch::default()
            },
        )
    }
}
