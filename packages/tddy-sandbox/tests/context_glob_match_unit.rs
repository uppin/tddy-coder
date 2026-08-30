//! Unit tests: the glob matcher that decides whether a worktree path is context.
//!
//! One predicate governs everything downstream — what is copied, what the manifest lists, and what
//! the reader will serve. A pattern that matched one path too many would hand an agent a file the
//! backend never asked for; one that matched one too few would leave it working blind. Both are
//! silent, so the edges are pinned here rather than inferred from the acceptance tests.
//!
//! Patterns are always **worktree-root-relative**, so a leading `/` or a `..` is a bug in the
//! caller, not a path to resolve.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md § Design.

use tddy_sandbox::matches_context_globs;

const CLAUDE_GLOBS: &[&str] = &[
    "CLAUDE.md",
    "AGENTS.md",
    ".claude/**",
    ".mcp.json",
    ".agents/**",
];

// ---------------------------------------------------------------------------
// Exact names
// ---------------------------------------------------------------------------

#[test]
fn a_pattern_without_wildcards_matches_only_that_exact_path() {
    // Then
    assert!(matches_context_globs("CLAUDE.md", CLAUDE_GLOBS));
    assert!(matches_context_globs(".mcp.json", CLAUDE_GLOBS));
}

/// A root-level pattern names a file at the root, not the same basename buried in the tree. A
/// vendored `docs/CLAUDE.md` is documentation about the tool, not instructions to the agent.
#[test]
fn a_root_level_pattern_does_not_match_the_same_basename_deeper_in_the_tree() {
    // Then
    assert!(!matches_context_globs("docs/CLAUDE.md", CLAUDE_GLOBS));
    assert!(!matches_context_globs(
        "vendor/some-crate/CLAUDE.md",
        CLAUDE_GLOBS
    ));
}

/// Prefix matching is by path segment, not by string. `.claude/**` must not swallow a sibling
/// directory whose name merely starts the same way.
#[test]
fn a_directory_pattern_does_not_match_a_sibling_whose_name_merely_starts_the_same() {
    // Then
    assert!(!matches_context_globs(
        ".claudex/settings.json",
        CLAUDE_GLOBS
    ));
    assert!(!matches_context_globs("CLAUDE.md.bak", CLAUDE_GLOBS));
    assert!(!matches_context_globs(".mcp.json.tmp", CLAUDE_GLOBS));
}

// ---------------------------------------------------------------------------
// `**` depth
// ---------------------------------------------------------------------------

#[test]
fn a_double_star_matches_a_file_directly_inside_the_directory() {
    // Then
    assert!(matches_context_globs(".claude/settings.json", CLAUDE_GLOBS));
}

#[test]
fn a_double_star_matches_a_file_many_levels_down() {
    // Then
    assert!(matches_context_globs(
        ".claude/skills/fluent-tests/references/rust/std-test.md",
        CLAUDE_GLOBS
    ));
}

/// The directory itself is not a file to copy. Callers feed this predicate file paths, and a bare
/// `.claude` answering true would have the copier try to read a directory as a file.
#[test]
fn a_double_star_does_not_match_the_bare_directory_name() {
    // Then
    assert!(!matches_context_globs(".claude", CLAUDE_GLOBS));
}

// ---------------------------------------------------------------------------
// Nothing outside the list
// ---------------------------------------------------------------------------

#[rstest::rstest]
#[case("README.md")]
#[case("src/main.rs")]
#[case("docs/architecture/overview.md")]
#[case("skills/whatever.md")]
#[case(".cursor/rules/style.mdc")]
#[case(".env")]
#[case(".git/config")]
#[case("package.json")]
fn a_path_no_pattern_names_does_not_match(#[case] rel_path: &str) {
    // Then
    assert!(
        !matches_context_globs(rel_path, CLAUDE_GLOBS),
        "{rel_path} must not match claude's globs"
    );
}

/// `.env` sitting *inside* an allow-listed tree is still allow-listed — the allow-list is the whole
/// gate, so a backend's own directory is trusted wholesale. Pinned because it is the one place a
/// reader of the security argument might expect an extra exclusion, and there is none: the
/// protection is that no caller can name `.env` at the root, not that `.env` is a magic word.
#[test]
fn a_dotenv_inside_an_allow_listed_tree_matches_because_the_tree_is_trusted() {
    // Then
    assert!(matches_context_globs(".claude/.env", CLAUDE_GLOBS));
    assert!(!matches_context_globs(".env", CLAUDE_GLOBS));
}

// ---------------------------------------------------------------------------
// Degenerate input
// ---------------------------------------------------------------------------

#[test]
fn an_empty_glob_list_matches_nothing() {
    // Then
    assert!(!matches_context_globs("CLAUDE.md", &[]));
    assert!(!matches_context_globs(".claude/settings.json", &[]));
}

#[test]
fn an_empty_path_matches_nothing() {
    // Then
    assert!(!matches_context_globs("", CLAUDE_GLOBS));
}

/// A path that escapes the root can never be context, whatever it looks like. The reader refuses
/// these separately, but the predicate must not be the thing that lets one through.
#[rstest::rstest]
#[case("../CLAUDE.md")]
#[case("../../.claude/settings.json")]
#[case(".claude/../../CLAUDE.md")]
#[case("/CLAUDE.md")]
#[case("/etc/passwd")]
fn a_traversing_or_absolute_path_never_matches(#[case] rel_path: &str) {
    // Then
    assert!(
        !matches_context_globs(rel_path, CLAUDE_GLOBS),
        "{rel_path} escapes the worktree root and must never be treated as context"
    );
}

/// Matching is case-sensitive, because the filesystems this runs on that are not
/// (`darwin`) still record the case the repository committed, and `claude.md` is not the file
/// Claude Code reads.
#[test]
fn matching_is_case_sensitive() {
    // Then
    assert!(!matches_context_globs("claude.md", CLAUDE_GLOBS));
    assert!(!matches_context_globs("Agents.md", CLAUDE_GLOBS));
}

/// A caller that spells a path with a redundant `./` prefix gets the same answer — the predicate
/// normalizes rather than refusing, because both spellings name the same file and a mismatch here
/// would show up as a file that silently stops syncing.
#[test]
fn a_redundant_leading_dot_slash_is_normalized_away() {
    // Then
    assert!(matches_context_globs("./CLAUDE.md", CLAUDE_GLOBS));
    assert!(matches_context_globs(
        "./.claude/settings.json",
        CLAUDE_GLOBS
    ));
}

/// Windows-style separators normalize to `/`, matching how `worktree_files` already treats an
/// incoming `rel_path`.
#[test]
fn backslash_separators_normalize_to_forward_slashes() {
    // Then
    assert!(matches_context_globs(
        ".claude\\settings.json",
        CLAUDE_GLOBS
    ));
}

// ---------------------------------------------------------------------------
// The exclusion list
// ---------------------------------------------------------------------------

/// `.claude/settings.local.json` is named by `.claude/**` and withheld anyway, because on a managed
/// session the daemon writes its Claude Code hooks configuration to exactly that path in the
/// agent's working directory, as a whole-file replace. Syncing the repository's copy there only
/// decides which write lands last — and when the repo's copy wins, the session's status reporting
/// stops with nothing saying so.
///
/// The refusal is in the predicate rather than in a caller so that both halves — the manifest walk
/// and the daemon's reader — inherit it from one place.
#[test]
fn the_settings_file_the_daemon_writes_is_withheld_though_the_allow_list_names_it() {
    // Then
    assert!(
        !matches_context_globs(".claude/settings.local.json", CLAUDE_GLOBS),
        "the daemon owns .claude/settings.local.json on a managed session; syncing the repo's copy \
         would overwrite the hooks it writes there"
    );
}

/// Only that exact path. The exclusion is a scalpel: the rest of `.claude/` — including the
/// *shared* `settings.json`, which the daemon never writes — still syncs, or the exclusion would
/// cost the agent its project's Claude configuration wholesale.
#[rstest::rstest]
#[case(".claude/settings.json")]
#[case(".claude/settings.local.json.bak")]
#[case(".claude/nested/settings.local.json")]
fn the_exclusion_withholds_that_one_path_and_nothing_adjacent(#[case] rel_path: &str) {
    // Then
    assert!(
        matches_context_globs(rel_path, CLAUDE_GLOBS),
        "{rel_path} is not the file the daemon owns and must still sync"
    );
}

/// The exclusion is checked on the *normalized* path, so a caller cannot spell its way past it —
/// the reader and the manifest would otherwise disagree about the same file the moment one of them
/// used the other's spelling.
#[rstest::rstest]
#[case("./.claude/settings.local.json")]
#[case(".claude\\settings.local.json")]
#[case(".claude/./settings.local.json")]
fn an_alternate_spelling_of_the_excluded_path_is_withheld_too(#[case] rel_path: &str) {
    // Then
    assert!(
        !matches_context_globs(rel_path, CLAUDE_GLOBS),
        "{rel_path} spells the excluded path and must be withheld"
    );
}

/// The exclusion is matched **case-insensitively**, and it is the only half of the predicate that
/// is. On macOS and Windows the filesystem is case-insensitive, so `.claude/Settings.local.json`
/// and `.claude/settings.local.json` open the same bytes — a case-sensitive exclusion is then a
/// one-keystroke bypass of the rule that keeps the sync from racing the daemon's own hooks writer,
/// and the reader hands back the hooks file after all.
///
/// The asymmetry with the case-*sensitive* allow-list above is the point: an allow-list may
/// under-match safely (the agent reads less guidance than the project ships), while an exclusion
/// may not (the agent reads the one file that was withheld).
#[rstest::rstest]
#[case(".claude/Settings.local.json")]
#[case(".claude/SETTINGS.LOCAL.JSON")]
#[case("./.claude/Settings.Local.Json")]
fn a_differently_cased_spelling_of_the_excluded_path_is_withheld_too(#[case] rel_path: &str) {
    // Then
    assert!(
        !matches_context_globs(rel_path, CLAUDE_GLOBS),
        "{rel_path} opens the file the daemon owns on a case-insensitive filesystem and must be \
         withheld"
    );
}
