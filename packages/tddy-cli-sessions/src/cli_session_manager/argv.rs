use super::CliSessionManager;

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
    /// The base argv (binary, model, session flag, permission mode) is built by the shared
    /// [`tddy_core::claude_argv::build_claude_base_argv`] so this path and the sandboxed runner
    /// stay in lockstep. A managed workflow's `--append-system-prompt-file` is inserted by
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
        let mode = permission_mode
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("auto");
        let mut argv = tddy_core::claude_argv::build_claude_base_argv(
            binary_path,
            model,
            session_id,
            mode,
            dangerously_skip_permissions,
            resume,
        );
        if let Some(p) = initial_prompt {
            let p = p.trim();
            if !p.is_empty() {
                argv.push(p.to_string());
            }
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
    pub fn build_cursor_argv(
        binary_path: &str,
        model: &str,
        chat_id: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Vec<String> {
        let mut argv = vec![binary_path.to_string()];
        if let Some(chat_id) = chat_id.map(str::trim).filter(|id| !id.is_empty()) {
            argv.push("--resume".to_string());
            argv.push(chat_id.to_string());
        }
        if !model.is_empty() {
            argv.push("--model".to_string());
            argv.push(model.to_string());
        }
        if let Some(p) = initial_prompt {
            let p = p.trim();
            if !p.is_empty() {
                argv.push(p.to_string());
            }
        }
        argv
    }
}
