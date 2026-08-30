//! Acceptance tests: the per-backend allow-list of context globs — AC1-AC6.
//!
//! Every coding backend declares which files it reads from the **target repo** — the repository the
//! session works on, never tddy-coder's own layout. The patterns are worktree-root-relative and
//! drive both the co-located context directory and the split session's synced one.
//!
//! Deliberately not under test here: whether the globs *match* anything (that is tddy-sandbox's
//! matcher) and whether the files reach an agent (that is the syncer). This file pins the table,
//! the trait default, and one property of the patterns themselves — that every one of them
//! compiles, which nothing else would report.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md § Acceptance Criteria.

use tddy_core::backend::{context_globs_for_agent, CodingBackend, StubBackend};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The globs a named agent syncs, as an owned sorted list — the order the table happens to declare
/// them in is not a product decision, so no test asserts on it.
fn globs_of(agent: &str) -> Vec<String> {
    let mut globs: Vec<String> = context_globs_for_agent(agent)
        .iter()
        .map(|g| (*g).to_string())
        .collect();
    globs.sort();
    globs
}

fn syncs(agent: &str, glob: &str) -> bool {
    context_globs_for_agent(agent).contains(&glob)
}

// ---------------------------------------------------------------------------
// AC1 — the trait method defaults to the table
// ---------------------------------------------------------------------------

/// AC1. A backend that overrides nothing still reports its own agent's globs, so a caller holding a
/// `CodingBackend` and a caller holding only an agent name read the same table.
#[test]
fn a_backend_that_overrides_nothing_reports_the_globs_of_its_own_name() {
    // Given
    let backend = StubBackend::default();

    // When
    let from_trait = backend.context_globs();
    let from_table = context_globs_for_agent(backend.name());

    // Then
    assert_eq!(
        from_trait, from_table,
        "the trait default must resolve through context_globs_for_agent(self.name())"
    );
}

// ---------------------------------------------------------------------------
// AC2-AC4 — the per-agent lists
// ---------------------------------------------------------------------------

/// AC2. Claude reads its own `CLAUDE.md` and `.claude/`, the shared `AGENTS.md` and `.agents/`, and
/// the project MCP manifest it loads from the repo root.
#[test]
fn claude_syncs_its_own_config_the_shared_agent_files_and_the_project_mcp_manifest() {
    // When
    let globs = globs_of("claude");

    // Then
    assert_eq!(
        globs,
        vec![
            ".agents/**",
            ".claude/**",
            ".mcp.json",
            "AGENTS.md",
            "CLAUDE.md",
        ]
    );
}

/// AC3. Cursor reads everything Claude does — it honours `CLAUDE.md` and `.claude/` — and its own
/// `.cursor/` on top, which no backend syncs today.
#[test]
fn cursor_syncs_everything_claude_does_plus_its_own_cursor_directory() {
    // When
    let cursor = globs_of("cursor");
    let claude = globs_of("claude");

    // Then
    for glob in &claude {
        assert!(
            cursor.contains(glob),
            "cursor must sync {glob}, which claude syncs; cursor has {cursor:?}"
        );
    }
    assert!(
        cursor.contains(&".cursor/**".to_string()),
        "cursor must sync its own .cursor/ tree; got {cursor:?}"
    );
}

/// AC4. Codex reads `AGENTS.md` and its own `.codex/`, and is not handed Claude's configuration —
/// narrowing per backend is the point of the table.
#[test]
fn codex_syncs_agents_md_and_its_own_config_but_not_claudes() {
    // When
    let globs = globs_of("codex");

    // Then
    assert_eq!(globs, vec![".agents/**", ".codex/**", "AGENTS.md"]);
}

// ---------------------------------------------------------------------------
// AC5 — the default narrows, never widens
// ---------------------------------------------------------------------------

/// AC5. An agent the table does not know gets the shared base. The default must *narrow*: an
/// unrecognised name that received the union of every backend's list would sync `.claude/` to an
/// agent nobody vetted, which is the widening this compiled-in table exists to prevent.
#[test]
fn an_unrecognised_agent_gets_the_shared_base_rather_than_the_union_of_every_list() {
    // When
    let unknown = globs_of("some-agent-that-does-not-exist");

    // Then
    assert_eq!(unknown, vec![".agents/**", "AGENTS.md"]);
    for widening in [".claude/**", ".cursor/**", ".codex/**", "CLAUDE.md"] {
        assert!(
            !unknown.contains(&widening.to_string()),
            "an unrecognised agent must not receive {widening}; got {unknown:?}"
        );
    }
}

/// AC5. Every known agent carries the shared base, so `AGENTS.md` and `.agents/skills` reach an
/// agent whatever backend runs it.
#[rstest::rstest]
#[case("claude")]
#[case("claude-acp")]
#[case("cursor")]
#[case("codex")]
#[case("codex-acp")]
fn every_known_agent_carries_the_shared_base(#[case] agent: &str) {
    // Then
    assert!(
        syncs(agent, "AGENTS.md"),
        "{agent} must sync AGENTS.md: {:?}",
        globs_of(agent)
    );
    assert!(
        syncs(agent, ".agents/**"),
        "{agent} must sync .agents/**: {:?}",
        globs_of(agent)
    );
}

// ---------------------------------------------------------------------------
// AC6 — tddy-coder's own conventions are gone
// ---------------------------------------------------------------------------

/// AC6. `docs/` and `skills/` are this repository's documentation conventions, not agent-tool
/// conventions. Syncing them dragged an arbitrary target repo's whole prose tree into the context
/// directory — and across the peer link on the split path.
#[rstest::rstest]
#[case("claude")]
#[case("claude-acp")]
#[case("cursor")]
#[case("codex")]
#[case("codex-acp")]
#[case("an-unrecognised-agent")]
fn no_agent_syncs_this_repositorys_own_docs_or_skills_conventions(#[case] agent: &str) {
    // When
    let globs = globs_of(agent);

    // Then
    for tddy_only in ["docs/**", "docs", "skills/**", "skills"] {
        assert!(
            !globs.contains(&tddy_only.to_string()),
            "{agent} must not sync {tddy_only} — a target-repo path this project invented; got {globs:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Every compiled-in pattern parses
// ---------------------------------------------------------------------------

/// A glob that does not parse is not a loud failure anywhere: `tddy_sandbox::matches_context_globs`
/// reads it as matching nothing, which is the safe reading but also a silent one. A typo'd
/// `.claude/**` would therefore switch off that entire tree — on the manifest half and the reader
/// half at once — leaving an agent quietly working without its project's rules. The matcher now
/// logs it; this is what keeps it from ever being logged.
///
/// The unrecognised name is a case rather than an afterthought: it selects the shared base, the one
/// row every session falls back to.
#[rstest::rstest]
#[case("claude")]
#[case("claude-acp")]
#[case("cursor")]
#[case("codex")]
#[case("codex-acp")]
#[case("stub")]
#[case("an-unrecognised-agent")]
fn every_glob_an_agent_syncs_is_a_pattern_the_matcher_can_compile(#[case] agent: &str) {
    // When
    let globs = context_globs_for_agent(agent);

    // Then
    for glob in globs {
        assert!(
            glob::Pattern::new(glob).is_ok(),
            "{agent} declares {glob:?}, which glob::Pattern cannot compile — the matcher would \
             read it as naming nothing and sync none of the tree behind it"
        );
    }
}

/// The exclusion list is compiled by the same matcher and carries the same risk in reverse: an
/// unparsable exclusion silently stops excluding, and `.claude/settings.local.json` starts fighting
/// the hooks file the daemon writes there.
///
/// Checked from here, beside the positive table, because the two are one allow-list read in two
/// steps and a reader looking for "what does an agent sync" must find both answers in one place.
#[test]
fn every_pattern_the_exclusion_list_withholds_is_one_the_matcher_can_compile() {
    // Then
    for glob in tddy_sandbox::CONTEXT_EXCLUDE_GLOBS {
        assert!(
            glob::Pattern::new(glob).is_ok(),
            "the context exclusion list declares {glob:?}, which glob::Pattern cannot compile — it \
             would stop excluding and nothing would say so"
        );
    }
}
