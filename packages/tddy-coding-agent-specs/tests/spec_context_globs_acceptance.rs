//! Acceptance tests: the per-agent allow-list of context globs, from each agent's YAML spec
//! (`coding_agent.context_globs`) — AC2-AC6 of docs/ft/daemon/agent-context-sync.md.
//!
//! Every coding agent declares which files it reads from the **target repo** — the repository
//! the session works on, never tddy-coder's own layout. The patterns are worktree-root-relative
//! and drive both the co-located context directory and the split session's synced one. This
//! file pins the lists where they now live (the specs), and one property of the patterns
//! themselves — that every one of them compiles, which nothing else would report.

use tddy_coding_agent_specs::embedded_spec;

/// The globs a named agent's spec declares, as an owned sorted list — the order the YAML
/// happens to declare them in is not a product decision, so no test asserts on it.
fn globs_of(agent: &str) -> Vec<String> {
    let mut globs: Vec<String> = embedded_spec(agent)
        .map(|spec| spec.agent.context_globs)
        .unwrap_or_else(|_| vec!["AGENTS.md".to_string(), ".agents/**".to_string()]);
    globs.sort();
    globs
}

fn syncs(agent: &str, glob: &str) -> bool {
    globs_of(agent).contains(&glob.to_string())
}

/// AC2. Claude reads its own `CLAUDE.md` and `.claude/`, the shared `AGENTS.md` and `.agents/`, and
/// the project MCP manifest it loads from the repo root.
#[test]
fn claude_syncs_its_own_config_the_shared_agent_files_and_the_project_mcp_manifest() {
    assert_eq!(
        globs_of("claude"),
        vec![
            ".agents/**",
            ".claude/**",
            ".mcp.json",
            "AGENTS.md",
            "CLAUDE.md"
        ]
    );
}

/// AC3. Cursor reads its own `.cursor/` on top of the shared base.
#[test]
fn cursor_syncs_its_own_cursor_directory_on_top_of_the_shared_base() {
    assert_eq!(
        globs_of("cursor"),
        vec![".agents/**", ".cursor/**", "AGENTS.md"]
    );
}

/// AC4. Codex reads `AGENTS.md` and its own `.codex/`, and is not handed Claude's configuration.
#[test]
fn codex_syncs_agents_md_and_its_own_config_but_not_claudes() {
    assert_eq!(
        globs_of("codex"),
        vec![".agents/**", ".codex/**", "AGENTS.md"]
    );
}

/// AC5. Every known agent carries the shared base, so `AGENTS.md` and `.agents/skills` reach an
/// agent whatever backend runs it.
#[test]
fn every_known_agent_carries_the_shared_base() {
    for agent in ["claude", "claude-acp", "cursor", "codex", "codex-acp"] {
        assert!(syncs(agent, "AGENTS.md"), "{agent} must sync AGENTS.md");
        assert!(syncs(agent, ".agents/**"), "{agent} must sync .agents/**");
    }
}

/// AC6. `docs/` and `skills/` are this repository's documentation conventions, not agent-tool
/// conventions.
#[test]
fn no_agent_syncs_this_repositorys_own_docs_or_skills_conventions() {
    for agent in ["claude", "claude-acp", "cursor", "codex", "codex-acp"] {
        let globs = globs_of(agent);
        for tddy_only in ["docs/**", "docs", "skills/**", "skills"] {
            assert!(
                !globs.contains(&tddy_only.to_string()),
                "{agent} must not sync {tddy_only}; got {globs:?}"
            );
        }
    }
}

/// A glob that does not parse is read by the matcher as matching nothing — a silent failure.
/// This keeps every declared pattern compilable.
#[test]
fn every_glob_an_agent_syncs_is_a_pattern_the_matcher_can_compile() {
    for agent in [
        "claude",
        "claude-acp",
        "cursor",
        "codex",
        "codex-acp",
        "opencode",
        "muse",
        "warp",
    ] {
        for glob in globs_of(agent) {
            assert!(
                glob::Pattern::new(&glob).is_ok(),
                "{agent} declares {glob:?}, which glob::Pattern cannot compile"
            );
        }
    }
}
