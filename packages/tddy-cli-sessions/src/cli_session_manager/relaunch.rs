use crate::cli_session_manager::pty_handle;

use super::CliSessionManager;

use std::path::Path;

use std::sync::Arc;

use std::path::PathBuf;

impl CliSessionManager {
    /// Resume (relaunch) an existing plain session by spawning a new process in the same worktree.
    ///
    /// Always passes `initial_prompt = None`: a resumed session continues via the resume argv its caller rendered
    /// and must not replay the original prompt (that would inject a duplicate user turn).
    pub async fn resume(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        argv: Vec<String>,
        session_type: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        self.resume_with_options(
            session_id,
            worktree_path,
            model,
            argv,
            session_type,
            None,
            Vec::new(),
            Vec::new(),
        )
        .await
    }

    /// Resume with managed-workflow options: like [`Self::resume`] but re-wires the orchestration
    /// prompt (`append_system_prompt_file`) and per-session env (`TDDY_SOCKET` + `PATH`) so a resumed
    /// managed session stays workflow-aware. Never replays the initial prompt and never carries over
    /// a prior permission mode (resume uses the default "auto").
    ///
    /// `extra_args` re-supplies the flags a spawn injected but nothing persisted — a split session's
    /// tool allowlist and `--mcp-config`, without which a resumed agent would come back with no way
    /// to reach its worktree at all.
    #[allow(clippy::too_many_arguments)]
    pub async fn resume_with_options(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        argv: Vec<String>,
        session_type: &str,
        append_system_prompt_file: Option<&Path>,
        extra_args: Vec<String>,
        env: Vec<(String, String)>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        self.start_with_options(
            session_id,
            worktree_path,
            model,
            argv,
            session_type,
            None,
            append_system_prompt_file,
            extra_args,
            env,
            None,
        )
        .await
    }
}
