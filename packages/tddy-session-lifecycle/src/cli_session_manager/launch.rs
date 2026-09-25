use crate::cli_session_manager::pty_handle;

use super::CliSessionManager;

use std::path::Path;

use super::MAIN_TERMINAL_ID;

use std::sync::Arc;

use std::path::PathBuf;

impl CliSessionManager {
    /// Spawn a new Cursor Agent CLI process for `session_id` in `worktree_path`.
    ///
    /// `chat_id` — the Cursor chat the session owns, launched into via `--resume <chat_id>`.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_cursor(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
        chat_id: Option<&str>,
        initial_prompt: Option<&str>,
        // Extra per-session env pairs applied to the cursor process (e.g. `TDDY_SEMANTIC_INDEX_DB`).
        env: Vec<(String, String)>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let argv = Self::build_cursor_argv(binary_path, model, chat_id, initial_prompt);
        self.spawn_tool(
            session_id,
            MAIN_TERMINAL_ID,
            "cursor-cli",
            worktree_path,
            model,
            argv,
            env,
            None,
        )
        .await
    }

    /// Resume a Cursor CLI session into `chat_id` (never replays the initial prompt).
    pub async fn resume_cursor(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
        chat_id: Option<&str>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        self.start_cursor(
            session_id,
            worktree_path,
            model,
            binary_path,
            chat_id,
            None,
            Vec::new(),
        )
        .await
    }

    /// Spawn a new claude CLI process for `session_id` in `worktree_path`.
    ///
    /// Returns an `Arc<PtyHandle>` on success. The child process is monitored in a background
    /// std thread; when it exits the session is removed from the registry.
    ///
    /// `initial_prompt` — when `Some` and non-empty, appended as a positional CLI argument so
    /// that `claude` receives it as the first user turn. Pass `None` (or `Some("")`) for an
    /// interactive session with no seeded prompt. **Resume** (`resume()`) always passes `None`
    /// because the session is continued via `--session-id`; re-injecting the original prompt
    /// would create a duplicate user turn.
    ///
    /// `permission_mode` — forwarded as `--permission-mode <mode>` to the claude binary.
    /// `None` or empty/whitespace defaults to `"auto"`.
    pub async fn start(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
        initial_prompt: Option<&str>,
        permission_mode: Option<&str>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        self.start_with_options(
            session_id,
            worktree_path,
            model,
            binary_path,
            initial_prompt,
            permission_mode,
            false,
            false,
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
        .await
    }

    /// Like [`Self::start`], plus managed-workflow launch options.
    ///
    /// `resume` — when true, continue the existing on-disk transcript via `--resume <id>` instead of
    /// assigning the id to a fresh session via `--session-id <id>`. Only [`Self::resume_with_options`]
    /// passes true; fresh starts pass false.
    ///
    /// `append_system_prompt_file` — when `Some`, inserted as `--append-system-prompt-file <path>`
    /// (before any positional prompt) so `claude` appends that file to its system prompt (a managed
    /// workflow's orchestration prompt).
    ///
    /// `env` — extra environment variables set on the spawned process (e.g. a managed session's
    /// per-session `TDDY_SOCKET` and a `PATH` that resolves `tddy-tools`).
    ///
    /// `extra_args` — flags appended before any positional prompt (a split session's tool allowlist
    /// and `--mcp-config`). They go before the prompt because `--mcp-config` is variadic and would
    /// otherwise swallow a bare positional as another config path.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_with_options(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        binary_path: &str,
        initial_prompt: Option<&str>,
        permission_mode: Option<&str>,
        dangerously_skip_permissions: bool,
        resume: bool,
        append_system_prompt_file: Option<&Path>,
        extra_args: Vec<String>,
        env: Vec<(String, String)>,
        os_user: Option<&str>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
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
        self.spawn_tool(
            session_id,
            MAIN_TERMINAL_ID,
            "claude-cli",
            worktree_path,
            model,
            argv,
            env,
            os_user,
        )
        .await
    }
}
