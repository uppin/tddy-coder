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
    // The split agent's launch (`start_with_options`) is its last caller: Telegram's hand-built
    // `start` went with `#agent-tools 18/19`.
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

    /// `argv` — a [`Self::build_claude_argv`] result for `initial_prompt` — with
    /// `--append-system-prompt-file` and `extra_args` inserted before any positional prompt, so every
    /// flag precedes the prompt. This is the argv [`Self::start_with_options`] launches.
    // The split agent's launch (`start_with_options`) is its last caller: Telegram's hand-built
    // `start` went with `#agent-tools 18/19`.
    pub fn with_claude_launch_flags(
        mut argv: Vec<String>,
        initial_prompt: Option<&str>,
        append_system_prompt_file: Option<&Path>,
        extra_args: Vec<String>,
    ) -> Vec<String> {
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
}
