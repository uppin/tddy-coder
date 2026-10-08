/// Where a session's worktree comes from. A local client (e.g. tddy-sandbox-app) may send an explicit
/// `repo_path` to use directly; otherwise the worktree is resolved from a registered `project_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeSource {
    /// Use this local checkout path directly (client-supplied).
    RepoPath(std::path::PathBuf),
    /// Resolve from the registered project id.
    Project(String),
}

/// Pure: choose the worktree source for a session from the request's `repo_path` / `project_id`.
/// A non-empty `repo_path` wins (local-client path); otherwise fall back to `project_id`.
pub fn session_worktree_source(repo_path: &str, project_id: &str) -> WorktreeSource {
    if repo_path.is_empty() {
        WorktreeSource::Project(project_id.to_string())
    } else {
        WorktreeSource::RepoPath(std::path::PathBuf::from(repo_path))
    }
}

/// Pure: assemble the pass-through argument tokens forwarded to the in-jail `claude` for a
/// sandboxed session, in the order `claude` must receive them.
///
/// Client-supplied `claude_args` come first, verbatim (e.g. `--add-dir /foo`). A non-empty
/// `initial_prompt` is appended last as a trailing positional, so it lands as the first user turn
/// even when extra flags precede it; an empty/whitespace prompt is omitted. The runner wraps each
/// returned token in a `--claude-arg` occurrence and inserts them after `claude`'s fixed flags and
/// before the MCP allowlist args (see `SpawnClaudePtyParams::claude_args`), which keeps a trailing
/// positional a positional instead of being swallowed by the variadic `--mcp-config`.
pub fn sandbox_claude_passthrough_args(
    claude_args: &[String],
    initial_prompt: &str,
) -> Vec<String> {
    let mut out: Vec<String> = claude_args.to_vec();
    let prompt = initial_prompt.trim();
    if !prompt.is_empty() {
        out.push(prompt.to_string());
    }
    out
}
