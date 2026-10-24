//! Which context files of the target repo each agent reads.

/// Context files every agent reads, whatever tool runs it: the cross-tool `AGENTS.md` convention
/// and the `.agents/` tree beside it.
const SHARED_CONTEXT_GLOBS: &[&str] = &["AGENTS.md", ".agents/**"];

/// Worktree-root-relative globs naming the context files an agent reads from the **target repo** —
/// the repository a session works on, never tddy-coder's own layout. Single source of truth for
/// what a co-located context directory holds and what a split session syncs to its peer; sibling of
/// [`curated_models_for_agent`](super::curated_models_for_agent), which answers the same question for models.
///
/// An unrecognised agent gets [`SHARED_CONTEXT_GLOBS`] alone. That is a deliberate *narrowing*: an
/// unknown name can only ever sync less than a known backend does. Widening it — returning the
/// union of every list — would hand `.claude/` and its permissions to an agent nobody vetted, which
/// is precisely what a compiled-in allow-list exists to prevent.
///
/// # `.claude/settings.local.json` is deliberately not synced
///
/// `.claude/**` names it, and it is nevertheless withheld — by `tddy_sandbox`'s
/// `CONTEXT_EXCLUDE_GLOBS`, which every matcher on both halves consults after this table. Do not
/// "fix" the omission by narrowing the pattern here or adding the path anywhere: on a managed
/// session **the daemon owns that file**. `write_claude_hooks_settings`
/// (`tddy_daemon::connection_service`) writes the six Claude Code hooks that report the session's
/// status into exactly that path in the agent's working directory, as a whole-file atomic replace
/// with no merge. Syncing the repository's copy into the same directory only decides which of the
/// two writes lands last: at spawn the hooks win and the synced bytes were pointless, and on a
/// later re-sync the repo's copy wins and status reporting dies silently for the rest of the
/// session. Neither outcome is guidance reaching an agent.
///
/// The exclusion lives in `tddy-sandbox` rather than here because that is where matching happens —
/// one predicate for the manifest walk, the copier, the daemon's reader and the syncer's deletes —
/// and a second list consulted by only some of them is how a manifest comes to advertise a path the
/// reader then refuses. (`tddy-sandbox` does not depend on `tddy-core`, and must not start to.)
#[must_use]
pub fn context_globs_for_agent(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" | "claude-acp" => &[
            "AGENTS.md",
            ".agents/**",
            "CLAUDE.md",
            ".claude/**",
            ".mcp.json",
        ],
        // Cursor honours Claude's configuration as well as its own, so it reads both trees.
        "cursor" => &[
            "AGENTS.md",
            ".agents/**",
            "CLAUDE.md",
            ".claude/**",
            ".cursor/**",
            ".mcp.json",
        ],
        "codex" | "codex-acp" => &["AGENTS.md", ".agents/**", ".codex/**"],
        // OpenCode's project configuration lives in `opencode.json` and the `.opencode/` tree.
        "opencode" => &["AGENTS.md", ".agents/**", "opencode.json", ".opencode/**"],
        // Muse Code's `.muse/` tree is unconfirmed: TODO(muse-cli-facts) in `cli_harness_muse`.
        "muse" => &["AGENTS.md", ".agents/**", ".muse/**"],
        // Warp reads its own `WARP.md` beside the shared base; its skills come from `.agents/`.
        "warp" => &["AGENTS.md", ".agents/**", "WARP.md"],
        _ => SHARED_CONTEXT_GLOBS,
    }
}
