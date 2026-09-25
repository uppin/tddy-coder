use crate::{cli_session_manager::CliSessionManager, connection_service::StackParentHost};

use std::path::PathBuf;

use crate::config::DaemonConfig;

use std::sync::Arc;

/// Whether a managed session running `recipe_name` binds a `spawn-conversation` handler on its
/// toolcall listener. Only the grill-me recipe does — a plain TDD session has nothing to hand off,
/// and the PR-stack orchestrator uses `spawn-child` (resolving a planned node) instead.
pub(crate) fn recipe_enables_conversation_spawn(recipe_name: &str) -> bool {
    recipe_name == "grill-me"
}

/// Derive a git-friendly branch slug from a free-form conversation prompt when the agent did not
/// supply an explicit `branch`. Lowercased, non-alphanumeric runs collapsed to a single `-`, and
/// truncated so the worktree branch name stays reasonable. Falls back to a stable label when the
/// prompt has no usable characters.
pub(crate) fn conversation_branch_slug(prompt: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in prompt.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "spawned-conversation".to_string()
    } else {
        format!("conversation/{trimmed}")
    }
}

/// Per-session [`ConversationSpawnHandler`] for a managed session (grill-me): spawns a brand-new
/// interactive claude-cli conversation on a fresh worktree, tagged with the calling session as its
/// orchestrator, reusing the same [`spawn_claude_cli_session_inner`] the `StartSession` RPC uses.
/// The generic sibling of [`StackChildSpawnHandler`] — it takes a free-form prompt instead of
/// resolving a planned PR-stack node id, and the spawned conversation is itself unmanaged.
pub(crate) struct GrillMeConversationSpawnHandler {
    /// Resolves each spawned conversation's base off the orchestrator, which is a session of *this*
    /// daemon. Held for the same reason [`StackChildSpawnHandler::stack_parent_host`] is.
    pub(crate) stack_parent_host: Arc<dyn StackParentHost>,
    pub(crate) config: DaemonConfig,
    pub(crate) tddy_data_dir: PathBuf,
    pub(crate) claude_cli_manager: Arc<CliSessionManager>,
    pub(crate) os_user: String,
    pub(crate) project_id: String,
    pub(crate) sessions_base: PathBuf,
    pub(crate) orchestrator_session_id: String,
    pub(crate) orchestrator_session_dir: PathBuf,
    /// Fallback model when the orchestrator session's metadata has none (a tddy-coder *tool*
    /// session writes `model: None` to its metadata, unlike a claude-cli session). The daemon knows
    /// the model at spawn time and supplies it here so `spawn_conversation` can still inherit one.
    pub(crate) model_override: Option<String>,
}
